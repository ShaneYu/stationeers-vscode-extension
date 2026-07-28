using StationeersBridge.RemoteNetwork.Core;

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

Console.WriteLine("RemoteNetwork grouping/source metadata tests passed (6 cases).");
