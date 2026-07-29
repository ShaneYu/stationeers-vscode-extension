using System.Collections.Generic;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using Assets.Scripts.Networking;
using Assets.Scripts.Objects;
using Assets.Scripts.Objects.Electrical;
using Assets.Scripts.Objects.Pipes;
using Assets.Scripts.Networks;
using StationeersBridge.RemoteNetwork.Core;

namespace StationeersBridge.RemoteNetwork;

internal sealed class RemoteNetworkIndex
{
    private DiscoverySnapshot _snapshot = new("uninitialized", new DiscoveryScope[0], new ScopeWarning[0]);
    private Dictionary<string, TargetIdentity> _targets = new();
    internal DiscoverySnapshot Snapshot => _snapshot;

    internal bool Reconcile(string worldEpoch)
    {
        var previousSignature = SnapshotSignature(_snapshot);
        var anchors = new List<RemoteNetworkAnchor>();
        foreach (var device in Device.AllDevices.Active())
        {
            if (device.PrefabName != RemoteNetworkPrefab.StructurePrefabName || device is not LogicMemory anchor) continue;
            var attachments = new List<NetworkAttachment>();
            for (var port = 0; port < 2; port++)
            {
                var network = anchor.GetNetwork(port);
                if (network is null) continue;
                var chips = network.DataDeviceList.SelectMany(DescribeChips)
                    .GroupBy(chip => chip.ChipReference).Select(group => group.First()).ToArray();
                attachments.Add(new NetworkAttachment(anchor.ReferenceId.ToString(), port, network.ReferenceId.ToString(), chips));
            }
            anchors.Add(new RemoteNetworkAnchor(anchor.ReferenceId.ToString(), anchor.CustomName, attachments));
        }
        _snapshot = DiscoveryGrouping.Group(worldEpoch, anchors);
        var discovered = new HashSet<string>(
            _snapshot.Scopes.SelectMany(scope => scope.Chips).Select(chip => chip.ChipReference),
            System.StringComparer.Ordinal);
        _targets = ResolveTargetIdentities(discovered);
        return !string.Equals(previousSignature, SnapshotSignature(_snapshot), System.StringComparison.Ordinal);
    }

    private static string SnapshotSignature(DiscoverySnapshot snapshot)
    {
        var builder = new StringBuilder(snapshot.WorldEpoch);
        foreach (var scope in snapshot.Scopes.OrderBy(item => item.NetworkReference, System.StringComparer.Ordinal).ThenBy(item => item.Label, System.StringComparer.Ordinal))
        {
            builder.Append('\u001f').Append(scope.NetworkReference).Append('\u001f').Append(scope.Label);
            foreach (var anchor in scope.AnchorReferences.OrderBy(item => item, System.StringComparer.Ordinal)) builder.Append('\u001f').Append(anchor);
            foreach (var chip in scope.Chips.OrderBy(item => item.ChipReference, System.StringComparer.Ordinal))
            {
                builder.Append('\u001f').Append(chip.ChipReference).Append('\u001f').Append(chip.HousingReference)
                    .Append('\u001f').Append(chip.Language).Append('\u001f').Append(chip.Powered)
                    .Append('\u001f').Append(chip.Source?.Version).Append('\u001f').Append(chip.Source?.Sha256);
            }
            foreach (var attachment in scope.Attachments.OrderBy(item => item.AnchorReference, System.StringComparer.Ordinal).ThenBy(item => item.Port))
            {
                builder.Append('\u001f').Append(attachment.AnchorReference).Append('\u001f').Append(attachment.Port).Append('\u001f').Append(attachment.NetworkReference);
            }
        }
        foreach (var warning in snapshot.Warnings.OrderBy(item => item.AnchorReference, System.StringComparer.Ordinal))
            builder.Append('\u001f').Append(warning.AnchorReference).Append('\u001f').Append(warning.Message);
        return builder.ToString();
    }

