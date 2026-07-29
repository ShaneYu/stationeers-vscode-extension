using System.Collections.Generic;
using System.Linq;
using System.Security.Cryptography;
using System.Text;

namespace StationeersBridge.Relay.Core;

public sealed class RelayAuthorityOptions
{
    public RelayLimits Limits { get; init; } = new(8192, 8, 64, TimeSpan.FromSeconds(30));
    public int WorkerCount { get; init; } = 1;
    public int MaxIdempotencyEntries { get; init; } = 256;
}

public sealed class RelayAuthorityService : IDisposable
{
    private readonly record struct IdempotencyKey(string PlayerId, string SessionId, string Value);
    private sealed record WorkItem(
        AuthenticatedPlayer Player,
        RelayRpcEnvelope Request,
        IdempotencyKey Key,
        TaskCompletionSource<RelayResponse> Completion,
        CancellationToken CancellationToken);
    private sealed record IdempotencyEntry(string Fingerprint, TaskCompletionSource<RelayResponse> Completion);

    private readonly RelayAuthorityOptions _options;
    private readonly IRelayPolicyResolver _policies;
    private readonly IRelayRevocationState _revocations;
    private readonly IRelayKillSwitch _killSwitch;
    private readonly IRelayWorldExecutor _executor;
    private readonly IRelayAuditSink _audit;
    private readonly IRelayClock _clock;
    private readonly object _gate = new();
    private readonly Queue<WorkItem> _queue = new();
    private readonly Dictionary<string, int> _playerCounts = new(StringComparer.Ordinal);
    private readonly Dictionary<IdempotencyKey, IdempotencyEntry> _idempotency = new();
    private readonly Queue<IdempotencyKey> _completedIdempotency = new();
    private readonly SemaphoreSlim _available = new(0);
    private readonly CancellationTokenSource _stopping = new();
    private readonly Task[] _workers;
    private int _globalCount;
    private bool _disposed;

    public RelayAuthorityService(
        RelayAuthorityOptions options,
        IRelayPolicyResolver policies,
        IRelayRevocationState revocations,
        IRelayKillSwitch killSwitch,
        IRelayWorldExecutor executor,
        IRelayAuditSink audit,
        IRelayClock? clock = null)
    {
        _options = options ?? throw new ArgumentNullException(nameof(options));
        ValidateOptions(options);
        _policies = policies ?? throw new ArgumentNullException(nameof(policies));
        _revocations = revocations ?? throw new ArgumentNullException(nameof(revocations));
        _killSwitch = killSwitch ?? throw new ArgumentNullException(nameof(killSwitch));
        _executor = executor ?? throw new ArgumentNullException(nameof(executor));
        _audit = audit ?? throw new ArgumentNullException(nameof(audit));
        _clock = clock ?? new SystemRelayClock();
        _workers = Enumerable.Range(0, options.WorkerCount).Select(_ => Task.Run(WorkerAsync)).ToArray();
    }

