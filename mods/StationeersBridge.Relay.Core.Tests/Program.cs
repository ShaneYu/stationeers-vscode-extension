using StationeersBridge.Relay.Core;

static void Assert(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException(message);
}

static RelayRpcEnvelope Request(
    string requestId,
    string idempotencyKey,
    string sessionId = "s1",
    string operation = "ic10.write",
    string payload = "source",
    DateTimeOffset? now = null,
    TimeSpan? lifetime = null) =>
    new(
        RelayProtocol.Version,
        requestId,
        "correlation-" + requestId,
        operation,
        sessionId,
        idempotencyKey,
        now ?? DateTimeOffset.UtcNow,
        (now ?? DateTimeOffset.UtcNow) + (lifetime ?? TimeSpan.FromSeconds(10)),
        payload);

static AuthenticatedPlayer Player(string playerId = "p1", string sessionId = "s1") =>
    new(playerId, sessionId, true, true, "verified-test-transport");

static RelayAuthorityService Service(
    MutablePolicy policy,
    IRelayWorldExecutor executor,
    CollectingAudit? audit = null,
    MutableRevocation? revocation = null,
    MutableKillSwitch? killSwitch = null,
    RelayAuthorityOptions? options = null,
    IRelayClock? clock = null) =>
    new(
        options ?? new RelayAuthorityOptions(),
        policy,
        revocation ?? new MutableRevocation(),
        killSwitch ?? new MutableKillSwitch(),
        executor,
        audit ?? new CollectingAudit(),
        clock);

async Task IdentityBindingFailsClosed()
{
    var executor = new FakeExecutor();
    using var service = Service(new MutablePolicy(RelayCapability.Administrator), executor);
    var unauthenticated = await service.SubmitAsync(
        new AuthenticatedPlayer("p1", "s1", true, false, "client"),
        Request("identity-1", "identity-key-1"));
    var mismatch = await service.SubmitAsync(
        Player(),
        Request("identity-2", "identity-key-2", sessionId: "forged-session"));

    Assert(unauthenticated.Code == "unauthorized_transport", "non-authoritative transport must fail closed");
    Assert(mismatch.Code == "identity_mismatch", "envelope session must match authenticated transport identity");
    Assert(executor.Calls == 0, "identity failures must never reach the world executor");
}

async Task PermissionsFailClosedWithoutVerifiedOwnership()
{
    var executor = new FakeExecutor();
    var policy = new MutablePolicy(RelayCapability.Ic10WriteOwn);
    using var service = Service(policy, executor);
    var own = await service.SubmitAsync(Player(), Request("permission-1", "permission-key-1"));
    Assert(own.Code == "permission_denied", $"write-own must fail closed until the game supplies verified ownership (got {own.Code})");

    policy.Capabilities = RelayCapability.Administrator;
    var adminRead = await service.SubmitAsync(
        Player(),
        Request("permission-2", "permission-key-2", operation: "discovery.read", payload: "{}"));
    var adminWrite = await service.SubmitAsync(Player(), Request("permission-3", "permission-key-3"));
    Assert(adminRead.Code == "ok" && adminWrite.Code == "ok", "administrator must project all relay capabilities");
    Assert(executor.Calls == 2, "only permitted operations should execute");
}

async Task AuthorizationIsRecheckedAfterQueueDelay()
{
    var executor = new BlockingExecutor();
    var policy = new MutablePolicy(RelayCapability.Ic10WriteAny);
    var audit = new CollectingAudit();
    using var service = Service(policy, executor, audit);

    var first = service.SubmitAsync(Player(), Request("recheck-1", "recheck-key-1"));
    await executor.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
    var queued = service.SubmitAsync(Player(), Request("recheck-2", "recheck-key-2"));
    policy.Enabled = false;
    executor.Release.TrySetResult();

    Assert((await first).Code == "ok", "in-flight operation should finish with its authoritative result");
    Assert((await queued).Code == "bridge_disabled", "global policy disable must apply after queue delay");
    Assert(executor.Calls == 1, "disabled queued operation must not execute");
    Assert(audit.Records.Any(record => record.RequestId == "recheck-2" && record.PermissionDecision == "bridge_disabled"),
        "denied queued write must be audited");
}