    private static IEnumerable<ChipSummary> DescribeChips(Device device)
    {
        if (device is CircuitHousing housing)
        {
            var chip = housing._ProgrammableChipSlot?.Get<ProgrammableChip>();
            if (chip is null) yield break;

            var language = Language(chip);

            yield return new ChipSummary(
                housing.ReferenceId.ToString(),
                chip.ReferenceId.ToString(),
                Name(housing),
                language,
                IsPowered(housing, chip),
                housing.PrefabName,
                chip.PrefabName,
                language == ChipLanguage.Ic10 ? Metadata(housing.GetSourceCode() ?? string.Empty, chip.LastEditedId.ToString()) : null);
            yield break;
        }

        // Large consoles hold a motherboard rather than a CircuitHousing.
        // StationeersLua identifies a Lua circuitboard by CurrentMotherboard's
        // ReferenceId as its housing, while the nested Lua chip has a separate
        // ReferenceId that the public vanilla API does not expose here.
        // Publish the current motherboard as a housing identity so the client
        // can safely resolve exactly one StationeersLua chip inside it.
        if (device is Console console && console.HasMotherboard && console.CurrentMotherboard is not null)
        {
            var motherboard = console.CurrentMotherboard;
            var programmableMotherboard = motherboard as Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard;
            var consoleSource = programmableMotherboard?.GetSourceCode().ToString() ?? string.Empty;
            var consoleLanguage = Language(motherboard, motherboard.MasterMotherboard, motherboard.SourcePrefab, motherboard.ParentSlot?.Get<Thing>(), programmableMotherboard is null ? null : consoleSource);
            var usesHousingIdentity = consoleLanguage == ChipLanguage.Lua;
            var sourceHostReference = usesHousingIdentity
                ? motherboard.ReferenceId.ToString()
                : console.ReferenceId.ToString();
            yield return new ChipSummary(
                sourceHostReference,
                sourceHostReference,
                Name(console),
                consoleLanguage,
                IsPowered(console, motherboard),
                console.PrefabName,
                motherboard.PrefabName,
                consoleLanguage == ChipLanguage.Ic10 ? Metadata(consoleSource, Version(motherboard)) : null,
                usesHousingIdentity);
        }
    }

    internal ChipSourceReadResult ReadSource(string chipReference, string worldEpoch)
    {
        if (!string.Equals(_snapshot.WorldEpoch, worldEpoch, System.StringComparison.Ordinal))
            return new(ChipSourceReadStatus.StaleWorld);

        foreach (var device in Device.AllDevices.Active())
        {
            if (device is CircuitHousing housing)
            {
                var chip = housing._ProgrammableChipSlot?.Get<ProgrammableChip>();
                if (chip is null || chip.ReferenceId.ToString() != chipReference) continue;
                if (Language(chip) != ChipLanguage.Ic10) return new(ChipSourceReadStatus.Lua);
                var source = housing.GetSourceCode() ?? string.Empty;
                return new(ChipSourceReadStatus.Success, new ChipSource(
                    worldEpoch, chip.ReferenceId.ToString(), housing.ReferenceId.ToString(), "ic10",
                    Encoding.UTF8.GetByteCount(source), chip.LastEditedId.ToString(), ChipSourceWriteValidation.Hash(source), source));
            }

            if (device is Console console && console.HasMotherboard && console.CurrentMotherboard is Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard motherboard && console.ReferenceId.ToString() == chipReference)
            {
                var source = motherboard.GetSourceCode().ToString();
                var language = Language(motherboard, motherboard.MasterMotherboard, motherboard.SourcePrefab, motherboard.ParentSlot?.Get<Thing>(), source);
                if (language != ChipLanguage.Ic10) return new(ChipSourceReadStatus.Lua);
                return new(ChipSourceReadStatus.Success, new ChipSource(
                    worldEpoch, chipReference, console.ReferenceId.ToString(), "ic10",
                    Encoding.UTF8.GetByteCount(source), Version(motherboard), ChipSourceWriteValidation.Hash(source), source));
            }
        }

        return new(ChipSourceReadStatus.UnknownChip);
    }

