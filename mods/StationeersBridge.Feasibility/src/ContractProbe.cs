using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;

namespace StationeersBridge.Feasibility;

internal static class ContractProbe
{
    private sealed class Contract
    {
        public Contract(string assembly, string type, params string[] members)
        {
            Assembly = assembly;
            Type = type;
            Members = members;
        }

        public string Assembly { get; }
        public string Type { get; }
        public string[] Members { get; }
    }

    private static readonly Contract[] Contracts =
    {
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Util.UnityMainThreadDispatcher",
            "M:Enqueue",
            "M:Instance",
            "M:Exists"),
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Objects.Prefab",
            "E:OnPrefabsLoaded",
            "M:RegisterExisting",
            "M:Find"),
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Objects.Thing",
            "F:PrefabName",
            "F:PrefabHash",
            "P:ReferenceId",
            "P:CustomName",
            "P:HasAuthority"),
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Objects.Pipes.Device",
            "F:AllDevices",
            "P:DataCableNetwork",
            "M:GetNetwork"),
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Networks.CableNetwork",
            "F:AllCableNetworks",
            "P:ReferenceId",
            "P:DataDeviceList"),
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Objects.Electrical.CircuitHousing",
            "P:_ProgrammableChipSlot",
            "P:LastEditedBy",
            "M:GetSourceCode",
            "M:SetSourceCode"),
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Objects.Electrical.ProgrammableChip",
            "F:SourceCode",
            "P:LastEditedId",
            "P:CompilationError",
            "M:GetSourceCode",
            "M:SetSourceCode",
            "M:SendUpdate"),
        new(
            "Assembly-CSharp",
            "Assets.Scripts.Networking.NetworkManager",
            "P:IsClient",
            "P:IsServer",
            "P:LocalClientId"),
        new(
            "LaunchPadBooster",
            "LaunchPadBooster.Mod",
            "M:AddPrefabs",
            "M:RegisterNetworkMessage"),
        new(
            "LaunchPadBooster",
            "LaunchPadBooster.Networking.IModNetworking",
            "M:RegisterRPC"),
        new(
            "LaunchPadBooster",
            "LaunchPadBooster.Networking.ModNetworkingExtensions",
            "M:CallHost"),
    };

    public static void Run(IReadOnlyCollection<Assembly> modAssemblies)
    {
        var loaded = AppDomain.CurrentDomain.GetAssemblies()
            .Concat(modAssemblies)
            .GroupBy(assembly => assembly.GetName().Name ?? string.Empty)
            .ToDictionary(group => group.Key, group => group.First());

        foreach (var contract in Contracts)
        {
            if (!loaded.TryGetValue(contract.Assembly, out var assembly))
            {
                ProbeLog.Write(
                    "api-contract",
                    "missing",
                    new { contract.Assembly, contract.Type, reason = "assembly" });
                continue;
            }

            var type = assembly.GetType(contract.Type, throwOnError: false);
            if (type is null)
            {
                ProbeLog.Write(
                    "api-contract",
                    "missing",
                    new
                    {
                        contract.Assembly,
                        contract.Type,
                        reason = "type",
                        assemblyVersion = assembly.GetName().Version?.ToString(),
                    });
                continue;
            }

            foreach (var memberSpec in contract.Members)
            {
                var memberName = memberSpec.Substring(2);
                var kind = memberSpec[0];
                var found = kind switch
                {
                    'F' => type.GetField(
                        memberName,
                        BindingFlags.Public
                            | BindingFlags.NonPublic
                            | BindingFlags.Static
                            | BindingFlags.Instance) is not null,
                    'P' => type.GetProperty(
                        memberName,
                        BindingFlags.Public
                            | BindingFlags.NonPublic
                            | BindingFlags.Static
                            | BindingFlags.Instance) is not null,
                    'E' => type.GetEvent(
                        memberName,
                        BindingFlags.Public
                            | BindingFlags.NonPublic
                            | BindingFlags.Static
                            | BindingFlags.Instance) is not null,
                    'M' => type.GetMethods(
                            BindingFlags.Public
                                | BindingFlags.NonPublic
                                | BindingFlags.Static
                                | BindingFlags.Instance)
                        .Any(method => method.Name == memberName),
                    _ => false,
                };

                ProbeLog.Write(
                    "api-contract",
                    found ? "observed" : "missing",
                    new
                    {
                        contract.Assembly,
                        assemblyVersion = assembly.GetName().Version?.ToString(),
                        contract.Type,
                        member = memberSpec,
                    });
            }
        }

        ProbeOptionalLua(loaded);
    }

    private static void ProbeOptionalLua(IReadOnlyDictionary<string, Assembly> loaded)
    {
        if (!loaded.TryGetValue("StationeersLua", out var assembly))
        {
            ProbeLog.Write(
                "optional-mod-detection",
                "absent",
                new { assembly = "StationeersLua", hardDependency = false });
            return;
        }

        var luaChip = assembly.GetType(
            "StationeersLua.IntegratedCircuitLua",
            throwOnError: false);
        var hooks = assembly.GetType(
            "StationeersLua.LuaChipRuntimePublicHooks",
            throwOnError: false);
        ProbeLog.Write(
            "optional-mod-detection",
            luaChip is not null ? "observed" : "partial",
            new
            {
                assembly = assembly.GetName().Name,
                version = assembly.GetName().Version?.ToString(),
                chipType = luaChip?.FullName,
                sourceChangedEvent =
                    hooks?.GetEvent(
                        "ChipSourceChanged",
                        BindingFlags.Public | BindingFlags.Static) is not null,
                hardDependency = false,
            });
    }
}
