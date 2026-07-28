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
    private readonly object _gate = new();
    private readonly Dictionary<string, string> _pending = new(StringComparer.Ordinal);
    public bool Track(string requestId, string worldEpoch) { lock (_gate) { return _pending.TryAdd(requestId, worldEpoch); } }
    public bool Accept(RelayResponse response, string worldEpoch) { lock (_gate) { return _pending.TryGetValue(response.RequestId, out var expected) && expected == worldEpoch && _pending.Remove(response.RequestId); } }
    public void Cancel(string requestId) { lock (_gate) _pending.Remove(requestId); }
}