    public Task<RelayResponse> SubmitAsync(
        AuthenticatedPlayer player,
        RelayRpcEnvelope request,
        CancellationToken cancellationToken = default)
    {
        if (player is null) throw new ArgumentNullException(nameof(player));
        if (request is null) throw new ArgumentNullException(nameof(request));

        var validation = Validate(player, request, _clock.UtcNow);
        if (validation is not null)
            return Reject(request, player, validation.Value.Code, validation.Value.Message, validation.Value.Retryable);
        if (cancellationToken.IsCancellationRequested)
            return Reject(request, player, "cancelled", "Request was cancelled.", true);

        var key = new IdempotencyKey(player.PlayerId, player.SessionId, request.IdempotencyKey);
        var fingerprint = Fingerprint(request);
        RelayResponse? rejected = null;
        Task<RelayResponse>? replayOriginal = null;

        lock (_gate)
        {
            if (_disposed)
            {
                rejected = Failure(request, "service_unavailable", "The relay authority service is stopping.", true);
            }
            else if (_idempotency.TryGetValue(key, out var old))
            {
                if (old.Fingerprint == fingerprint)
                    replayOriginal = old.Completion.Task;
                else
                    rejected = Failure(request, "idempotency_conflict", "Idempotency key was reused for a different request.", false);
            }
            else if (_globalCount >= _options.Limits.MaxGlobalQueue)
            {
                rejected = Failure(request, "global_queue_full", "The relay queue is full.", true);
            }
            else
            {
                _playerCounts.TryGetValue(player.PlayerId, out var playerCount);
                if (playerCount >= _options.Limits.MaxPlayerQueue)
                {
                    rejected = Failure(request, "player_queue_full", "This player's relay queue is full.", true);
                }
                else
                {
                    TrimCompletedIdempotencyForSlot();
                    if (_idempotency.Count >= _options.MaxIdempotencyEntries)
                    {
                        rejected = Failure(request, "idempotency_cache_full", "The relay retry cache is full.", true);
                    }
                    else
                    {
                        var completion = new TaskCompletionSource<RelayResponse>(TaskCreationOptions.RunContinuationsAsynchronously);
                        _idempotency.Add(key, new IdempotencyEntry(fingerprint, completion));
                        _queue.Enqueue(new WorkItem(player, request, key, completion, cancellationToken));
                        _globalCount++;
                        _playerCounts[player.PlayerId] = playerCount + 1;
                        _available.Release();
                        return completion.Task;
                    }
                }
            }
        }

        if (replayOriginal is not null)
            return ReplayAsync(replayOriginal, player, request, cancellationToken);
        Audit(request, player, "rejected", rejected!.Code, null);
        return Task.FromResult(rejected);
    }

    private async Task<RelayResponse> ReplayAsync(
        Task<RelayResponse> original,
        AuthenticatedPlayer player,
        RelayRpcEnvelope request,
        CancellationToken cancellationToken)
    {
        RelayResponse response;
        if (!cancellationToken.CanBeCanceled)
        {
            response = await original.ConfigureAwait(false);
        }
        else
        {
            var cancelled = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
            using var registration = cancellationToken.Register(() => cancelled.TrySetResult(true));
            if (await Task.WhenAny(original, cancelled.Task).ConfigureAwait(false) != original)
                response = Failure(request, "cancelled", "Request was cancelled.", true);
            else
                response = await original.ConfigureAwait(false);
        }

        var correlated = response with
        {
            RequestId = request.RequestId,
            CorrelationId = request.CorrelationId,
        };
        Audit(request, player, "idempotent_replay", correlated.Code, null);
        return correlated;
    }

