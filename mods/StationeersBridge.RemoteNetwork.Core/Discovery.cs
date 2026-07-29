using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Security.Cryptography;
using System.Text;

namespace StationeersBridge.RemoteNetwork.Core;

public enum ChipLanguage { Ic10, Lua, Unknown }

public sealed record ChipSourceMetadata(int Length, string Version, string Sha256);

public sealed record ChipSummary(
    string HousingReference,
    string ChipReference,
    string HousingName,
    ChipLanguage Language,
    bool Powered = false,
    string HousingPrefab = "unknown",
    string ChipPrefab = "unknown",
    ChipSourceMetadata? Source = null,
    bool ChipReferenceIsHousing = false);

public sealed record ChipSource(
    string WorldEpoch,
    string ChipReference,
    string HousingReference,
    string Language,
    int Length,
    string Version,
    string Sha256,
    string Source);

public enum ChipSourceReadStatus { Success, StaleWorld, UnknownChip, Lua, Unavailable }

public sealed record ChipSourceReadResult(ChipSourceReadStatus Status, ChipSource? Source = null);

public sealed record ChipSourceWriteRequest(
    string RequestId,
    string WorldEpoch,
    string ExpectedVersion,
    string ExpectedSha256,
    string Source,
    string SourceSha256);

public sealed record ChipSourceWriteResponse(
    string WorldEpoch,
    string ChipReference,
    string HousingReference,
    string Version,
    string Sha256,
    int Length,
    bool Applied);

public enum ChipSourceWriteStatus
{
    Applied,
    StaleWorld,
    StaleTarget,
    UnknownChip,
    Denied,
    Lua,
    Conflict,
    Oversized,
    InvalidSource,
    Unavailable,
}

public sealed record ChipSourceWriteResult(
    ChipSourceWriteStatus Status,
    ChipSourceWriteResponse? Response = null,
    ChipSource? Current = null);

public static class ChipSourceWriteValidation
{
    public static ChipSourceWriteStatus? Validate(ChipSourceWriteRequest request, int maxSourceBytes)
    {
        if (request is null) throw new ArgumentNullException(nameof(request));
        if (maxSourceBytes < 1) throw new ArgumentOutOfRangeException(nameof(maxSourceBytes));
        if (string.IsNullOrWhiteSpace(request.RequestId) ||
            string.IsNullOrWhiteSpace(request.WorldEpoch) ||
            string.IsNullOrWhiteSpace(request.ExpectedVersion) ||
            request.Source is null ||
            !IsSha256(request.ExpectedSha256) ||
            !IsSha256(request.SourceSha256))
            return ChipSourceWriteStatus.InvalidSource;
        if (Encoding.UTF8.GetByteCount(request.Source) > maxSourceBytes)
            return ChipSourceWriteStatus.Oversized;
        if (request.Source.Any(character =>
                character != '\r' && character != '\n' && character != '\t' &&
                (character < ' ' || char.IsSurrogate(character))))
            return ChipSourceWriteStatus.InvalidSource;
        if (!string.Equals(request.SourceSha256, Hash(request.Source), StringComparison.Ordinal))
            return ChipSourceWriteStatus.InvalidSource;
        return null;
    }

    public static bool HasConflict(ChipSourceWriteRequest request, ChipSource current)
    {
        if (request is null) throw new ArgumentNullException(nameof(request));
        if (current is null) throw new ArgumentNullException(nameof(current));
        if (!string.Equals(request.WorldEpoch, current.WorldEpoch, StringComparison.Ordinal))
            return true;
        if (!string.Equals(request.ExpectedSha256, current.Sha256, StringComparison.Ordinal))
            return true;
        return current.Version != "0" &&
            !string.Equals(request.ExpectedVersion, current.Version, StringComparison.Ordinal);
    }

    public static bool IsSha256(string? value) =>
        value is not null && value.Length == 64 &&
        value.All(character => character is >= '0' and <= '9' or >= 'a' and <= 'f');

    public static string Hash(string source) =>
        string.Concat(new SHA256Managed().ComputeHash(Encoding.UTF8.GetBytes(source))
            .Select(byteValue => byteValue.ToString("x2")));
}

public sealed record NetworkAttachment(string AnchorReference, int Port, string NetworkReference, IReadOnlyList<ChipSummary> Chips);