async Task RevocationAndKillSwitchApplyWithoutRestart()
{
    var executor = new FakeExecutor();
    var policy = new MutablePolicy(RelayCapability.Ic10WriteAny);
    var revocation = new MutableRevocation();
    var killSwitch = new MutableKillSwitch();
    using var service = Service(policy, executor, revocation: revocation, killSwitch: killSwitch);

    revocation.Revoked = true;
    var revoked = await service.SubmitAsync(Player(), Request("guard-1", "guard-key-1"));
    revocation.Revoked = false;
    killSwitch.Enabled = true;
    var killed = await service.SubmitAsync(Player(), Request("guard-2", "guard-key-2"));

    Assert(revoked.Code == "revoked", "runtime player revocation must fail closed");
    Assert(killed.Code == "kill_switch", "runtime global kill switch must fail closed");
    Assert(executor.Calls == 0, "revoked or globally disabled writes must not execute");
}

async Task QueueBoundsArePerPlayerAndGlobal()
{
    var executor = new BlockingExecutor();
    var options = new RelayAuthorityOptions
    {
        Limits = new RelayLimits(1024, 1, 2, TimeSpan.FromMinutes(1)),
        MaxIdempotencyEntries = 8,
    };
    using var service = Service(new MutablePolicy(RelayCapability.Ic10WriteAny), executor, options: options);

    var inFlight = service.SubmitAsync(Player("p1"), Request("queue-1", "queue-key-1"));
    await executor.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
    var p1Queued = service.SubmitAsync(Player("p1"), Request("queue-2", "queue-key-2"));
    var p1Rejected = await service.SubmitAsync(Player("p1"), Request("queue-3", "queue-key-3"));
    var p2Queued = service.SubmitAsync(
        Player("p2", "s2"),
        Request("queue-4", "queue-key-4", sessionId: "s2"));
    var globalRejected = await service.SubmitAsync(
        Player("p3", "s3"),
        Request("queue-5", "queue-key-5", sessionId: "s3"));

    Assert(p1Rejected.Code == "player_queue_full", "one player must not exceed the per-player queue bound");
    Assert(globalRejected.Code == "global_queue_full", "all players together must not exceed the global queue bound");
    executor.Release.TrySetResult();
    Assert((await inFlight).Code == "ok" && (await p1Queued).Code == "ok" && (await p2Queued).Code == "ok",
        "accepted bounded work should complete");
}

async Task IdempotencyIsSessionBoundAndConflictSafe()
{
    var executor = new FakeExecutor();
    using var service = Service(new MutablePolicy(RelayCapability.Ic10WriteAny), executor);
    var firstRequest = Request("idem-1", "shared-key");
    var first = await service.SubmitAsync(Player(), firstRequest);
    var replayRequest = firstRequest with { RequestId = "idem-2", CorrelationId = "correlation-idem-2" };
    var replay = await service.SubmitAsync(Player(), replayRequest);
    var conflict = await service.SubmitAsync(
        Player(),
        replayRequest with { RequestId = "idem-3", CorrelationId = "correlation-idem-3", Payload = "different" });
    var newSession = await service.SubmitAsync(
        Player(sessionId: "s2"),
        Request("idem-4", "shared-key", sessionId: "s2"));

    Assert(first.Code == "ok" && replay.Code == "ok", "semantic retry must replay the completed result");
    Assert(replay.RequestId == "idem-2" && replay.CorrelationId == "correlation-idem-2",
        "replayed result must retain the retry's correlation identifiers");
    Assert(conflict.Code == "idempotency_conflict", "same-session key reuse with different payload must fail");
    Assert(newSession.Code == "ok", "new authenticated session must not receive stale retry state");
    Assert(executor.Calls == 2, "retry must execute once while a new authenticated session executes independently");
}

async Task InFlightIdempotencyEntriesAreNeverEvicted()
{
    var executor = new BlockingExecutor();
    var options = new RelayAuthorityOptions
    {
        Limits = new RelayLimits(1024, 2, 2, TimeSpan.FromMinutes(1)),
        MaxIdempotencyEntries = 1,
    };
    using var service = Service(new MutablePolicy(RelayCapability.Ic10WriteAny), executor, options: options);
    var request = Request("active-idem-1", "active-key");
    var first = service.SubmitAsync(Player(), request);
    await executor.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
    var duplicate = service.SubmitAsync(
        Player(),
        request with { RequestId = "active-idem-2", CorrelationId = "correlation-active-idem-2" });
    var cacheFull = await service.SubmitAsync(Player(), Request("active-idem-3", "other-key"));

    Assert(cacheFull.Code == "idempotency_cache_full", "active retry state must be bounded without eviction");
    executor.Release.TrySetResult();
    Assert((await first).Code == "ok" && (await duplicate).Code == "ok", "in-flight retry must share the original result");
    Assert(executor.Calls == 1, "in-flight retry must never duplicate a mutation");
}