    internal ChipSourceWriteResult WriteSource(
        string chipReference,
        ChipSourceWriteRequest request,
        int maxSourceBytes)
    {
        if (!string.Equals(_snapshot.WorldEpoch, request.WorldEpoch, System.StringComparison.Ordinal))
            return new(ChipSourceWriteStatus.StaleWorld);
        if (!_targets.TryGetValue(chipReference, out var expectedTarget))
            return new(ChipSourceWriteStatus.UnknownChip);

        var validation = ChipSourceWriteValidation.Validate(request, maxSourceBytes);
        if (validation is not null) return new(validation.Value);
        if (!string.Equals(
                request.SourceSha256,
                ChipSourceWriteValidation.Hash(request.Source),
                System.StringComparison.Ordinal))
            return new(ChipSourceWriteStatus.InvalidSource);

        foreach (var device in Device.AllDevices.Active())
        {
            if (device is CircuitHousing housing)
            {
                var chip = housing._ProgrammableChipSlot?.Get<ProgrammableChip>();
                if (chip is null || chip.ReferenceId.ToString() != chipReference) continue;
                if (!expectedTarget.Matches(housing.ReferenceId.ToString(), chip.ReferenceId.ToString()))
                    return new(ChipSourceWriteStatus.StaleTarget);
                if (Language(chip) != ChipLanguage.Ic10) return new(ChipSourceWriteStatus.Lua);
                if (!housing.HasAuthority) return new(ChipSourceWriteStatus.Denied);

                var currentSource = housing.GetSourceCode() ?? string.Empty;
                var current = DescribeSource(
                    request.WorldEpoch,
                    chipReference,
                    housing.ReferenceId.ToString(),
                    chip.LastEditedId.ToString(),
                    currentSource);
                if (ChipSourceWriteValidation.HasConflict(request, current))
                    return new(ChipSourceWriteStatus.Conflict, Current: current);

                try
                {
                    housing.SetSourceCode(request.Source);
                    chip.SendUpdate();
                    return VerifyApplied(
                        request,
                        housing.GetSourceCode() ?? string.Empty,
                        chip.LastEditedId.ToString(),
                        housing.ReferenceId.ToString());
                }
                catch
                {
                    return new(ChipSourceWriteStatus.Unavailable);
                }
            }

            if (device is Console console &&
                console.HasMotherboard &&
                console.CurrentMotherboard is Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard motherboard &&
                console.ReferenceId.ToString() == chipReference)
            {
                if (!expectedTarget.Matches(console.ReferenceId.ToString(), motherboard.ReferenceId.ToString()))
                    return new(ChipSourceWriteStatus.StaleTarget);
                var currentSource = motherboard.GetSourceCode().ToString();
                if (Language(motherboard, motherboard.MasterMotherboard, motherboard.SourcePrefab, motherboard.ParentSlot?.Get<Thing>(), currentSource) != ChipLanguage.Ic10)
                    return new(ChipSourceWriteStatus.Lua);
                if (!console.HasAuthority || !motherboard.HasAuthority)
                    return new(ChipSourceWriteStatus.Denied);

                var current = DescribeSource(
                    request.WorldEpoch,
                    chipReference,
                    console.ReferenceId.ToString(),
                    Version(motherboard),
                    currentSource);
                if (ChipSourceWriteValidation.HasConflict(request, current))
                    return new(ChipSourceWriteStatus.Conflict, Current: current);

                try
                {
                    motherboard.SetSourceCode(request.Source);
                    motherboard.SendUpdate();
                    return VerifyApplied(
                        request,
                        motherboard.GetSourceCode().ToString(),
                        Version(motherboard),
                        console.ReferenceId.ToString());
                }
                catch
                {
                    return new(ChipSourceWriteStatus.Unavailable);
                }
            }
        }

        return new(ChipSourceWriteStatus.StaleTarget);
    }

    private ChipSourceWriteResult VerifyApplied(
        ChipSourceWriteRequest request,
        string observedSource,
        string observedVersion,
        string housingReference)
    {
        var observedHash = ChipSourceWriteValidation.Hash(observedSource);
        if (!string.Equals(observedHash, request.SourceSha256, System.StringComparison.Ordinal))
            return new(ChipSourceWriteStatus.Unavailable);

        return new(
            ChipSourceWriteStatus.Applied,
            new ChipSourceWriteResponse(
                request.WorldEpoch,
                FindChipReference(housingReference),
                housingReference,
                observedVersion,
                observedHash,
                Encoding.UTF8.GetByteCount(observedSource),
                true));
    }

    private static ChipSource DescribeSource(
        string worldEpoch,
        string chipReference,
        string housingReference,
        string version,
        string source) =>
        new(
            worldEpoch,
            chipReference,
            housingReference,
            "ic10",
            Encoding.UTF8.GetByteCount(source),
            version,
            ChipSourceWriteValidation.Hash(source),
            source);

