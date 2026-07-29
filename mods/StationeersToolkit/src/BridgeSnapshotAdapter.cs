using System;
using System.Collections.Generic;
using System.Linq;
using Assets.Scripts.Networking;
using StationeersToolkit.Core;
using UnityEngine;

namespace StationeersToolkit;

internal sealed class BridgeSnapshotAdapter
{
    private readonly RemoteNetworkIndex _index;
    private readonly Func<int> _revision;
    private readonly Func<bool> _worldLoaded;
    private readonly Func<int> _worldEpoch;
    private readonly Func<string, string, ChipSourceReadResult> _readSource;
    private readonly Func<string, ChipSourceWriteRequest, ChipSourceWriteResult> _writeSource;
    private readonly string _instanceId = Guid.NewGuid().ToString("N");

    internal BridgeSnapshotAdapter(RemoteNetworkIndex index, Func<int> revision, Func<bool> worldLoaded, Func<int> worldEpoch, Func<string, string, ChipSourceReadResult> readSource, Func<string, ChipSourceWriteRequest, ChipSourceWriteResult> writeSource)
    {
        _index = index;
        _revision = revision;
        _worldLoaded = worldLoaded;
        _worldEpoch = worldEpoch;
        _readSource = readSource;
        _writeSource = writeSource;
    }

    internal object Hello() => new
    {
        apiVersion = "1.0",
        bridgeVersion = "0.1.0",
        gameVersion = Application.version,
        instanceId = _instanceId,
        role = Role(),
        world = new { loaded = _worldLoaded(), name = NetworkManager.CurrentGameSession?.Name ?? WorldManager.CurrentWorldName ?? string.Empty, epoch = _worldEpoch().ToString(), revision = _revision().ToString() },
        capabilities = new { scopeDiscovery = true, ic10SourceRead = true, ic10SourceWrite = true, multiplayerRelay = RemoteAuthorityRelay.IsConfigured, eventStream = false },
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
                housingPrefab = chip.HousingPrefab,
                chipPrefab = chip.ChipPrefab,
                language = Language(chip.Language),
                identitySource = chip.ChipReferenceIsHousing ? "housing" : "chip",
                powered = chip.Powered,
                source = chip.Source is null
                    ? new { readable = false, writable = false, length = 0, version = "0", sha256 = new string('0', 64) }
                    : new { readable = true, writable = true, length = chip.Source.Length, version = chip.Source.Version, sha256 = chip.Source.Sha256 },
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

    internal ChipSourceReadResult Source(string chipId, string worldEpoch) => _readSource(chipId, worldEpoch);
    internal ChipSourceWriteResult WriteSource(string chipId, ChipSourceWriteRequest request) => _writeSource(chipId, request);

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
