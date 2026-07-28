using System;
using System.Collections.Generic;
using System.Diagnostics;
using Assets.Scripts.Networking;
using Assets.Scripts.Networks;
using Assets.Scripts.Objects.Electrical;
using Assets.Scripts.Objects.Pipes;

namespace StationeersBridge.Feasibility;

internal static class WorldProbe
{
    public static void Run(
        string sceneName,
        string fixtureNamePrefix,
        bool allowSourceMutation,
        string sourceMutationHousingName,
        string sourceMutationConfirmation)
    {
        try
        {
            var started = Stopwatch.StartNew();
            var allocationBefore = GC.GetTotalMemory(forceFullCollection: false);
            var anchors = new List<LogicMemory>();

            foreach (var device in Device.AllDevices.Active())
            {
                if (device is not LogicMemory anchor)
                {
                    continue;
                }

                if (anchor.PrefabName == PrefabProbe.StructurePrefabName
                    || (anchor.CustomName ?? string.Empty).Trim().StartsWith(
                        fixtureNamePrefix,
                        StringComparison.Ordinal))
                {
                    anchors.Add(anchor);
                }
            }

            var chipAppearances = 0;
            var uniqueChips = new HashSet<long>();
            var mutatedChips = new HashSet<long>();
            var networkHandles = new HashSet<long>();
            var maxAnchorDurationTicks = 0L;

            foreach (var anchor in anchors)
            {
                var anchorStarted = Stopwatch.StartNew();
                for (var port = 0; port < 2; port += 1)
                {
                    var network = anchor.GetNetwork(port);
                    if (network is null)
                    {
                        ProbeLog.Write(
                            "network-traversal",
                            "disconnected",
                            new
                            {
                                anchorReference = anchor.ReferenceId.ToString(),
                                anchorName = anchor.CustomName,
                                anchorPrefab = anchor.PrefabName,
                                port,
                            });
                        continue;
                    }

                    networkHandles.Add(network.ReferenceId);
                    ProbeNetwork(
                        anchor,
                        port,
                        network,
                        fixtureNamePrefix,
                        allowSourceMutation,
                        sourceMutationHousingName,
                        sourceMutationConfirmation,
                        uniqueChips,
                        mutatedChips,
                        ref chipAppearances);
                }

                anchorStarted.Stop();
                maxAnchorDurationTicks = Math.Max(
                    maxAnchorDurationTicks,
                    anchorStarted.ElapsedTicks);
            }

            started.Stop();
            var allocationAfter = GC.GetTotalMemory(forceFullCollection: false);
            ProbeLog.Write(
                "world-enumeration",
                "observed",
                new
                {
                    scene = sceneName,
                    role = NetworkManager.NetworkRole.ToString(),
                    isClient = NetworkManager.IsClient,
                    isServer = NetworkManager.IsServer,
                    localClientId = NetworkManager.LocalClientId.ToString(),
                    threadId = Environment.CurrentManagedThreadId,
                    anchorCount = anchors.Count,
                    physicalNetworkCount = networkHandles.Count,
                    chipAppearances,
                    uniqueChipCount = uniqueChips.Count,
                    durationTicks = started.ElapsedTicks,
                    durationMilliseconds = started.Elapsed.TotalMilliseconds,
                    maxAnchorTicks = maxAnchorDurationTicks,
                    allocationDeltaBytes = allocationAfter - allocationBefore,
                });
        }
        catch (Exception exception)
        {
            ProbeLog.Failure("world-enumeration", exception);
        }
    }

