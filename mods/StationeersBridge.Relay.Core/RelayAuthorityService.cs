using System.Collections.Generic;
using System.Linq;

namespace StationeersBridge.Relay.Core;

public sealed class RelayAuthorityOptions
{
    public RelayLimits Limits { get; init; } = new(8192, 8, 64, TimeSpan.FromSeconds(30));
    public int WorkerCount { get; init; } = 1;
    public int MaxIdempotencyEntries { get; init; } = 256;
}

public sealed class RelayAuthorityService : IDisposable
{
    private sealed record WorkItem(AuthenticatedPlayer Player, RelayRpcEnvelope Request, string Fingerprint, TaskCompletionSource<RelayResponse> Completion);
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
    private readonly Dictionary<string, IdempotencyEntry> _idempotency = new(StringComparer.Ordinal);
    private readonly Queue<string> _idempotencyOrder = new();
    private readonly SemaphoreSlim _available = new(0);
    private readonly CancellationTokenSource _stopping = new();
    private readonly Task[] _workers;
    private int _globalCount;

    public RelayAuthorityService(RelayAuthorityOptions options, IRelayPolicyResolver policies, IRelayRevocationState revocations, IRelayKillSwitch killSwitch, IRelayWorldExecutor executor, IRelayAuditSink audit, IRelayClock? clock = null)
    {
        _options = options ?? throw new ArgumentNullException(nameof(options));
        ValidateOptions(options);
        _policies = policies ?? throw new ArgumentNullException(nameof(policies)); _revocations = revocations ?? throw new ArgumentNullException(nameof(revocations));
        _killSwitch = killSwitch ?? throw new ArgumentNullException(nameof(killSwitch)); _executor = executor ?? throw new ArgumentNullException(nameof(executor)); _audit = audit ?? throw new ArgumentNullException(nameof(audit)); _clock = clock ?? new SystemRelayClock();
        _workers = Enumerable.Range(0, options.WorkerCount).Select(_ => Task.Run(WorkerAsync)).ToArray();
    }

    public Task<RelayResponse> SubmitAsync(AuthenticatedPlayer player, RelayRpcEnvelope request, CancellationToken cancellationToken = default)
    {
        var validation = Validate(player, request);
        if (validation is not null) return Task.FromResult(Failure(request, validation.Value.Code, validation.Value.Message, validation.Value.Retryable));
        var key = player.PlayerId + "\u001f" + request.IdempotencyKey;
        var fingerprint = request.Operation + "\u001f" + request.Payload + "\u001f" + request.PlayerSessionId;
        lock (_gate)
        {
            if (_idempotency.TryGetValue(key, out var old))
                return old.Fingerprint == fingerprint ? old.Completion.Task : Task.FromResult(Failure(request, "idempotency_conflict", "Idempotency key was reused for a different request.", false));
            if (_globalCount >= _options.Limits.MaxGlobalQueue) return Task.FromResult(Failure(request, "global_queue_full", "The relay queue is full.", true));
            _playerCounts.TryGetValue(player.PlayerId, out var count);
            if (count >= _options.Limits.MaxPlayerQueue) return Task.FromResult(Failure(request, "player_queue_full", "This player's relay queue is full.", true));
            var completion = new TaskCompletionSource<RelayResponse>(TaskCreationOptions.RunContinuationsAsynchronously);
            _idempotency[key] = new IdempotencyEntry(fingerprint, completion); _idempotencyOrder.Enqueue(key);
            TrimIdempotency();
            _queue.Enqueue(new WorkItem(player, request, fingerprint, completion)); _globalCount++; _playerCounts[player.PlayerId] = count + 1;
            _available.Release();
            if (cancellationToken.CanBeCanceled) _ = CancelIfRequestedAsync(completion, cancellationToken);
            return completion.Task;
        }
    }

    private async Task CancelIfRequestedAsync(TaskCompletionSource<RelayResponse> completion, CancellationToken token)
    { try { await Task.Delay(Timeout.InfiniteTimeSpan, token).ConfigureAwait(false); } catch (OperationCanceledException) { completion.TrySetResult(new RelayResponse("", "", "cancelled", "Request was cancelled.", null, true)); } }

    private async Task WorkerAsync()
    {
        while (!_stopping.IsCancellationRequested)
        {
            try { await _available.WaitAsync(_stopping.Token).ConfigureAwait(false); } catch (OperationCanceledException) { break; }
            WorkItem? item = null;
            lock (_gate) { if (_queue.Count > 0) { item = _queue.Dequeue(); _globalCount--; _playerCounts[item.Player.PlayerId]--; } }
            if (item is not null) await ProcessAsync(item).ConfigureAwait(false);
        }
    }