    private string FindChipReference(string housingReference) =>
        _targets.First(pair => pair.Value.HousingReference == housingReference).Key;

    private static Dictionary<string, TargetIdentity> ResolveTargetIdentities(HashSet<string> discovered)
    {
        var targets = new Dictionary<string, TargetIdentity>(System.StringComparer.Ordinal);
        foreach (var device in Device.AllDevices.Active())
        {
            if (device is CircuitHousing housing)
            {
                var chip = housing._ProgrammableChipSlot?.Get<ProgrammableChip>();
                if (chip is null) continue;
                var chipReference = chip.ReferenceId.ToString();
                if (discovered.Contains(chipReference))
                    targets[chipReference] = new(housing.ReferenceId.ToString(), chipReference);
                continue;
            }

            if (device is Console console &&
                console.HasMotherboard &&
                console.CurrentMotherboard is Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard motherboard)
            {
                var chipReference = console.ReferenceId.ToString();
                if (discovered.Contains(chipReference))
                    targets[chipReference] = new(console.ReferenceId.ToString(), motherboard.ReferenceId.ToString());
            }
        }
        return targets;
    }

    private static ChipSourceMetadata Metadata(string source, string version) =>
        new(Encoding.UTF8.GetByteCount(source), version, ChipSourceWriteValidation.Hash(source));

    private static string Version(object value)
    {
        var property = value.GetType().GetProperty("LastEditedId", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.Public);
        return property?.GetValue(value)?.ToString() ?? "0";
    }

    private static string Name(Device device) => string.IsNullOrWhiteSpace(device.CustomName) ? device.PrefabName : device.CustomName;

    private static bool IsPowered(Device device, object poweredObject)
    {
        var poweredValue = poweredObject is ProgrammableChip chip ? chip.PoweredValue :
            poweredObject is Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard motherboard ? motherboard.PoweredValue : 0;
        return device.Powered || device.PoweredValue > 0 ||
            poweredObject is ProgrammableChip { Powered: true } || poweredObject is Assets.Scripts.Objects.Motherboards.ProgrammableChipMotherboard { Powered: true } ||
            poweredValue > 0;
    }

    private static ChipLanguage Language(params object?[] values)
    {
        var identity = string.Join(" ", values.Where(value => value is not null).SelectMany(value => Identity(value!)));
        return identity.IndexOf("lua", System.StringComparison.OrdinalIgnoreCase) >= 0
            ? ChipLanguage.Lua
            : values.OfType<string>().Any(LooksLikeLua)
                ? ChipLanguage.Lua
            : values.Any(value => value is ProgrammableChip) || identity.IndexOf("ic10", System.StringComparison.OrdinalIgnoreCase) >= 0
                ? ChipLanguage.Ic10
                : ChipLanguage.Unknown;
    }

    private static bool LooksLikeLua(string source) =>
        source.IndexOf("local ", System.StringComparison.OrdinalIgnoreCase) >= 0 ||
        source.IndexOf("function ", System.StringComparison.OrdinalIgnoreCase) >= 0 ||
        source.IndexOf("require(", System.StringComparison.OrdinalIgnoreCase) >= 0 ||
        source.IndexOf(" then", System.StringComparison.OrdinalIgnoreCase) >= 0;

    private static IEnumerable<string> Identity(object value)
    {
        yield return value.GetType().FullName ?? string.Empty;
        if (value is Thing thing) yield return thing.PrefabName;

        foreach (var propertyName in new[] { "DisplayName", "SpawnableName", "TrackableName", "PrefabName" })
        {
            var property = value.GetType().GetProperty(propertyName, System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.Public);
            if (property?.GetValue(value) is string text) yield return text;
        }

        var getPrefabName = value.GetType().GetMethod("GetPrefabName", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.Public, null, System.Type.EmptyTypes, null);
        if (getPrefabName?.Invoke(value, null) is string prefabName) yield return prefabName;
    }

    private sealed record TargetIdentity(string HousingReference, string SourceReference)
    {
        internal bool Matches(string housingReference, string sourceReference) =>
            HousingReference == housingReference && SourceReference == sourceReference;
    }
}
