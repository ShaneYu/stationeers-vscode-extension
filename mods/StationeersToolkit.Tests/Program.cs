using StationeersToolkit.Core;

static NetworkAttachment Attachment(string anchor, int port, string network, params string[] chips) =>
    new(anchor, port, network, chips.Select(id => new ChipSummary("housing-" + id, id, id, ChipLanguage.Ic10)).ToArray());

static void Assert(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException(message);
}

var chip = Attachment("a1", 0, "n1", "c1");
var sameNetwork = DiscoveryGrouping.Group("epoch-1", new[]
{
    new RemoteNetworkAnchor("a1", " Lab ", new[] { chip }),
    new RemoteNetworkAnchor("a2", "Lab", new[] { Attachment("a2", 0, "n1", "c1") }),
});
Assert(sameNetwork.Scopes.Count == 1, "same network and label must group");
Assert(sameNetwork.Scopes[0].AnchorCount == 2, "group must count aliases");
Assert(sameNetwork.Scopes[0].Chips.Count == 1, "chips must be deduplicated");

var aliases = DiscoveryGrouping.Group("epoch-1", new[]
{
    new RemoteNetworkAnchor("a1", "Alpha", new[] { chip }),
    new RemoteNetworkAnchor("a2", "Beta", new[] { Attachment("a2", 0, "n1", "c1") }),
});
Assert(aliases.Scopes.Count == 2 && aliases.Scopes.All(scope => scope.Chips.Count == 1), "different labels must retain aliases");

var splitNetwork = DiscoveryGrouping.Group("epoch-1", new[]
{
    new RemoteNetworkAnchor("a1", "Lab", new[] { chip }),
    new RemoteNetworkAnchor("a2", "Lab", new[] { Attachment("a2", 0, "n2", "c2") }),
});
Assert(splitNetwork.Scopes.Count == 2, "same label on different networks must split");

var unnamed = DiscoveryGrouping.Group("epoch-1", new[]
{
    new RemoteNetworkAnchor("a1", "  ", new[] { chip }),
});
Assert(unnamed.Scopes.Count == 0 && unnamed.Warnings.Count == 1, "empty labels must warn without creating scopes");

var metadata = new ChipSourceMetadata(12, "7", new string('a', 64));
var sourceChip = new ChipSummary("housing-c1", "c1", "IC", ChipLanguage.Ic10, Source: metadata);
Assert(sourceChip.Source?.Length == 12 && sourceChip.Source.Version == "7", "IC10 source metadata must be retained");
var luaChip = new ChipSummary("housing-l1", "l1", "Lua", ChipLanguage.Lua, Source: null);
Assert(luaChip.Source is null, "Lua source metadata must not be exposed");
var consoleLuaChip = new ChipSummary(
    "motherboard-l2",
    "motherboard-l2",
    "Screen 1",
    ChipLanguage.Lua,
    Source: null,
    ChipReferenceIsHousing: true);
Assert(
    consoleLuaChip.ChipReferenceIsHousing &&
    consoleLuaChip.ChipReference == consoleLuaChip.HousingReference,
    "console-hosted Lua boards must retain their housing-only identity marker");

var initialReload = DiscoveryGrouping.Group("epoch-reload-1", new[]
{
    new RemoteNetworkAnchor("a1", "Lab", new[] { Attachment("a1", 0, "n1", "c1") }),
});
var incrementalReload = DiscoveryGrouping.Group("epoch-reload-1", new[]
{
    new RemoteNetworkAnchor("a1", "Lab", new[] { Attachment("a1", 0, "n1", "c1") }),
    new RemoteNetworkAnchor("a2", "Lab", new[] { Attachment("a2", 0, "n2", "c2") }),
});
Assert(initialReload.Scopes.Count == 1 && incrementalReload.Scopes.Count == 2, "incremental discovery must expose the new network");
Assert(incrementalReload.Scopes.SelectMany(scope => scope.Chips).Select(chip => chip.ChipReference).Distinct().Count() == 2, "incremental discovery must not duplicate chip identities");
var reloadedWorld = DiscoveryGrouping.Group("epoch-reload-2", new[]
{
    new RemoteNetworkAnchor("a1", "Lab", new[] { Attachment("a1", 0, "n1", "c1") }),
});
Assert(reloadedWorld.WorldEpoch != initialReload.WorldEpoch, "world reload must establish a new discovery epoch");