    private static void ProbeNetwork(
        LogicMemory anchor,
        int port,
        CableNetwork network,
        string fixtureNamePrefix,
        bool allowSourceMutation,
        string sourceMutationHousingName,
        string sourceMutationConfirmation,
        ISet<long> uniqueChips,
        ISet<long> mutatedChips,
        ref int chipAppearances)
    {
        var housingCount = 0;
        foreach (var device in network.DataDeviceList)
        {
            if (device is not CircuitHousing housing)
            {
                continue;
            }

            housingCount += 1;
            var chip = housing._ProgrammableChipSlot?.Get<ProgrammableChip>();
            if (chip is null)
            {
                continue;
            }

            chipAppearances += 1;
            uniqueChips.Add(chip.ReferenceId);
            var source = housing.GetSourceCode() ?? string.Empty;
            var isLua = string.Equals(
                chip.GetType().FullName,
                "StationeersLua.IntegratedCircuitLua",
                StringComparison.Ordinal);

            ProbeLog.Write(
                "chip-classification",
                "observed",
                new
                {
                    anchorReference = anchor.ReferenceId.ToString(),
                    anchorName = anchor.CustomName,
                    port,
                    networkReference = network.ReferenceId.ToString(),
                    housingReference = housing.ReferenceId.ToString(),
                    housingName = housing.CustomName,
                    chipReference = chip.ReferenceId.ToString(),
                    chipPrefab = chip.PrefabName,
                    runtimeType = chip.GetType().FullName,
                    language = isLua ? "lua" : "ic10",
                    sourceLength = source.Length,
                    sourceSha256 = SourceHash.Sha256(source),
                    chipLastEditedId = chip.LastEditedId.ToString(),
                    housingLastEditedBy = housing.LastEditedBy.ToString(),
                    compilationError = chip.CompilationError,
                    hasAuthority = housing.HasAuthority,
                });

            if (allowSourceMutation
                && !isLua
                && chip.PrefabName == "ItemIntegratedCircuit10"
                && housing.HasAuthority
                && string.Equals(
                    (housing.CustomName ?? string.Empty).Trim(),
                    sourceMutationHousingName.Trim(),
                    StringComparison.Ordinal)
                && string.Equals(
                    sourceMutationConfirmation,
                    "MUTATE_AND_RESTORE_P302_SOURCE",
                    StringComparison.Ordinal)
                && mutatedChips.Add(chip.ReferenceId))
            {
                ProbeSourceMutation(housing, chip, source);
            }
            else if (allowSourceMutation
                && string.Equals(
                    (housing.CustomName ?? string.Empty).Trim(),
                    sourceMutationHousingName.Trim(),
                    StringComparison.Ordinal)
                && !string.Equals(
                    sourceMutationConfirmation,
                    "MUTATE_AND_RESTORE_P302_SOURCE",
                    StringComparison.Ordinal))
            {
                ProbeLog.Write(
                    "ic10-source-mutation",
                    "blocked",
                    new
                    {
                        housingReference = housing.ReferenceId.ToString(),
                        reason = "confirmation value missing or incorrect",
                    });
            }
        }

        ProbeLog.Write(
            "network-traversal",
            "observed",
            new
            {
                anchorReference = anchor.ReferenceId.ToString(),
                anchorName = anchor.CustomName,
                port,
                networkReference = network.ReferenceId.ToString(),
                attachedDeviceCount = network.DataDeviceList.Count,
                housingCount,
                threadId = Environment.CurrentManagedThreadId,
                authoritativeRead = NetworkManager.IsServer || !NetworkManager.IsClient,
            });
    }

    private static void ProbeSourceMutation(
        CircuitHousing housing,
        ProgrammableChip chip,
        string originalSource)
    {
        var mutation = originalSource + "\n# P3.02 reversible source probe";
        var started = Stopwatch.StartNew();
        var mutationAccepted = false;
        var compilationErrorAfterMutation = false;
        var restored = false;
        Exception? mutationException = null;
        try
        {
            housing.SetSourceCode(mutation);
            var observedMutation = housing.GetSourceCode() ?? string.Empty;
            mutationAccepted = string.Equals(
                mutation,
                observedMutation,
                StringComparison.Ordinal);
            compilationErrorAfterMutation = chip.CompilationError;
        }
        catch (Exception exception)
        {
            mutationException = exception;
        }
        finally
        {
            try
            {
                housing.SetSourceCode(originalSource);
                restored = string.Equals(
                    originalSource,
                    housing.GetSourceCode(),
                    StringComparison.Ordinal);
            }
            catch (Exception restoreException)
            {
                ProbeLog.Write(
                    "ic10-source-restore",
                    "failed",
                    new
                    {
                        housingReference = housing.ReferenceId.ToString(),
                        exceptionType = restoreException.GetType().FullName,
                        restoreException.Message,
                    });
            }
        }

        started.Stop();
        ProbeLog.Write(
            "ic10-source-mutation",
            mutationException is null && mutationAccepted && restored
                ? "observed"
                : "failed",
            new
            {
                housingReference = housing.ReferenceId.ToString(),
                chipReference = chip.ReferenceId.ToString(),
                originalSha256 = SourceHash.Sha256(originalSource),
                mutationSha256 = SourceHash.Sha256(mutation),
                mutationAccepted,
                restored,
                compilationErrorAfterMutation,
                exceptionType = mutationException?.GetType().FullName,
                exceptionMessage = mutationException?.Message,
                chipLastEditedIdAfter = chip.LastEditedId.ToString(),
                housingLastEditedByAfter = housing.LastEditedBy.ToString(),
                durationTicks = started.ElapsedTicks,
                durationMilliseconds = started.Elapsed.TotalMilliseconds,
            });
    }
}