async Task CancelledAndExpiredQueuedWritesDoNotExecute()
{
    var now = DateTimeOffset.UtcNow;
    var clock = new MutableClock(now);
    var executor = new BlockingExecutor();
    using var service = Service(
        new MutablePolicy(RelayCapability.Ic10WriteAny),
        executor,
        clock: clock,
        options: new RelayAuthorityOptions
        {
            Limits = new RelayLimits(1024, 3, 3, TimeSpan.FromMinutes(2)),
            MaxIdempotencyEntries = 8,
        });

    var inFlight = service.SubmitAsync(
        Player(),
        Request("lifetime-1", "lifetime-key-1", now: now, lifetime: TimeSpan.FromMinutes(1)));
    await executor.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
    using var cancellation = new CancellationTokenSource();
    var cancelled = service.SubmitAsync(
        Player(),
        Request("lifetime-2", "lifetime-key-2", now: now, lifetime: TimeSpan.FromMinutes(1)),
        cancellation.Token);
    var expired = service.SubmitAsync(
        Player(),
        Request("lifetime-3", "lifetime-key-3", now: now, lifetime: TimeSpan.FromSeconds(1)));
    cancellation.Cancel();
    clock.UtcNow = now.AddSeconds(2);
    executor.Release.TrySetResult();

    Assert((await inFlight).Code == "ok", "first authoritative operation should complete");
    Assert((await cancelled).Code == "cancelled", "queued cancellation must prevent execution");
    Assert((await expired).Code == "request_expired", "queue-delayed expiry must prevent execution");
    Assert(executor.Calls == 1, "cancelled and expired queued writes must not mutate the world");
}

async Task ResponsesAndAuditRecordsAreBoundedAndSanitized()
{
    var payload = "secret source text";
    var executor = new FakeExecutor
    {
        ResponsePayload = new string('x', 32),
        TargetReference = "chip-1\r\nforged-log-line",
    };
    var audit = new CollectingAudit();
    using var service = Service(
        new MutablePolicy(RelayCapability.Ic10WriteAny),
        executor,
        audit,
        options: new RelayAuthorityOptions
        {
            Limits = new RelayLimits(1024, 2, 2, TimeSpan.FromMinutes(1), MaxResponseBytes: 8),
        });

    var response = await service.SubmitAsync(
        Player("player\nforged"),
        Request("audit-\r\nforged", "audit-key", payload: payload));
    Assert(response.Code == "response_too_large" && response.Payload is null,
        "oversized authoritative response payload must not cross the relay");
    var record = audit.Records.Single();
    Assert(!record.PlayerId.Any(char.IsControl) &&
           !record.RequestId.Any(char.IsControl) &&
           !record.TargetReference.Any(char.IsControl),
        "audit identifiers must be safe against control-character log injection");
    Assert(!record.ToString().Contains(payload, StringComparison.Ordinal),
        "audit record must never contain source payload");

    var denied = await service.SubmitAsync(
        new AuthenticatedPlayer("p2", "s2", false, true, "test"),
        Request("audit-denied", "audit-denied-key", sessionId: "s2", payload: payload));
    Assert(denied.Code == "unauthorized_transport", "invalid write attempt should be rejected");
    Assert(audit.Records.Count == 2, "every attempted mutation, including transport rejection, must be audited");
}

