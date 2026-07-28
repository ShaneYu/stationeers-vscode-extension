using System.Collections.Generic;
using System.Linq;
using Assets.Scripts.Networking;
using Assets.Scripts.Objects;
using Assets.Scripts.Objects.Electrical;
using Assets.Scripts.Objects.Pipes;
using Assets.Scripts.Networks;
using StationeersBridge.RemoteNetwork.Core;

namespace StationeersBridge.RemoteNetwork;

internal sealed class RemoteNetworkIndex
{
    private DiscoverySnapshot _snapshot = new("uninitialized", new DiscoveryScope[0], new ScopeWarning[0]);
    internal DiscoverySnapshot Snapshot => _snapshot;

    internal void Reconcile(string worldEpoch)
    {
        var anchors = new List<RemoteNetworkAnchor>();
        foreach (var device in Device.AllDevices.Active())
        {
            if (device.PrefabName != RemoteNetworkPrefab.StructurePrefabName || device is not LogicMemory anchor) continue;
            var attachments = new List<NetworkAttachment>();
            for (var port = 0; port < 2; port++)
            {
                var network = anchor.GetNetwork(port);
                if (network is null) continue;
                var chips = network.DataDeviceList.SelectMany(DescribeChips)
                    .GroupBy(chip => chip.ChipReference).Select(group => group.First()).ToArray();
                attachments.Add(new NetworkAttachment(anchor.ReferenceId.ToString(), port, network.ReferenceId.ToString(), chips));
            }
            anchors.Add(new RemoteNetworkAnchor(anchor.ReferenceId.ToString(), anchor.CustomName, attachments));
        }
        _snapshot = DiscoveryGrouping.Group(worldEpoch, anchors);
    }

    private static IEnumerable<ChipSummary> DescribeChips(Device device)
    {
        if (device is CircuitHousing housing)
        {
            var chip = housing._ProgrammableChipSlot?.Get<ProgrammableChip>();
            if (chip is null) yield break;

            yield return new ChipSummary(
                housing.ReferenceId.ToString(),
                chip.ReferenceId.ToString(),
                Name(housing),
                Language(chip),
                IsPowered(housing, chip),
                housing.PrefabName,
                chip.PrefabName);
            yield break;
        }

        // Large consoles hold a ProgrammableChipMotherboard rather than a
        // CircuitHousing. The motherboard is still the user's Lua/IC host,
        // so expose the console as its stable housing reference.
        if (device is Console console && console.HasMotherboard && console.CurrentMotherboard is not null)
        {
            var motherboard = console.CurrentMotherboard;
            var programmableMotherboard = motherboard as Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard;
            yield return new ChipSummary(
                console.ReferenceId.ToString(),
                console.ReferenceId.ToString(),
                Name(console),
                Language(motherboard, motherboard.MasterMotherboard, motherboard.SourcePrefab, motherboard.ParentSlot?.Get<Thing>(), programmableMotherboard is null ? null : programmableMotherboard.GetSourceCode().ToString()),
                IsPowered(console, motherboard),
                console.PrefabName,
                motherboard.PrefabName);
        }
    }

    private static string Name(Device device) => string.IsNullOrWhiteSpace(device.CustomName) ? device.PrefabName : device.CustomName;

    private static bool IsPowered(Device device, object poweredObject)
    {
        var poweredValue = poweredObject is ProgrammableChip chip ? chip.PoweredValue :
            poweredObject is Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard motherboard ? motherboard.PoweredValue : 0;
        return device.Powered || device.PoweredValue > 0 ||
            poweredObject is ProgrammableChip { Powered: true } || poweredObject is Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard { Powered: true } ||
            poweredValue > 0;
    }

    private static ChipLanguage Language(params object?[] values)
    {
        var identity = string.Join(" ", values.Where(value => value is not null).SelectMany(value => Identity(value!)));
        return identity.IndexOf("lua", System.StringComparison.OrdinalIgnoreCase) >= 0
            ? ChipLanguage.Lua
            : values.OfType<string>().Any(LooksLikeLua)
                ? ChipLanguage.Lua
            : values.Any(value => value is ProgrammableChip) || identity.IndexOf("ic10", System.StringComparison.OrdinalIgnoreCase) >= 0
                ? ChipLanguage.Ic10
                : ChipLanguage.Unknown;
    }

    private static bool LooksLikeLua(string source) =>
        source.IndexOf("local ", System.StringComparison.OrdinalIgnoreCase) >= 0 ||
        source.IndexOf("function ", System.StringComparison.OrdinalIgnoreCase) >= 0 ||
        source.IndexOf("require(", System.StringComparison.OrdinalIgnoreCase) >= 0 ||
        source.IndexOf(" then", System.StringComparison.OrdinalIgnoreCase) >= 0;

    private static IEnumerable<string> Identity(object value)
    {
        yield return value.GetType().FullName ?? string.Empty;
        if (value is Thing thing) yield return thing.PrefabName;

        foreach (var propertyName in new[] { "DisplayName", "SpawnableName", "TrackableName", "PrefabName" })
        {
            var property = value.GetType().GetProperty(propertyName, System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.Public);
            if (property?.GetValue(value) is string text) yield return text;
        }

        var getPrefabName = value.GetType().GetMethod("GetPrefabName", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.Public, null, System.Type.EmptyTypes, null);
        if (getPrefabName?.Invoke(value, null) is string prefabName) yield return prefabName;
    }
}
