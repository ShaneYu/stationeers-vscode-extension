using System;
using System.Collections.Generic;
using System.Linq;
using Assets.Scripts;
using Assets.Scripts.Objects;
using Assets.Scripts.Objects.Electrical;
using LaunchPadBooster;
using UnityEngine;
using Object = UnityEngine.Object;

namespace StationeersBridge.Feasibility;

internal static class PrefabProbe
{
    public const string StructurePrefabName = "StructureRemoteNetworkProbe";
    public const string KitPrefabName = "ItemKitRemoteNetworkProbe";

    private static bool _installed;
    private static bool _registered;
    private static int _vanillaStructureHash;
    private static int _vanillaKitHash;

    public static void Install(Mod mod)
    {
        if (_installed)
        {
            return;
        }

        _installed = true;
        Prefab.OnPrefabsLoaded += ObserveRegistration;
        StagePrefabs(mod);
    }

    public static void Uninstall()
    {
        if (!_installed)
        {
            return;
        }

        Prefab.OnPrefabsLoaded -= ObserveRegistration;
        _installed = false;
    }

    private static void StagePrefabs(Mod mod)
    {
        if (_registered)
        {
            return;
        }

        try
        {
            var sourcePrefabs = WorldManager.Instance?.SourcePrefabs;
            var vanillaStructure = sourcePrefabs?
                .OfType<LogicMemory>()
                .FirstOrDefault(prefab =>
                    prefab.PrefabName == "StructureLogicMemory");
            var vanillaKit = sourcePrefabs?
                .OfType<MultiConstructor>()
                .FirstOrDefault(prefab =>
                    prefab.PrefabName == "ItemKitLogicMemory");
            if (vanillaStructure is null || vanillaKit is null)
            {
                ProbeLog.Write(
                    "prefab-registration",
                    "waiting",
                    new
                    {
                        phase = "source-prefab-staging",
                        vanillaStructure = vanillaStructure is not null,
                        vanillaKit = vanillaKit is not null,
                        fallback = "Prefab.OnPrefabsLoaded",
                    });
                return;
            }

            _vanillaStructureHash = vanillaStructure.PrefabHash;
            _vanillaKitHash = vanillaKit.PrefabHash;

            var structure = Object.Instantiate(vanillaStructure);
            structure.gameObject.name = StructurePrefabName;
            structure.PrefabName = StructurePrefabName;
            structure.PrefabHash = Animator.StringToHash(StructurePrefabName);
            structure.IsCustomThing = false;
            structure.HideInStationpedia = false;

            var kit = Object.Instantiate(vanillaKit);
            kit.gameObject.name = KitPrefabName;
            kit.PrefabName = KitPrefabName;
            kit.PrefabHash = Animator.StringToHash(KitPrefabName);
            kit.IsCustomThing = false;
            kit.HideInStationpedia = false;
            kit.Constructables = new List<Structure> { structure };

            var isolatedBuildMetadata =
                structure.BuildStates.Count > 0
                && vanillaStructure.BuildStates.Count > 0
                && !ReferenceEquals(
                    structure.BuildStates[0],
                    vanillaStructure.BuildStates[0])
                && !ReferenceEquals(
                    structure.BuildStates[0].Tool,
                    vanillaStructure.BuildStates[0].Tool);
            if (!isolatedBuildMetadata)
            {
                ProbeLog.Write(
                    "prefab-registration",
                    "blocked",
                    new
                    {
                        phase = "source-prefab-staging",
                        reason = "cloned build metadata is shared with vanilla",
                    });
                Object.Destroy(structure.gameObject);
                Object.Destroy(kit.gameObject);
                return;
            }

            structure.BuildStates[0].Tool.ToolEntry = kit;
            mod.AddPrefabs(new[] { structure.gameObject, kit.gameObject });
            mod.SetupPrefabs<Thing>().SetBlueprintMaterials();

            ProbeLog.Write(
                "prefab-registration",
                "staged",
                new
                {
                    structurePrefab = structure.PrefabName,
                    structureHash = structure.PrefabHash,
                    kitPrefab = kit.PrefabName,
                    kitHash = kit.PrefabHash,
                    sourcePrefabPhase = true,
                    isCustomThing = structure.IsCustomThing,
                    isolatedBuildMetadata,
                    entryToolHash = structure.BuildStates[0].Tool.ToolEntry.PrefabHash,
                });
        }
        catch (Exception exception)
        {
            ProbeLog.Failure("prefab-registration", exception);
        }
    }

