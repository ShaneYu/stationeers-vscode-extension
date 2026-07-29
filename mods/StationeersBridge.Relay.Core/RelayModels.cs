using System.Text;

namespace StationeersBridge.Relay.Core;

public static class RelayProtocol
{
    public const int Version = 1;
    public const string UnsupportedTransportCode = "server_companion_required";
}

[Flags]
public enum RelayCapability { None = 0, DiscoveryRead = 1, Ic10Read = 2, Ic10WriteOwn = 4, Ic10WriteAny = 8, Administrator = 16 }

public sealed record RelayLimits(
    int MaxPayloadBytes,
    int MaxPlayerQueue,
    int MaxGlobalQueue,
    TimeSpan MaxRequestAge,
    int MaxResponseBytes = 8192,
    int MaxMetadataBytes = 512);
public sealed record RelayRpcEnvelope(int Version, string RequestId, string CorrelationId, string Operation, string PlayerSessionId, string IdempotencyKey, DateTimeOffset IssuedAt, DateTimeOffset ExpiresAt, string Payload)
{
    public int PayloadBytes => Encoding.UTF8.GetByteCount(Payload ?? string.Empty);
}

// PlayerId and SessionId are supplied by the verified game transport. They are not read from the RPC payload.
public sealed record AuthenticatedPlayer(string PlayerId, string SessionId, bool IsAuthenticated, bool IsAuthoritativeProcess, string TransportName);
public sealed record RelayPolicy(bool Enabled, RelayCapability Capabilities);
public sealed record RelayOperationResult(bool Succeeded, string Code, string Message, string? ResponsePayload, string? OldHash, string? NewHash, bool Retryable = false, string? WorldEpoch = null, string? TargetReference = null);
public sealed record RelayResponse(string RequestId, string CorrelationId, string Code, string Message, string? Payload, bool Retryable);
public sealed record RelayAuditRecord(DateTimeOffset Timestamp, string PlayerId, string SessionId, string WorldEpoch, string TargetReference, string? OldHash, string? NewHash, string PermissionDecision, string RequestId, string Result);

public interface IRelayPolicyResolver { RelayPolicy Resolve(AuthenticatedPlayer player); }
public interface IRelayRevocationState { bool IsRevoked(AuthenticatedPlayer player); }
public interface IRelayKillSwitch { bool IsEnabled { get; } }
public interface IRelayWorldExecutor { Task<RelayOperationResult> ExecuteAsync(AuthenticatedPlayer player, RelayRpcEnvelope request, CancellationToken cancellationToken); }
public interface IRelayAuditSink { void Record(RelayAuditRecord record); }
public interface IRelayClock { DateTimeOffset UtcNow { get; } }

public sealed class SystemRelayClock : IRelayClock { public DateTimeOffset UtcNow => DateTimeOffset.UtcNow; }
public sealed class AllowNothingPolicy : IRelayPolicyResolver { public RelayPolicy Resolve(AuthenticatedPlayer player) => new(false, RelayCapability.None); }
public sealed class DisabledRevocationState : IRelayRevocationState { public bool IsRevoked(AuthenticatedPlayer player) => false; }
public sealed class DisabledKillSwitch : IRelayKillSwitch { public bool IsEnabled => false; }
public sealed class NullAuditSink : IRelayAuditSink { public void Record(RelayAuditRecord record) { } }

public sealed record RelayTransportState(bool Supported, bool Authoritative, string Code, string Message);
public interface IRelayTransport
{
    RelayTransportState State { get; }
    Task<RelayResponse> DispatchAsync(RelayRpcEnvelope request, CancellationToken cancellationToken);
}

// Until a verified game RPC implementation exists, multiplayer dispatch is deliberately unavailable.
public sealed class UnsupportedRelayTransport : IRelayTransport
{
    public RelayTransportState State => new(false, false, RelayProtocol.UnsupportedTransportCode, "An authoritative server companion transport is required.");
    public Task<RelayResponse> DispatchAsync(RelayRpcEnvelope request, CancellationToken cancellationToken) =>
        Task.FromResult(new RelayResponse(request.RequestId, request.CorrelationId, State.Code, State.Message, null, false));
}
