using System;
using System.Collections.Generic;
using System.Linq;
using Assets.Scripts.Networking;
using StationeersBridge.RemoteNetwork.Core;
using UnityEngine;

namespace StationeersBridge.RemoteNetwork;

internal sealed class BridgeSnapshotAdapter
{
    private readonly RemoteNetworkIndex _index;
    private readonly Func<int> _revision;
    private readonly Func<bool> _worldLoaded;
    private readonly Func<int> _worldEpoch;
    private readonly string _instanceId = Guid.NewGuid().ToString("N");

    internal BridgeSnapshotAdapter(RemoteNetworkIndex index, Func<int> revision, Func<bool> worldLoaded, Func<int> worldEpoch)
    {
        _index = index;
        _revision = revision;
        _worldLoaded = worldLoaded;
        _worldEpoch = worldEpoch;
    }

    internal object Hello() => new
    {
        apiVersion = "1.0",
        bridgeVersion = "0.1.0",
        gameVersion = Application.version,
        instanceId = _instanceId,
        role = Role(),
        world = new { loaded = _worldLoaded(), epoch = _worldEpoch().ToString(), revision = _revision().ToString() },
        capabilities = new { scopeDiscovery = true, ic10SourceRead = false, ic10SourceWrite = false, multiplayerRelay = false, eventStream = false },
        limits = new { maxSourceBytes = 65536, maxRequestsPerSecond = 10, maxConnections = 8 },
    };

    internal object Scopes()
    {
        var snapshot = _index.Snapshot;
        var chips = snapshot.Scopes.SelectMany(scope => scope.Chips).GroupBy(chip => chip.ChipReference, StringComparer.Ordinal).Select(group =>
        {
            var chip = group.First();
            return new
            {
                chipId = chip.ChipReference,
                housingReferenceId = chip.HousingReference,
                housingName = chip.HousingName,
                housingPrefab = "unknown",
                chipPrefab = "unknown",
                language = Language(chip.Language),
                powered = false,
                source = new { readable = false, writable = false, version = "0", sha256 = new string('0', 64) },
            };
        }).ToArray();

        return new
        {
            worldEpoch = snapshot.WorldEpoch,
            revision = _revision().ToString(),
            scopes = snapshot.Scopes.Select(scope => new
            {
                scopeId = $"scope:{scope.NetworkReference}:{scope.Label}",
                name = scope.Label,
                disambiguator = (string?)null,
                anchorCount = scope.AnchorCount,
                chipIds = scope.Chips.Select(chip => chip.ChipReference).ToArray(),
            }).ToArray(),
            chips,
            warnings = snapshot.Warnings.Select(warning => new { code = "unlabeled_remote_network", message = warning.Message, anchorReferenceId = warning.AnchorReference }).ToArray(),
        };
    }

    private static string Language(ChipLanguage language) => language switch
    {
        ChipLanguage.Lua => "lua",
        ChipLanguage.Ic10 => "ic10",
        _ => "unknown",
    };

    private static string Role() => NetworkManager.IsServer && NetworkManager.IsClient
        ? "host"
        : NetworkManager.IsServer
            ? "dedicatedServer"
            : NetworkManager.IsClient
                ? "client"
                : "singlePlayer";
}
