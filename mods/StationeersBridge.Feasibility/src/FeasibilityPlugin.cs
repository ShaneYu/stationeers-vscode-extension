using System;
using System.Collections;
using System.Collections.Generic;
using System.Reflection;
using Assets.Scripts.Networking;
using Assets.Scripts.Util;
using BepInEx.Configuration;
using LaunchPadBooster;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace StationeersBridge.Feasibility;

public sealed class FeasibilityPlugin : MonoBehaviour
{
    internal const string ModId = "dev.stationeers.bridge.feasibility";
    internal const string Version = "0.1.0";

    private static readonly Mod ProbeMod = new(ModId, Version);

    private ConfigEntry<bool>? _enabled;
    private ConfigEntry<bool>? _registerPrefab;
    private ConfigEntry<bool>? _runRpc;
    private ConfigEntry<bool>? _allowSourceMutation;
    private ConfigEntry<string>? _fixtureNamePrefix;
    private ConfigEntry<string>? _sourceMutationHousingName;
    private ConfigEntry<string>? _sourceMutationConfirmation;
    private List<Assembly> _modAssemblies = new();
    private int _sceneGeneration;
    private Coroutine? _dispatcherWait;

    public void OnLoaded(
        List<GameObject> prefabs,
        ConfigFile config,
        List<Assembly> assemblies)
    {
        _enabled = config.Bind(
            "Development",
            "Enabled",
            false,
            "Enable P3.02 diagnostic probes. Do not enable for ordinary play.");
        _registerPrefab = config.Bind(
            "Development",
            "RegisterProbePrefab",
            true,
            "Register cloned RemoteNetwork probe kit and structure prefabs.");
        _runRpc = config.Bind(
            "Development",
            "RunRpcOnWorldLoad",
            false,
            "Send one bounded RPC to the authoritative host after a world loads.");
        _allowSourceMutation = config.Bind(
            "Development",
            "AllowSourceMutation",
            false,
            "Temporarily mutate and restore IC10 source in one exact named fixture.");
        _fixtureNamePrefix = config.Bind(
            "Development",
            "FixtureNamePrefix",
            "P302-",
            "Only named anchors and housings starting with this prefix are probed.");
        _sourceMutationHousingName = config.Bind(
            "Development",
            "SourceMutationHousingName",
            "P302-IC10",
            "Exact disposable IC10 housing name eligible for the source mutation probe.");
        _sourceMutationConfirmation = config.Bind(
            "Development",
            "SourceMutationConfirmation",
            string.Empty,
            "Must equal MUTATE_AND_RESTORE_P302_SOURCE before source mutation runs.");

        _modAssemblies = assemblies;
        ProbeLog.Write(
            "loader-lifecycle",
            _enabled.Value ? "enabled" : "disabled",
            new
            {
                entrypoint = GetType().FullName,
                callback = nameof(OnLoaded),
                threadId = Environment.CurrentManagedThreadId,
                prefabCount = prefabs.Count,
                assemblyCount = assemblies.Count,
                version = Version,
            });

        if (!_enabled.Value)
        {
            return;
        }

        ProbeMod.Networking.RegisterRPC<AuthorityProbeRpc>();
        ContractProbe.Run(_modAssemblies);
        MainThreadProbe.Run();

        if (_registerPrefab.Value)
        {
            PrefabProbe.Install(ProbeMod);
        }

        SceneManager.sceneLoaded += OnSceneLoaded;
        WorldManager.OnWorldStarted += OnWorldStarted;
    }

    private void OnSceneLoaded(Scene scene, LoadSceneMode mode)
    {
        ProbeLog.Write(
            "world-scene",
            "loaded",
            new { scene = scene.name, mode = mode.ToString() });
    }

    private void OnWorldStarted()
    {
        var sceneName = SceneManager.GetActiveScene().name;
        ProbeLog.Write(
            "world-lifecycle",
            "started",
            new { scene = sceneName, threadId = Environment.CurrentManagedThreadId });
        _sceneGeneration += 1;
        if (_dispatcherWait is not null)
        {
            StopCoroutine(_dispatcherWait);
        }

        _dispatcherWait = StartCoroutine(
            WaitForDispatcherThenQueue(sceneName, _sceneGeneration));
    }

    private IEnumerator WaitForDispatcherThenQueue(
        string sceneName,
        int sceneGeneration)
    {
        const float maxWaitSeconds = 30f;
        var waitedFrames = 0;
        var startedAt = Time.realtimeSinceStartup;

        while (!UnityMainThreadDispatcher.Exists()
               && Time.realtimeSinceStartup - startedAt < maxWaitSeconds)
        {
            waitedFrames += 1;
            yield return null;
        }

        _dispatcherWait = null;
        if (sceneGeneration != _sceneGeneration)
        {
            yield break;
        }

        if (!UnityMainThreadDispatcher.Exists())
        {
            ProbeLog.Write(
                "main-thread-dispatch",
                "blocked",
                new
                {
                    reason = "UnityMainThreadDispatcher remained unavailable",
                    waitedFrames,
                    elapsedMs =
                        (Time.realtimeSinceStartup - startedAt) * 1000f,
                    scene = sceneName,
                });
            yield break;
        }

        ProbeLog.Write(
            "main-thread-dispatch",
            "ready",
            new
            {
                waitedFrames,
                elapsedMs = (Time.realtimeSinceStartup - startedAt) * 1000f,
                scene = sceneName,
            });
        MainThreadProbe.Run();
        QueueWorldProbe(sceneName, sceneGeneration);
    }

    private void QueueWorldProbe(string sceneName, int sceneGeneration)
    {
        if (!UnityMainThreadDispatcher.Exists())
        {
            ProbeLog.Write(
                "main-thread-dispatch",
                "blocked",
                new { reason = "UnityMainThreadDispatcher.Instance is unavailable" });
            return;
        }

        UnityMainThreadDispatcher.Instance().Enqueue(
            () =>
            {
                if (sceneGeneration != _sceneGeneration
                    || !string.Equals(
                        SceneManager.GetActiveScene().name,
                        sceneName,
                        StringComparison.Ordinal))
                {
                    ProbeLog.Write(
                        "world-enumeration",
                        "cancelled",
                        new
                        {
                            reason = "scene changed before queued probe executed",
                            queuedScene = sceneName,
                            activeScene = SceneManager.GetActiveScene().name,
                        });
                    return;
                }

                WorldProbe.Run(
                    sceneName,
                    _fixtureNamePrefix?.Value ?? "P302-",
                    _allowSourceMutation?.Value ?? false,
                    _sourceMutationHousingName?.Value ?? "P302-IC10",
                    _sourceMutationConfirmation?.Value ?? string.Empty);

                if (_runRpc?.Value ?? false)
                {
                    AuthorityProbeRpc.RunClientProbe();
                }
            });
    }

    private void OnDestroy()
    {
        _sceneGeneration += 1;
        if (_dispatcherWait is not null)
        {
            StopCoroutine(_dispatcherWait);
            _dispatcherWait = null;
        }

        SceneManager.sceneLoaded -= OnSceneLoaded;
        WorldManager.OnWorldStarted -= OnWorldStarted;
        PrefabProbe.Uninstall();
        ProbeLog.Write(
            "loader-lifecycle",
            "unloaded",
            new { threadId = Environment.CurrentManagedThreadId });
    }
}