async Task UnsupportedTransportAndWorldCorrelationFailClosed()
{
    var request = Request("transport-1", "transport-key-1");
    var unsupported = await new UnsupportedRelayTransport().DispatchAsync(request, default);
    Assert(unsupported.Code == RelayProtocol.UnsupportedTransportCode, "unmodded multiplayer must require a server companion");

    var correlator = new RelayResponseCorrelator(maxPending: 1);
    Assert(correlator.Track("transport-1", "correlation-transport-1", "epoch-1", "s1"),
        "request should be tracked once");
    Assert(!correlator.Track("transport-2", "correlation-transport-2", "epoch-1", "s1"),
        "pending response correlation must be bounded");
    Assert(!correlator.Accept(
            new RelayResponse("transport-1", "forged-correlation", "ok", "", null, false),
            "epoch-1",
            "s1"),
        "response with a forged correlation identifier must be ignored");
    Assert(!correlator.Accept(
            new RelayResponse("transport-1", "correlation-transport-1", "ok", "", null, false),
            "epoch-2",
            "s1"),
        "late response from an old world must be ignored");
    Assert(!correlator.Accept(
            new RelayResponse("transport-1", "correlation-transport-1", "ok", "", null, false),
            "epoch-1",
            "old-session"),
        "late response from an old player session must be ignored");
    Assert(correlator.Accept(
            new RelayResponse("transport-1", "correlation-transport-1", "ok", "", null, false),
            "epoch-1",
            "s1"),
        "matching request, correlation, world, and session should be accepted once");
}

await IdentityBindingFailsClosed();
await PermissionsFailClosedWithoutVerifiedOwnership();
await AuthorizationIsRecheckedAfterQueueDelay();
await RevocationAndKillSwitchApplyWithoutRestart();
await QueueBoundsArePerPlayerAndGlobal();
await IdempotencyIsSessionBoundAndConflictSafe();
await InFlightIdempotencyEntriesAreNeverEvicted();
await CancelledAndExpiredQueuedWritesDoNotExecute();
await ResponsesAndAuditRecordsAreBoundedAndSanitized();
await UnsupportedTransportAndWorldCorrelationFailClosed();
Console.WriteLine("Relay core authority contract tests passed (10 cases).");

sealed class MutablePolicy : IRelayPolicyResolver
{
    public MutablePolicy(RelayCapability capabilities) => Capabilities = capabilities;
    public bool Enabled { get; set; } = true;
    public RelayCapability Capabilities { get; set; }
    public RelayPolicy Resolve(AuthenticatedPlayer player) => new(Enabled, Capabilities);
}

sealed class MutableRevocation : IRelayRevocationState
{
    public bool Revoked { get; set; }
    public bool IsRevoked(AuthenticatedPlayer player) => Revoked;
}

sealed class MutableKillSwitch : IRelayKillSwitch
{
    public bool Enabled { get; set; }
    public bool IsEnabled => Enabled;
}

sealed class MutableClock : IRelayClock
{
    public MutableClock(DateTimeOffset utcNow) => UtcNow = utcNow;
    public DateTimeOffset UtcNow { get; set; }
}

sealed class CollectingAudit : IRelayAuditSink
{
    private readonly object _gate = new();
    private readonly List<RelayAuditRecord> _records = new();
    public IReadOnlyList<RelayAuditRecord> Records
    {
        get { lock (_gate) return _records.ToArray(); }
    }
    public void Record(RelayAuditRecord record)
    {
        lock (_gate) _records.Add(record);
    }
}

sealed class FakeExecutor : IRelayWorldExecutor
{
    public int Calls { get; private set; }
    public string? ResponsePayload { get; set; } = "response";
    public string TargetReference { get; set; } = "chip-1";
    public Task<RelayOperationResult> ExecuteAsync(
        AuthenticatedPlayer player,
        RelayRpcEnvelope request,
        CancellationToken cancellationToken)
    {
        Calls++;
        return Task.FromResult(new RelayOperationResult(
            true,
            "ok",
            "done",
            ResponsePayload,
            new string('a', 64),
            new string('b', 64),
            WorldEpoch: "epoch-1",
            TargetReference: TargetReference));
    }
}

sealed class BlockingExecutor : IRelayWorldExecutor
{
    public int Calls { get; private set; }
    public TaskCompletionSource Entered { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

    public async Task<RelayOperationResult> ExecuteAsync(
        AuthenticatedPlayer player,
        RelayRpcEnvelope request,
        CancellationToken cancellationToken)
    {
        Calls++;
        Entered.TrySetResult();
        await Release.Task.WaitAsync(cancellationToken);
        return new RelayOperationResult(
            true,
            "ok",
            "done",
            "response",
            new string('a', 64),
            new string('b', 64),
            WorldEpoch: "epoch-1",
            TargetReference: "chip-1");
    }
}
