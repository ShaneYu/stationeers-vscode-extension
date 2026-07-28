using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Assets.Scripts.Util;
using BepInEx.Configuration;
using LaunchPadBooster;
using UnityEngine;
using System.Security.Cryptography;

namespace StationeersBridge.RemoteNetwork;

public sealed class RemoteNetworkPlugin : MonoBehaviour
{
    private static readonly Mod Mod = new("dev.stationeers.bridge.remotenetwork", "0.1.0");
    private readonly RemoteNetworkIndex _index = new();
    private ConfigEntry<bool>? _enabled;
    private ConfigEntry<bool>? _bridgeEnabled;
    private ConfigEntry<int>? _bridgePort;
    private ConfigEntry<string>? _pairingToken;
    private BridgeHttpService? _bridge;
    private BridgeSnapshotAdapter? _bridgeSnapshot;
    private int _worldEpoch;
    private int _revision;
    private bool _worldLoaded;

    public void OnLoaded(List<GameObject> prefabs, ConfigFile config, List<Assembly> assemblies)
    {
        _enabled = config.Bind("RemoteNetwork", "Enabled", true, "Enable the RemoteNetwork device and local discovery index.");
        if (!_enabled.Value) return;
        _bridgeEnabled = config.Bind("Bridge", "Enabled", true, "Expose read-only RemoteNetwork discovery over the authenticated loopback bridge.");
        _bridgePort = config.Bind("Bridge", "Port", 3032, "Loopback bridge port. Do not expose this port publicly.");
        _pairingToken = config.Bind("Bridge", "PairingToken", NewToken(), "Copy this token into VS Code SecretStorage when pairing the bridge.");
        RemoteNetworkPrefab.Install(Mod);
        _bridgeSnapshot = new BridgeSnapshotAdapter(_index, () => _revision, () => _worldLoaded, () => _worldEpoch);
        if (_bridgeEnabled.Value)
        {
            _bridge = new BridgeHttpService(_bridgePort.Value, _pairingToken.Value, _bridgeSnapshot.Hello, _bridgeSnapshot.Scopes);
            _bridge.Start();
        }
        WorldManager.OnWorldStarted += OnWorldStarted;
    }

    private void OnWorldStarted()
    {
        _worldEpoch++;
        StartCoroutine(ReconcileWhenReady(_worldEpoch));
    }

    private IEnumerator ReconcileWhenReady(int epoch)
    {
        while (!UnityMainThreadDispatcher.Exists()) yield return null;
        UnityMainThreadDispatcher.Instance().Enqueue(() =>
        {
            if (epoch == _worldEpoch) _index.Reconcile(epoch.ToString());
            if (epoch == _worldEpoch) { _worldLoaded = true; _revision++; }
        });
    }

    private void OnDestroy()
    {
        WorldManager.OnWorldStarted -= OnWorldStarted;
        _bridge?.Dispose();
        RemoteNetworkPrefab.Uninstall();
    }

    private static string NewToken()
    {
        var bytes = new byte[32];
        using var random = RandomNumberGenerator.Create();
        random.GetBytes(bytes);
        return Convert.ToBase64String(bytes);
    }
}
