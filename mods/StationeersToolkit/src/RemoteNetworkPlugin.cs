using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Assets.Scripts.Util;
using BepInEx.Configuration;
using Assets.Scripts.Networking;
using LaunchPadBooster;
using UnityEngine;
using System.Security.Cryptography;
using StationeersToolkit.Core;

namespace StationeersToolkit;

public sealed class RemoteNetworkPlugin : MonoBehaviour
{
    private static readonly Mod Mod = new("com.shaneyu.stationeerstoolkit", "0.1.0");
    private readonly RemoteNetworkIndex _index = new();
    private ConfigEntry<bool>? _enabled;
    private ConfigEntry<bool>? _bridgeEnabled;
    private ConfigEntry<int>? _bridgePort;
    private ConfigEntry<bool>? _relayEnabled;
    private ConfigEntry<bool>? _allowRemoteWrites;
    private ConfigEntry<string>? _pairingToken;
    private BridgeHttpService? _bridge;
    private BridgeSnapshotAdapter? _bridgeSnapshot;
    private ConfigFile? _config;
    private int _worldEpoch;
    private int _revision;
    private bool _worldLoaded;
    private int _mainThreadId;
    private bool _bridgeStarted;

    public void OnLoaded(List<GameObject> prefabs, ConfigFile config, List<Assembly> assemblies)
    {
        _mainThreadId = Environment.CurrentManagedThreadId;
        _config = config;
        _enabled = config.Bind("RemoteNetwork", "Enabled", true, "Enable the RemoteNetwork device and local discovery index.");
        if (!_enabled.Value) return;
        _bridgeEnabled = config.Bind("Bridge", "Enabled", true, "Expose RemoteNetwork discovery and conditional IC10 source sync over the authenticated loopback bridge.");
        _bridgePort = config.Bind("Bridge", "Port", 3032, "Loopback bridge port. Do not expose this port publicly.");
        _relayEnabled = config.Bind("Relay", "Enabled", true, "Route client discovery source reads and conditional writes through the authoritative host/server.");
        _allowRemoteWrites = config.Bind("Relay", "AllowRemoteWrites", true, "Allow authenticated multiplayer clients to write IC10 source through the authoritative host/server.");
        if (_relayEnabled.Value)
        {
            RemoteAuthorityRelay.Configure(
                (chipId, worldEpoch) => _index.ReadSource(chipId, worldEpoch),
                (chipId, request) => _index.WriteSource(chipId, request, 65536),
                clientId => _allowRemoteWrites.Value && clientId > 0);
            Mod.Networking.RegisterMessage<RemoteAuthorityRequestMessage>();
            Mod.Networking.RegisterMessage<RemoteAuthorityResponseMessage>();
        }
        RemoteNetworkPrefab.Install(Mod);
        _bridgeSnapshot = new BridgeSnapshotAdapter(_index, () => _revision, () => _worldLoaded, () => _worldEpoch, ReadSource, WriteSource);
        WorldManager.OnWorldStarted += OnWorldStarted;
    }

    private ChipSourceReadResult ReadSource(string chipId, string worldEpoch)
    {
        if (_relayEnabled?.Value == true && NetworkManager.IsClient && !NetworkManager.IsServer)
            return RemoteAuthorityRelay.ReadFromHost(chipId, worldEpoch);
        if (Environment.CurrentManagedThreadId == _mainThreadId)
            return _index.ReadSource(chipId, worldEpoch);
        if (!UnityMainThreadDispatcher.Exists()) return new(ChipSourceReadStatus.Unavailable);
        var completion = new System.Threading.Tasks.TaskCompletionSource<ChipSourceReadResult>();
        UnityMainThreadDispatcher.Instance().Enqueue(() => completion.TrySetResult(_index.ReadSource(chipId, worldEpoch)));
        return completion.Task.Wait(TimeSpan.FromSeconds(2))
            ? completion.Task.Result
            : new ChipSourceReadResult(ChipSourceReadStatus.Unavailable);
    }

    private ChipSourceWriteResult WriteSource(string chipId, ChipSourceWriteRequest request)
    {
        if (_relayEnabled?.Value == true && NetworkManager.IsClient && !NetworkManager.IsServer)
            return RemoteAuthorityRelay.WriteToHost(chipId, request);
        if (Environment.CurrentManagedThreadId == _mainThreadId)
            return _index.WriteSource(chipId, request, 65536);
        if (!UnityMainThreadDispatcher.Exists()) return new(ChipSourceWriteStatus.Unavailable);
        var completion = new System.Threading.Tasks.TaskCompletionSource<ChipSourceWriteResult>();
        UnityMainThreadDispatcher.Instance().Enqueue(() => completion.TrySetResult(_index.WriteSource(chipId, request, 65536)));
        return completion.Task.Wait(TimeSpan.FromSeconds(2))
            ? completion.Task.Result
            : new ChipSourceWriteResult(ChipSourceWriteStatus.Unavailable);
    }

    private void OnWorldStarted()
    {
        StartBridgeForRuntimeRole();
        _worldEpoch++;
        _worldLoaded = false;
        StartCoroutine(ReconcileWhenReady(_worldEpoch));
        StartCoroutine(ReconcileLoop(_worldEpoch));
    }

    private void Update() => RemoteAuthorityRelay.Tick();

    private void StartBridgeForRuntimeRole()
    {
        if (_bridgeStarted || _bridgeEnabled is null || !_bridgeEnabled.Value || _config is null || _bridgePort is null || _bridgeSnapshot is null)
            return;

        var isServer = NetworkManager.IsServer;
        var isClient = NetworkManager.IsClient;
        var isBatchMode = Application.isBatchMode;
        var role = BridgeRuntimePolicy.GetRole(isServer, isClient, isBatchMode);
        if (!BridgeRuntimePolicy.ShouldStartIdeBridge(isServer, isClient, isBatchMode))
        {
            Debug.Log($"[StationeersToolkit] Runtime role={role}; capability={BridgeRuntimePolicy.CapabilityState(isServer, isClient, isBatchMode)}");
            return;
        }

        _pairingToken = _config.Bind("Bridge", "PairingToken", NewToken(), "Copy this token into VS Code SecretStorage when pairing the bridge.");
        _bridge = new BridgeHttpService(_bridgePort.Value, _pairingToken.Value, _bridgeSnapshot.Hello, _bridgeSnapshot.Scopes, _bridgeSnapshot.Source, _bridgeSnapshot.WriteSource);
        _bridge.Start();
        _bridgeStarted = true;
        Debug.Log($"[StationeersToolkit] Runtime role={role}; capability={BridgeRuntimePolicy.CapabilityState(isServer, isClient, isBatchMode)}; loopback bridge started on 127.0.0.1:{_bridgePort.Value}");
    }

    private IEnumerator ReconcileLoop(int epoch)
    {
        while (epoch == _worldEpoch)
        {
            yield return new WaitForSeconds(1f);
            if (epoch != _worldEpoch || !_worldLoaded) continue;
            UnityMainThreadDispatcher.Instance().Enqueue(() =>
            {
                if (epoch == _worldEpoch && _index.Reconcile(epoch.ToString())) _revision++;
            });
        }
    }

    private IEnumerator ReconcileWhenReady(int epoch)
    {
        while (!UnityMainThreadDispatcher.Exists()) yield return null;
        UnityMainThreadDispatcher.Instance().Enqueue(() =>
        {
            if (epoch == _worldEpoch && _index.Reconcile(epoch.ToString())) _revision++;
            if (epoch == _worldEpoch) _worldLoaded = true;
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