    private async Task WorkerAsync()
    {
        while (true)
        {
            try
            {
                await _available.WaitAsync(_stopping.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                break;
            }

            WorkItem? item = null;
            lock (_gate)
            {
                if (_queue.Count > 0)
                {
                    item = _queue.Dequeue();
                    _globalCount--;
                    var remaining = _playerCounts[item.Player.PlayerId] - 1;
                    if (remaining == 0) _playerCounts.Remove(item.Player.PlayerId);
                    else _playerCounts[item.Player.PlayerId] = remaining;
                }
            }

            if (item is not null) await ProcessAsync(item).ConfigureAwait(false);
        }
    }

    private async Task ProcessAsync(WorkItem item)
    {
        var request = item.Request;
        var now = _clock.UtcNow;
        var decision = item.CancellationToken.IsCancellationRequested
            ? ("cancelled", "Request was cancelled.", true)
            : ValidateExecution(item.Player, request, now);
        RelayOperationResult result;

        if (decision is not null)
        {
            result = new(false, decision.Value.Code, decision.Value.Message, null, null, null, decision.Value.Retryable);
        }
        else
        {
            try
            {
                using var cancellation = CancellationTokenSource.CreateLinkedTokenSource(_stopping.Token, item.CancellationToken);
                result = await _executor.ExecuteAsync(item.Player, request, cancellation.Token).ConfigureAwait(false);
                if (result is null)
                    result = new(false, "execution_failed", "The authoritative operation returned no result.", null, null, null, true);
            }
            catch (OperationCanceledException)
            {
                result = new(false, "cancelled", "Request was cancelled.", null, null, null, true);
            }
            catch
            {
                result = new(false, "execution_failed", "The authoritative operation failed.", null, null, null, true);
            }
        }

        if (result.ResponsePayload is not null &&
            Encoding.UTF8.GetByteCount(result.ResponsePayload) > _options.Limits.MaxResponseBytes)
        {
            result = result with
            {
                Succeeded = false,
                Code = "response_too_large",
                Message = "The authoritative response exceeds the configured limit.",
                ResponsePayload = null,
                Retryable = false,
            };
        }

        Audit(request, item.Player, decision?.Code ?? "allowed", result.Code, result);
        Complete(item, new RelayResponse(
            request.RequestId,
            request.CorrelationId,
            SafeResponseCode(result.Code),
            SafeResponseMessage(result.Message),
            result.ResponsePayload,
            result.Retryable));
    }

    private (string Code, string Message, bool Retryable)? ValidateExecution(
        AuthenticatedPlayer player,
        RelayRpcEnvelope request,
        DateTimeOffset now)
    {
        if (request.ExpiresAt <= now)
            return ("request_expired", "The request expired while waiting for authority.", true);
        if (player.SessionId != request.PlayerSessionId)
            return ("identity_mismatch", "The request session does not match the authenticated player.", false);
        if (player is { IsAuthenticated: false } or { IsAuthoritativeProcess: false })
            return ("unauthorized_transport", "An authenticated authoritative transport is required.", false);
        if (player.SessionId.Length == 0 || player.PlayerId.Length == 0)
            return ("unauthorized_transport", "An authenticated authoritative transport is required.", false);

        RelayPolicy policy;
        try
        {
            if (_killSwitch.IsEnabled)
                return ("kill_switch", "The relay kill switch is enabled.", true);
            if (_revocations.IsRevoked(player))
                return ("revoked", "The player's relay permission was revoked.", false);
            policy = _policies.Resolve(player);
            if (policy is null)
                return ("authorization_unavailable", "Relay authorization is unavailable.", true);
        }
        catch
        {
            return ("authorization_unavailable", "Relay authorization is unavailable.", true);
        }

        if (!policy.Enabled)
            return ("bridge_disabled", "The relay is disabled.", false);
        if (request.Operation != "discovery.read" &&
            request.Operation != "ic10.read" &&
            request.Operation != "ic10.write")
            return ("unsupported_operation", "The relay operation is not supported.", false);

        var administrator = policy.Capabilities.HasFlag(RelayCapability.Administrator);
        if (request.Operation == "discovery.read" &&
            !administrator &&
            !policy.Capabilities.HasFlag(RelayCapability.DiscoveryRead))
            return ("permission_denied", "Discovery permission is required.", false);
        if (request.Operation == "ic10.read" &&
            !administrator &&
            !policy.Capabilities.HasFlag(RelayCapability.Ic10Read))
            return ("permission_denied", "IC10 read permission is required.", false);
        if (request.Operation == "ic10.write" &&
            !administrator &&
            !policy.Capabilities.HasFlag(RelayCapability.Ic10WriteAny))
        {
            var message = policy.Capabilities.HasFlag(RelayCapability.Ic10WriteOwn)
                ? "Verified target ownership is unavailable; an explicit write-any or administrator grant is required."
                : "IC10 write permission is required.";
            return ("permission_denied", message, false);
        }

        return null;
    }

    private (string Code, string Message, bool Retryable)? Validate(
        AuthenticatedPlayer player,
        RelayRpcEnvelope request,
        DateTimeOffset now)
    {
        if (!player.IsAuthenticated ||
            !player.IsAuthoritativeProcess ||
            string.IsNullOrWhiteSpace(player.PlayerId) ||
            string.IsNullOrWhiteSpace(player.SessionId) ||
            string.IsNullOrWhiteSpace(player.TransportName))
            return ("unauthorized_transport", "An authenticated authoritative transport is required.", false);
        if (request.Version != RelayProtocol.Version)
            return ("unsupported_version", "The relay protocol version is unsupported.", false);
        if (!string.Equals(request.PlayerSessionId, player.SessionId, StringComparison.Ordinal))
            return ("identity_mismatch", "The request session does not match the authenticated player.", false);
        if (request.Payload is null)
            return ("invalid_request", "A request payload is required.", false);
        if (request.PayloadBytes > _options.Limits.MaxPayloadBytes)
            return ("payload_too_large", "The request payload exceeds the configured limit.", false);
        if (request.ExpiresAt <= request.IssuedAt ||
            request.ExpiresAt <= now ||
            request.IssuedAt > now.AddSeconds(5) ||
            request.ExpiresAt - request.IssuedAt > _options.Limits.MaxRequestAge)
            return ("request_expired", "The request expiry metadata is invalid or expired.", true);
        if (string.IsNullOrWhiteSpace(request.RequestId) ||
            string.IsNullOrWhiteSpace(request.CorrelationId) ||
            string.IsNullOrWhiteSpace(request.IdempotencyKey) ||
            string.IsNullOrWhiteSpace(request.Operation))
            return ("invalid_request", "Required request identifiers and operation are missing.", false);
        if (MetadataBytes(player.PlayerId, player.SessionId, player.TransportName, request.RequestId,
                request.CorrelationId, request.IdempotencyKey, request.Operation, request.PlayerSessionId) >
            _options.Limits.MaxMetadataBytes)
            return ("metadata_too_large", "Request or identity metadata exceeds the configured limit.", false);
        return null;
    }

    private Task<RelayResponse> Reject(
        RelayRpcEnvelope request,
        AuthenticatedPlayer player,
        string code,
        string message,
        bool retryable)
    {
        var response = Failure(request, code, message, retryable);
        Audit(request, player, "rejected", code, null);
        return Task.FromResult(response);
    }

    private void Complete(WorkItem item, RelayResponse response)
    {
        lock (_gate)
        {
            item.Completion.TrySetResult(response);
            if (_idempotency.TryGetValue(item.Key, out var entry) &&
                ReferenceEquals(entry.Completion, item.Completion))
            {
                _completedIdempotency.Enqueue(item.Key);
                TrimCompletedIdempotency();
            }
        }
    }

    private void TrimCompletedIdempotencyForSlot()
    {
        while (_idempotency.Count >= _options.MaxIdempotencyEntries &&
               _completedIdempotency.Count > 0)
            _idempotency.Remove(_completedIdempotency.Dequeue());
    }

    private void TrimCompletedIdempotency()
    {
        while (_idempotency.Count > _options.MaxIdempotencyEntries &&
               _completedIdempotency.Count > 0)
            _idempotency.Remove(_completedIdempotency.Dequeue());
    }

    private void Audit(
        RelayRpcEnvelope request,
        AuthenticatedPlayer player,
        string permissionDecision,
        string resultCode,
        RelayOperationResult? result)
    {
        if (!IsMutation(request.Operation)) return;
        try
        {
            _audit.Record(new RelayAuditRecord(
                _clock.UtcNow,
                SafeAuditValue(player.PlayerId, "unavailable"),
                SafeAuditValue(player.SessionId, "unavailable"),
                SafeAuditValue(result?.WorldEpoch, "unavailable"),
                SafeAuditValue(result?.TargetReference, "redacted"),
                SafeHash(result?.OldHash),
                SafeHash(result?.NewHash),
                SafeAuditValue(permissionDecision, "unavailable"),
                SafeAuditValue(request.RequestId, "unavailable"),
                SafeAuditValue(resultCode, "unavailable")));
        }
        catch
        {
            // Audit failures must not kill an authority worker or leak request payloads into fallback logs.
        }
    }

    private static string Fingerprint(RelayRpcEnvelope request)
    {
        using var buffer = new System.IO.MemoryStream();
        using (var writer = new System.IO.BinaryWriter(buffer, Encoding.UTF8, true))
        {
            writer.Write(request.Version);
            writer.Write(request.Operation);
            writer.Write(request.PlayerSessionId);
            writer.Write(request.Payload);
        }
        using var hash = SHA256.Create();
        return string.Concat(hash.ComputeHash(buffer.ToArray())
            .Select(value => value.ToString("x2", System.Globalization.CultureInfo.InvariantCulture)));
    }

    private static int MetadataBytes(params string[] values) =>
        values.Sum(value => Encoding.UTF8.GetByteCount(value ?? string.Empty));

    private static string SafeAuditValue(string? value, string fallback)
    {
        if (string.IsNullOrWhiteSpace(value)) return fallback;
        var safe = new string(value.Take(128).Select(character => char.IsControl(character) ? '?' : character).ToArray());
        return string.IsNullOrWhiteSpace(safe) ? fallback : safe;
    }

    private static string? SafeHash(string? value) =>
        value is not null &&
        value.Length == 64 &&
        value.All(character => character is >= '0' and <= '9' or >= 'a' and <= 'f')
            ? value
            : null;

    private static string SafeResponseCode(string? code) =>
        !string.IsNullOrWhiteSpace(code) && code.Length <= 64 &&
        code.All(character => character is >= 'a' and <= 'z' or >= '0' and <= '9' or '_')
            ? code
            : "execution_failed";

    private static string SafeResponseMessage(string? message)
    {
        if (string.IsNullOrWhiteSpace(message)) return "The authoritative operation did not provide a result message.";
        return new string(message.Take(512).Select(character => char.IsControl(character) ? ' ' : character).ToArray());
    }

    private static bool IsMutation(string operation) => operation == "ic10.write";

    private static RelayResponse Failure(
        RelayRpcEnvelope request,
        string code,
        string message,
        bool retryable) =>
        new(request.RequestId, request.CorrelationId, code, message, null, retryable);

    private static void ValidateOptions(RelayAuthorityOptions options)
    {
        if (options.WorkerCount < 1 ||
            options.MaxIdempotencyEntries < 1 ||
            options.Limits.MaxPayloadBytes < 1 ||
            options.Limits.MaxResponseBytes < 1 ||
            options.Limits.MaxMetadataBytes < 1 ||
            options.Limits.MaxPlayerQueue < 1 ||
            options.Limits.MaxGlobalQueue < 1 ||
            options.Limits.MaxRequestAge <= TimeSpan.Zero)
            throw new ArgumentOutOfRangeException(nameof(options));
    }

    public void Dispose()
    {
        List<WorkItem> abandoned;
        lock (_gate)
        {
            if (_disposed) return;
            _disposed = true;
            abandoned = _queue.ToList();
            _queue.Clear();
            _globalCount = 0;
            _playerCounts.Clear();
        }

        _stopping.Cancel();
        foreach (var item in abandoned)
        {
            var response = Failure(item.Request, "service_unavailable", "The relay authority service stopped before execution.", true);
            Audit(item.Request, item.Player, "service_stopping", response.Code, null);
            Complete(item, response);
        }

        try
        {
            Task.WaitAll(_workers, TimeSpan.FromSeconds(1));
        }
        catch
        {
            // Disposal is best effort; in-flight executors receive the stopping token.
        }
        _available.Dispose();
        _stopping.Dispose();
    }
}
