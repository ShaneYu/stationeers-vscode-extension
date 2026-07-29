using System.Linq;

namespace StationeersBridge.Relay.Core;

public sealed class SinglePlayerRelay : IRelayTransport
{
    private readonly RelayAuthorityService _authority;
    private readonly AuthenticatedPlayer _player;
    public SinglePlayerRelay(RelayAuthorityService authority, string playerId = "single-player")
    {
        _authority = authority ?? throw new ArgumentNullException(nameof(authority));
        _player = new AuthenticatedPlayer(playerId, "single-player-session", true, true, "internal-single-player");
    }
    public RelayTransportState State => new(true, true, "single_player", "Using the internal single-player authority short circuit.");
    public Task<RelayResponse> DispatchAsync(RelayRpcEnvelope request, CancellationToken cancellationToken) => _authority.SubmitAsync(_player, request with { PlayerSessionId = _player.SessionId }, cancellationToken);
}

public sealed class RelayResponseCorrelator
{
    private sealed record PendingResponse(string CorrelationId, string WorldEpoch, string PlayerSessionId);
    private readonly object _gate = new();
    private readonly Dictionary<string, PendingResponse> _pending = new(StringComparer.Ordinal);
    private readonly int _maxPending;

    public RelayResponseCorrelator(int maxPending = 256)
    {
        if (maxPending < 1) throw new ArgumentOutOfRangeException(nameof(maxPending));
        _maxPending = maxPending;
    }

    public bool Track(string requestId, string correlationId, string worldEpoch, string playerSessionId)
    {
        if (string.IsNullOrWhiteSpace(requestId) ||
            string.IsNullOrWhiteSpace(correlationId) ||
            string.IsNullOrWhiteSpace(worldEpoch) ||
            string.IsNullOrWhiteSpace(playerSessionId))
            return false;
        lock (_gate)
        {
            return _pending.Count < _maxPending &&
                _pending.TryAdd(requestId, new PendingResponse(correlationId, worldEpoch, playerSessionId));
        }
    }

    public bool Accept(RelayResponse response, string worldEpoch, string playerSessionId)
    {
        lock (_gate)
        {
            return _pending.TryGetValue(response.RequestId, out var expected) &&
                string.Equals(expected.CorrelationId, response.CorrelationId, StringComparison.Ordinal) &&
                string.Equals(expected.WorldEpoch, worldEpoch, StringComparison.Ordinal) &&
                string.Equals(expected.PlayerSessionId, playerSessionId, StringComparison.Ordinal) &&
                _pending.Remove(response.RequestId);
        }
    }

    public void Cancel(string requestId) { lock (_gate) _pending.Remove(requestId); }
    public void CancelSession(string playerSessionId)
    {
        lock (_gate)
        {
            foreach (var requestId in _pending
                .Where(pair => string.Equals(pair.Value.PlayerSessionId, playerSessionId, StringComparison.Ordinal))
                .Select(pair => pair.Key)
                .ToArray())
                _pending.Remove(requestId);
        }
    }
}
