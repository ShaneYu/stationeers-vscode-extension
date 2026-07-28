using StationeersBridge.Relay.Core;

static void Assert(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
var now = DateTimeOffset.UtcNow;
var request = new RelayRpcEnvelope(1, "r1", "c1", "ic10.write", "s1", "i1", now, now.AddMinutes(1), "source");
var executor = new FakeExecutor();
var policies = new MutablePolicy();
using var service = new RelayAuthorityService(new RelayAuthorityOptions { Limits = new(64, 1, 1, TimeSpan.FromMinutes(1)) }, policies, new NoRevocation(), new NoKillSwitch(), executor, new CollectingAudit());

var denied = await service.SubmitAsync(new AuthenticatedPlayer("p1", "s1", true, true, "test"), request);
Assert(denied.Code == "permission_denied", "write must be denied without a write capability");
policies.Policy = new(true, RelayCapability.Ic10WriteAny);
var permittedRequest = request with { RequestId = "r2", IdempotencyKey = "i2" };
var accepted = await service.SubmitAsync(new AuthenticatedPlayer("p1", "s1", true, true, "test"), permittedRequest);
var replay = await service.SubmitAsync(new AuthenticatedPlayer("p1", "s1", true, true, "test"), permittedRequest);
Assert(accepted.Code == "ok" && replay.Code == "ok" && executor.Calls == 1, "idempotent retry must not execute twice");
var mismatch = await service.SubmitAsync(new AuthenticatedPlayer("p1", "s1", true, true, "test"), permittedRequest with { Payload = "different" });
Assert(mismatch.Code == "idempotency_conflict", "reusing an idempotency key with different data must fail");
var unauthenticated = await service.SubmitAsync(new AuthenticatedPlayer("client", "s1", true, false, "unverified"), request with { RequestId = "r3", IdempotencyKey = "i3" });
Assert(unauthenticated.Code == "unauthorized_transport", "non-authoritative transport must fail closed");
var unsupported = await new UnsupportedRelayTransport().DispatchAsync(request, default);
Assert(unsupported.Code == "server_companion_required", "unsupported multiplayer transport must be explicit");
var killSwitch = new MutableKillSwitch();
using var guardedService = new RelayAuthorityService(new RelayAuthorityOptions(), policies, new MutableRevocation(), killSwitch, executor, new CollectingAudit());
killSwitch.Enabled = true;
var killSwitchNow = DateTimeOffset.UtcNow;
var killed = await guardedService.SubmitAsync(new AuthenticatedPlayer("p2", "s2", true, true, "test"), request with { RequestId = "r4", CorrelationId = "c4", PlayerSessionId = "s2", IdempotencyKey = "i4", IssuedAt = killSwitchNow, ExpiresAt = killSwitchNow.AddSeconds(10) });
Assert(killed.Code == "kill_switch", $"kill switch must be checked (got {killed.Code})");
killSwitch.Enabled = false;
var revoked = (MutableRevocation)guardedService.GetType().GetField("_revocations", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)!.GetValue(guardedService)!;
revoked.Revoked = true;
var revokedNow = DateTimeOffset.UtcNow;
var deniedRevoked = await guardedService.SubmitAsync(new AuthenticatedPlayer("p3", "s3", true, true, "test"), request with { RequestId = "r5", CorrelationId = "c5", PlayerSessionId = "s3", IdempotencyKey = "i5", IssuedAt = revokedNow, ExpiresAt = revokedNow.AddSeconds(10) });
Assert(deniedRevoked.Code == "revoked", "revocation must be checked");
var correlator = new RelayResponseCorrelator();
Assert(correlator.Track("r3", "epoch-1"), "correlation should track");
Assert(!correlator.Accept(new RelayResponse("r3", "c1", "ok", "", null, false), "epoch-2"), "late response from old world must be ignored");
Assert(correlator.Accept(new RelayResponse("r3", "c1", "ok", "", null, false), "epoch-1"), "current response should correlate");
Console.WriteLine("Relay core tests passed (6 cases).");

sealed class MutablePolicy : IRelayPolicyResolver { public RelayPolicy Policy { get; set; } = new(true, RelayCapability.None); public RelayPolicy Resolve(AuthenticatedPlayer player) => Policy; }
sealed class NoRevocation : IRelayRevocationState { public bool IsRevoked(AuthenticatedPlayer player) => false; }
sealed class NoKillSwitch : IRelayKillSwitch { public bool IsEnabled => false; }
sealed class MutableRevocation : IRelayRevocationState { public bool Revoked; public bool IsRevoked(AuthenticatedPlayer player) => Revoked; }
sealed class MutableKillSwitch : IRelayKillSwitch { public bool Enabled; public bool IsEnabled => Enabled; }
sealed class CollectingAudit : IRelayAuditSink { public void Record(RelayAuditRecord record) { if (record.TargetReference != "redacted") throw new InvalidOperationException("audit must not contain request payload"); } }
sealed class FakeExecutor : IRelayWorldExecutor { public int Calls; public Task<RelayOperationResult> ExecuteAsync(AuthenticatedPlayer player, RelayRpcEnvelope request, CancellationToken cancellationToken) { Calls++; return Task.FromResult(new RelayOperationResult(true, "ok", "done", "response", "old", "new")); } }
