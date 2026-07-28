using System;
using System.Collections.Generic;
using System.Linq;
using Assets.Scripts;
using Assets.Scripts.Objects;
using Assets.Scripts.Objects.Electrical;
using LaunchPadBooster;
using UnityEngine;
using Object = UnityEngine.Object;

namespace StationeersBridge.RemoteNetwork;

internal static class RemoteNetworkPrefab
{
    internal const string StructurePrefabName = "StructureRemoteNetwork";
    internal const string KitPrefabName = "ItemKitRemoteNetwork";
    private static bool _registered;

    internal static void Install(Mod mod)
    {
        Prefab.OnPrefabsLoaded += Register;
        Register();
    }

    internal static void Uninstall() => Prefab.OnPrefabsLoaded -= Register;

    private static void Register()
    {
        if (_registered) return;
        var vanillaStructure = Prefab.Find<LogicMemory>("StructureLogicMemory");
        var vanillaKit = Prefab.Find<MultiConstructor>("ItemKitLogicMemory");
        if (vanillaStructure is null || vanillaKit is null) return;

        var structure = Object.Instantiate(vanillaStructure, Prefab.PrefabsGameObject.transform);
        var kit = Object.Instantiate(vanillaKit, Prefab.PrefabsGameObject.transform);
        structure.gameObject.name = StructurePrefabName; structure.PrefabName = StructurePrefabName;
        structure.PrefabHash = Animator.StringToHash(StructurePrefabName); structure.IsCustomThing = false; structure.HideInStationpedia = false;
        kit.gameObject.name = KitPrefabName; kit.PrefabName = KitPrefabName;
        kit.PrefabHash = Animator.StringToHash(KitPrefabName); kit.IsCustomThing = false; kit.HideInStationpedia = false;
        kit.Constructables = new List<Structure> { structure };
        if (structure.BuildStates.Count == 0 || vanillaStructure.BuildStates.Count == 0
            || ReferenceEquals(structure.BuildStates[0], vanillaStructure.BuildStates[0])
            || ReferenceEquals(structure.BuildStates[0].Tool, vanillaStructure.BuildStates[0].Tool))
        {
            Object.Destroy(structure.gameObject); Object.Destroy(kit.gameObject);
            throw new InvalidOperationException("RemoteNetwork build metadata is not isolated from Logic Memory.");
        }
        structure.BuildStates[0].Tool.ToolEntry = kit;
        WorldManager.Instance.SourcePrefabs.Add(structure); WorldManager.Instance.SourcePrefabs.Add(kit);
        Prefab.RegisterExisting(structure); Prefab.RegisterExisting(kit);
        _registered = true;
    }
}