    private static void ObserveRegistration()
    {
        if (_registered)
        {
            return;
        }

        try
        {
            var structure = Prefab.Find<LogicMemory>(StructurePrefabName);
            var kit = Prefab.Find<MultiConstructor>(KitPrefabName);
            if (structure is null || kit is null)
            {
                RegisterAtPrefabReady();
                structure = Prefab.Find<LogicMemory>(StructurePrefabName);
                kit = Prefab.Find<MultiConstructor>(KitPrefabName);
            }

            if (structure is null || kit is null)
            {
                ProbeLog.Write(
                    "prefab-registration",
                    "failed",
                    new
                    {
                        phase = "registered-prefab-observation",
                        structure = structure is not null,
                        kit = kit is not null,
                    });
                return;
            }

            _registered = true;
            var dataPorts = structure.OpenEnds.Count(
                connection => connection.ConnectionType == NetworkType.Data);
            ProbeLog.Write(
                "prefab-registration",
                "observed",
                new
                {
                    structurePrefab = structure.PrefabName,
                    structureHash = structure.PrefabHash,
                    kitPrefab = kit.PrefabName,
                    kitHash = kit.PrefabHash,
                    dataPorts,
                    totalPorts = structure.OpenEnds.Count,
                    reusesVanillaType = structure.GetType().FullName,
                    distinctFromVanilla =
                        structure.PrefabHash != _vanillaStructureHash
                        && kit.PrefabHash != _vanillaKitHash,
                    vanillaStructureHashAfter = _vanillaStructureHash,
                    vanillaKitHashAfter = _vanillaKitHash,
                    entryToolHash = structure.BuildStates[0].Tool.ToolEntry.PrefabHash,
                    kitConstructableHash = kit.Constructables[0].PrefabHash,
                    creativeCursorExpected = !structure.IsCustomThing,
                    customNameProperty = true,
                    usedPower = structure.UsedPower,
                    recipeExpectation = "Gold=1,Copper=1",
                });
        }
        catch (Exception exception)
        {
            ProbeLog.Failure("prefab-registration", exception);
        }
    }

    private static void RegisterAtPrefabReady()
    {
        var vanillaStructure = Prefab.Find<LogicMemory>("StructureLogicMemory");
        var vanillaKit = Prefab.Find<MultiConstructor>("ItemKitLogicMemory");
        if (vanillaStructure is null || vanillaKit is null)
        {
            return;
        }

        _vanillaStructureHash = vanillaStructure.PrefabHash;
        _vanillaKitHash = vanillaKit.PrefabHash;

        var structure = Object.Instantiate(
            vanillaStructure,
            Prefab.PrefabsGameObject.transform);
        structure.gameObject.name = StructurePrefabName;
        structure.PrefabName = StructurePrefabName;
        structure.PrefabHash = Animator.StringToHash(StructurePrefabName);
        structure.IsCustomThing = false;
        structure.HideInStationpedia = false;

        var kit = Object.Instantiate(
            vanillaKit,
            Prefab.PrefabsGameObject.transform);
        kit.gameObject.name = KitPrefabName;
        kit.PrefabName = KitPrefabName;
        kit.PrefabHash = Animator.StringToHash(KitPrefabName);
        kit.IsCustomThing = false;
        kit.HideInStationpedia = false;
        kit.Constructables = new List<Structure> { structure };

        var isolatedBuildMetadata =
            structure.BuildStates.Count > 0
            && vanillaStructure.BuildStates.Count > 0
            && !ReferenceEquals(
                structure.BuildStates[0],
                vanillaStructure.BuildStates[0])
            && !ReferenceEquals(
                structure.BuildStates[0].Tool,
                vanillaStructure.BuildStates[0].Tool);
        if (!isolatedBuildMetadata)
        {
            Object.Destroy(structure.gameObject);
            Object.Destroy(kit.gameObject);
            throw new InvalidOperationException(
                "Cloned build metadata is shared with the vanilla prefab.");
        }

        structure.BuildStates[0].Tool.ToolEntry = kit;
        WorldManager.Instance.SourcePrefabs.Add(structure);
        WorldManager.Instance.SourcePrefabs.Add(kit);
        Prefab.RegisterExisting(structure);
        Prefab.RegisterExisting(kit);

        ProbeLog.Write(
            "prefab-registration",
            "staged",
            new
            {
                phase = "prefab-ready-fallback",
                structureHash = structure.PrefabHash,
                kitHash = kit.PrefabHash,
                sourcePrefabPhase = false,
                sourcePrefabCatalogued = true,
                isCustomThing = structure.IsCustomThing,
                isolatedBuildMetadata,
            });
    }
}