public sealed record RemoteNetworkAnchor(string AnchorReference, string? Label, IReadOnlyList<NetworkAttachment> Attachments)
{
    public RemoteNetworkAnchor(string anchorReference, string? label, IEnumerable<NetworkAttachment> attachments)
        : this(anchorReference, label, new ReadOnlyCollection<NetworkAttachment>(attachments.ToArray())) { }
}

public sealed record ScopeWarning(string AnchorReference, string Message);

public sealed record DiscoveryScope(
    string WorldEpoch,
    string NetworkReference,
    string Label,
    int AnchorCount,
    IReadOnlyList<string> AnchorReferences,
    IReadOnlyList<ChipSummary> Chips,
    IReadOnlyList<NetworkAttachment> Attachments)
{
    public DiscoveryScope(
        string worldEpoch, string networkReference, string label,
        IEnumerable<string> anchorReferences, IEnumerable<ChipSummary> chips,
        IEnumerable<NetworkAttachment> attachments)
        : this(
            worldEpoch,
            networkReference,
            label,
            anchorReferences.Count(),
            new ReadOnlyCollection<string>(anchorReferences.Distinct(StringComparer.Ordinal).ToArray()),
            new ReadOnlyCollection<ChipSummary>(chips.ToArray()),
            new ReadOnlyCollection<NetworkAttachment>(attachments.ToArray())) { }
}

public sealed record DiscoverySnapshot(
    string WorldEpoch,
    IReadOnlyList<DiscoveryScope> Scopes,
    IReadOnlyList<ScopeWarning> Warnings)
{
    public DiscoverySnapshot(string worldEpoch, IEnumerable<DiscoveryScope> scopes, IEnumerable<ScopeWarning> warnings)
        : this(
            worldEpoch,
            new ReadOnlyCollection<DiscoveryScope>(scopes.ToArray()),
            new ReadOnlyCollection<ScopeWarning>(warnings.ToArray())) { }
}

public static class DiscoveryGrouping
{
    public static DiscoverySnapshot Group(string worldEpoch, IEnumerable<RemoteNetworkAnchor> anchors)
    {
        var scopes = new Dictionary<(string Network, string Label), ScopeBuilder>();
        var warnings = new List<ScopeWarning>();

        foreach (var anchor in anchors)
        {
            var label = (anchor.Label ?? string.Empty).Trim();
            if (label.Length == 0)
            {
                warnings.Add(new ScopeWarning(anchor.AnchorReference, "Label this Remote Network before using it as a discovery scope."));
                continue;
            }

            foreach (var attachment in anchor.Attachments)
            {
                var key = (attachment.NetworkReference, label);
                if (!scopes.TryGetValue(key, out var builder))
                {
                    builder = new ScopeBuilder(worldEpoch, attachment.NetworkReference, label);
                    scopes.Add(key, builder);
                }

                builder.Add(anchor.AnchorReference, attachment);
            }
        }

        return new DiscoverySnapshot(worldEpoch, scopes.Values.Select(builder => builder.Build()), warnings);
    }

    private sealed class ScopeBuilder
    {
        private readonly HashSet<string> _anchors = new(StringComparer.Ordinal);
        private readonly Dictionary<string, ChipSummary> _chips = new(StringComparer.Ordinal);
        private readonly List<NetworkAttachment> _attachments = new();
        private readonly string _worldEpoch;
        private readonly string _networkReference;
        private readonly string _label;

        public ScopeBuilder(string worldEpoch, string networkReference, string label)
        {
            _worldEpoch = worldEpoch;
            _networkReference = networkReference;
            _label = label;
        }

        public void Add(string anchorReference, NetworkAttachment attachment)
        {
            _anchors.Add(anchorReference);
            _attachments.Add(attachment);
            foreach (var chip in attachment.Chips)
            {
                _chips.TryAdd(chip.ChipReference, chip);
            }
        }

        public DiscoveryScope Build() => new(
            _worldEpoch,
            _networkReference,
            _label,
            _anchors.OrderBy(value => value, StringComparer.Ordinal),
            _chips.Values.OrderBy(chip => chip.ChipReference, StringComparer.Ordinal),
            _attachments);
    }
}
