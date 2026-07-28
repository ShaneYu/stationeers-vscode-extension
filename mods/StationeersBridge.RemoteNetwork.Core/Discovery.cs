using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;

namespace StationeersBridge.RemoteNetwork.Core;

public enum ChipLanguage { Ic10, Lua, Unknown }

public sealed record ChipSummary(string HousingReference, string ChipReference, string HousingName, ChipLanguage Language);

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
