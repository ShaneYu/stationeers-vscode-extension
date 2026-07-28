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
                var chips = network.DataDeviceList.OfType<CircuitHousing>()
                    .Select(housing => housing._ProgrammableChipSlot?.Get<ProgrammableChip>())
                    .Where(chip => chip is not null)
                    .Select(chip => new ChipSummary(
                        chip!.ReferenceId.ToString(), chip.ReferenceId.ToString(), chip.PrefabName,
                        chip.GetType().FullName == "StationeersLua.IntegratedCircuitLua" ? ChipLanguage.Lua : ChipLanguage.Ic10))
                    .GroupBy(chip => chip.ChipReference).Select(group => group.First()).ToArray();
                attachments.Add(new NetworkAttachment(anchor.ReferenceId.ToString(), port, network.ReferenceId.ToString(), chips));
            }
            anchors.Add(new RemoteNetworkAnchor(anchor.ReferenceId.ToString(), anchor.CustomName, attachments));
        }
        _snapshot = DiscoveryGrouping.Group(worldEpoch, anchors);
    }
}
