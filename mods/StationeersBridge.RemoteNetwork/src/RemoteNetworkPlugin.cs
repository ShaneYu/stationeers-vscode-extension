using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Assets.Scripts.Util;
using BepInEx.Configuration;
using LaunchPadBooster;
using UnityEngine;

namespace StationeersBridge.RemoteNetwork;

public sealed class RemoteNetworkPlugin : MonoBehaviour
{
    private static readonly Mod Mod = new("dev.stationeers.bridge.remotenetwork", "0.1.0");
    private readonly RemoteNetworkIndex _index = new();
    private ConfigEntry<bool>? _enabled;
    private int _worldEpoch;

    public void OnLoaded(List<GameObject> prefabs, ConfigFile config, List<Assembly> assemblies)
    {
        _enabled = config.Bind("RemoteNetwork", "Enabled", true, "Enable the RemoteNetwork device and local discovery index.");
        if (!_enabled.Value) return;
        RemoteNetworkPrefab.Install(Mod);
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
        });
    }

    private void OnDestroy()
    {
        WorldManager.OnWorldStarted -= OnWorldStarted;
        RemoteNetworkPrefab.Uninstall();
    }
}