    private async Task ProcessAsync(WorkItem item)
    {
        var request = item.Request; var now = _clock.UtcNow; var policy = _policies.Resolve(item.Player);
        var decision = Permission(item.Player, request.Operation, policy, now);
        RelayOperationResult result;
        if (decision is not null) result = new(false, decision.Value.Code, decision.Value.Message, null, null, null, decision.Value.Retryable);
        else
        {
            try { result = await _executor.ExecuteAsync(item.Player, request, _stopping.Token).ConfigureAwait(false); }
            catch (OperationCanceledException) { result = new(false, "cancelled", "Request was cancelled.", null, null, null, true); }
            catch { result = new(false, "execution_failed", "The authoritative operation failed.", null, null, null, true); }
        }
        if (IsMutation(request.Operation)) _audit.Record(new RelayAuditRecord(now, item.Player.PlayerId, item.Player.SessionId, result.WorldEpoch ?? "unavailable", result.TargetReference ?? "redacted", result.OldHash, result.NewHash, decision?.Code ?? "allowed", request.RequestId, result.Code));
        item.Completion.TrySetResult(new RelayResponse(request.RequestId, request.CorrelationId, result.Code, result.Message, result.ResponsePayload, result.Retryable));
    }

    private (string Code, string Message, bool Retryable)? Permission(AuthenticatedPlayer player, string operation, RelayPolicy policy, DateTimeOffset now)
    {
        if (!policy.Enabled) return ("bridge_disabled", "The relay is disabled.", false);
        if (_killSwitch.IsEnabled) return ("kill_switch", "The relay kill switch is enabled.", true);
        if (_revocations.IsRevoked(player)) return ("revoked", "The player's relay permission was revoked.", false);
        if (operation != "discovery.read" && operation != "ic10.read" && operation != "ic10.write") return ("unsupported_operation", "The relay operation is not supported.", false);
        if (operation == "discovery.read" && !policy.Capabilities.HasFlag(RelayCapability.DiscoveryRead)) return ("permission_denied", "Discovery permission is required.", false);
        if (operation == "ic10.read" && !policy.Capabilities.HasFlag(RelayCapability.Ic10Read)) return ("permission_denied", "IC10 read permission is required.", false);
        if (operation == "ic10.write" && !(policy.Capabilities.HasFlag(RelayCapability.Ic10WriteOwn) || policy.Capabilities.HasFlag(RelayCapability.Ic10WriteAny) || policy.Capabilities.HasFlag(RelayCapability.Administrator))) return ("permission_denied", "IC10 write permission is required.", false);
        return null;
    }

    private (string Code, string Message, bool Retryable)? Validate(AuthenticatedPlayer player, RelayRpcEnvelope request)
    {
        if (!player.IsAuthenticated || !player.IsAuthoritativeProcess || string.IsNullOrWhiteSpace(player.PlayerId) || string.IsNullOrWhiteSpace(player.SessionId)) return ("unauthorized_transport", "An authenticated authoritative transport is required.", false);
        if (request.Version != RelayProtocol.Version) return ("unsupported_version", "The relay protocol version is unsupported.", false);
        if (request.PlayerSessionId != player.SessionId) return ("identity_mismatch", "The request session does not match the authenticated player.", false);
        if (request.PayloadBytes > _options.Limits.MaxPayloadBytes) return ("payload_too_large", "The request payload exceeds the configured limit.", false);
        var now = _clock.UtcNow;
        if (request.ExpiresAt <= now || request.IssuedAt > now.AddSeconds(5) || request.ExpiresAt - request.IssuedAt > _options.Limits.MaxRequestAge) return ("request_expired", "The request expiry metadata is invalid or expired.", true);
        if (string.IsNullOrWhiteSpace(request.RequestId) || string.IsNullOrWhiteSpace(request.CorrelationId) || string.IsNullOrWhiteSpace(request.IdempotencyKey) || string.IsNullOrWhiteSpace(request.Operation)) return ("invalid_request", "Required request identifiers and operation are missing.", false);
        return null;
    }

    private void TrimIdempotency() { while (_idempotency.Count > _options.MaxIdempotencyEntries && _idempotencyOrder.Count > 0) _idempotency.Remove(_idempotencyOrder.Dequeue()); }
    private static bool IsMutation(string operation) => operation == "ic10.write";
    private static RelayResponse Failure(RelayRpcEnvelope r, string code, string message, bool retryable) => new(r.RequestId, r.CorrelationId, code, message, null, retryable);
    private static void ValidateOptions(RelayAuthorityOptions o) { if (o.WorkerCount < 1 || o.MaxIdempotencyEntries < 1 || o.Limits.MaxPayloadBytes < 1 || o.Limits.MaxPlayerQueue < 1 || o.Limits.MaxGlobalQueue < 1 || o.Limits.MaxRequestAge <= TimeSpan.Zero) throw new ArgumentOutOfRangeException(nameof(o)); }
    public void Dispose() { _stopping.Cancel(); try { Task.WaitAll(_workers, TimeSpan.FromSeconds(1)); } catch { } _available.Dispose(); _stopping.Dispose(); }
}