var originalSource = "move r0 1";
var originalHash = ChipSourceWriteValidation.Hash(originalSource);
var updatedSource = "move r0 2";
var updatedHash = ChipSourceWriteValidation.Hash(updatedSource);
var validWrite = new ChipSourceWriteRequest(
    "write-1",
    "epoch-1",
    "7",
    originalHash,
    updatedSource,
    updatedHash);
Assert(
    ChipSourceWriteValidation.Validate(validWrite, 1024) is null,
    "well-formed write metadata and source hash must validate");
Assert(
    ChipSourceWriteValidation.Validate(validWrite with { SourceSha256 = originalHash }, 1024) ==
    ChipSourceWriteStatus.InvalidSource,
    "source hash mismatch must fail before world mutation");
Assert(
    ChipSourceWriteValidation.Validate(validWrite with { ExpectedSha256 = "not-a-hash" }, 1024) ==
    ChipSourceWriteStatus.InvalidSource,
    "malformed expected hash must fail before conflict handling");
var oversizedSource = new string('x', 32);
Assert(
    ChipSourceWriteValidation.Validate(
        validWrite with
        {
            Source = oversizedSource,
            SourceSha256 = ChipSourceWriteValidation.Hash(oversizedSource),
        },
        8) == ChipSourceWriteStatus.Oversized,
    "source writes must enforce the configured UTF-8 byte bound");

var current = new ChipSource(
    "epoch-1",
    "chip-1",
    "housing-1",
    "ic10",
    originalSource.Length,
    "7",
    originalHash,
    originalSource);
Assert(
    !ChipSourceWriteValidation.HasConflict(validWrite, current),
    "matching world, version, and hash must allow the atomic write");
Assert(
    ChipSourceWriteValidation.HasConflict(validWrite with { ExpectedVersion = "6" }, current),
    "stale nonzero version must preserve conflict safety");
Assert(
    ChipSourceWriteValidation.HasConflict(validWrite with { WorldEpoch = "epoch-2" }, current),
    "stale world must preserve conflict safety");
Assert(
    !ChipSourceWriteValidation.HasConflict(
        validWrite with { ExpectedVersion = "unknown-client-version" },
        current with { Version = "0" }),
    "hash remains the concurrency token when the game exposes no usable version");

Assert(BridgeRuntimePolicy.GetRole(false, false) == BridgeRuntimeRole.SinglePlayer, "offline runtime must be classified as single-player");
Assert(BridgeRuntimePolicy.GetRole(false, true) == BridgeRuntimeRole.Client, "client runtime must be classified as client");
Assert(BridgeRuntimePolicy.GetRole(true, true) == BridgeRuntimeRole.Host, "listen-server runtime must be classified as host");
Assert(BridgeRuntimePolicy.GetRole(true, false) == BridgeRuntimeRole.DedicatedServer, "server-only runtime must be classified as dedicated server");
Assert(BridgeRuntimePolicy.GetRole(false, false, true) == BridgeRuntimeRole.DedicatedServer, "batch-mode runtime must be classified as dedicated server");
Assert(BridgeRuntimePolicy.ShouldStartIdeBridge(false, true), "client bridge must remain available");
Assert(BridgeRuntimePolicy.ShouldStartIdeBridge(true, true), "host bridge must remain available");
Assert(!BridgeRuntimePolicy.ShouldStartIdeBridge(true, false), "dedicated server must suppress the IDE bridge");
Assert(!BridgeRuntimePolicy.ShouldStartIdeBridge(false, false, true), "batch-mode runtime must suppress the IDE bridge");
Assert(BridgeRuntimePolicy.CapabilityState(true, false) == "dedicated_server_listener_suppressed", "dedicated server capability must be explicit");

Console.WriteLine("RemoteNetwork grouping/source authority contract tests passed (26 cases).");
