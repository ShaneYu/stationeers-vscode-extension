using System.Text.Json.Serialization;

namespace StationeersBridge.Core;

public sealed record BridgeHello(
    string ApiVersion, string BridgeVersion, string GameVersion, string InstanceId,
    string Role, BridgeWorld World, BridgeCapabilities Capabilities, BridgeLimits Limits);
public sealed record BridgeWorld(bool Loaded, string Epoch, string Revision);
public sealed record BridgeCapabilities(bool ScopeDiscovery, bool Ic10SourceRead, bool Ic10SourceWrite, bool MultiplayerRelay, bool EventStream);
public sealed record BridgeLimits(int MaxSourceBytes, int MaxRequestsPerSecond, int MaxConnections);
public sealed record BridgeSnapshot(string WorldEpoch, string Revision, IReadOnlyList<BridgeScope> Scopes, IReadOnlyList<BridgeChip> Chips, IReadOnlyList<BridgeWarning> Warnings);
public sealed record BridgeScope(string ScopeId, string Name, string? Disambiguator, int AnchorCount, IReadOnlyList<string> ChipIds);
public sealed record BridgeChip(string ChipId, string HousingReferenceId, string HousingName, string HousingPrefab, string ChipPrefab, string Language, bool Powered, BridgeSourceSummary Source);
public sealed record BridgeSourceSummary(bool Readable, bool Writable, string Version, string Sha256);
public sealed record BridgeSource(string WorldEpoch, string ChipId, string HousingReferenceId, string Language, string Version, string Sha256, string Source);
public sealed record BridgeWarning(string Code, string Message, string AnchorReferenceId);
public sealed record BridgeEvent(string ApiVersion, string EventId, string WorldEpoch, string Revision, string Type, IReadOnlyDictionary<string, string> Data);
public sealed record BridgeError(string Code, string Message, string RequestId, bool Retryable, IReadOnlyDictionary<string, object?> Details);
public sealed record BridgeErrorEnvelope(BridgeError Error);
