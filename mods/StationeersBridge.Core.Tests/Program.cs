using System.Text.Json;
using System.Net;
using System.Net.Http.Json;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using StationeersBridge.Core;

var root = args.Length == 1 ? Path.GetFullPath(args[0]) : FindRepositoryRoot();
var fixtureRoot = Path.Combine(root, "docs", "live-integration", "bridge", "v1", "fixtures");
var hello = Read("hello.json");
var scopes = Read("scopes.json");
var source = Read("source.json");
var error = Read("error-stale-world.json");
var eventFixture = Read("event.json");
Require(hello.GetProperty("apiVersion").GetString() == "1.0", "hello apiVersion");
Require(scopes.GetProperty("worldEpoch").GetString() is not null, "scope epoch");
Require(scopes.GetProperty("chips")[0].GetProperty("chipId").ValueKind == JsonValueKind.String, "chip ID must be a JSON string");
Require(source.GetProperty("chipId").ValueKind == JsonValueKind.String, "source chip ID must be a JSON string");
Require(source.GetProperty("language").GetString() == "ic10", "source fixture language");
Require(error.GetProperty("error").GetProperty("retryable").GetBoolean(), "safe retryable error envelope");
Require(eventFixture.GetProperty("type").GetString() == "snapshot.invalidated", "event type");
await RunWriteSliceTests();
Console.WriteLine("Bridge contract fixtures and write slice: 12 assertions passed.");

JsonElement Read(string name) => JsonDocument.Parse(File.ReadAllText(Path.Combine(fixtureRoot, name))).RootElement.Clone();
void Require(bool condition, string name) { if (!condition) throw new InvalidDataException($"Failed contract assertion: {name}"); }

string FindRepositoryRoot()
{
    var starts = new[] { Environment.CurrentDirectory, AppContext.BaseDirectory };
    foreach (var start in starts)
    {
        var directory = new DirectoryInfo(start);
        while (directory is not null)
        {
            var fixture = Path.Combine(directory.FullName, "docs", "live-integration", "bridge", "v1", "fixtures", "hello.json");
            if (File.Exists(fixture)) return directory.FullName;
            directory = directory.Parent;
        }
    }

    throw new DirectoryNotFoundException("Could not locate the repository bridge fixtures. Pass the repository root as the first argument.");
}

async Task RunWriteSliceTests()
{
    var source = "alias Sensor d0\n";
    var provider = new MemoryProvider(source, writable: true);
    var port = FreePort();
    using var service = new BridgeReadOnlyService(provider, new BridgeServiceOptions { Port = port, BearerToken = "test-token", MaxRequestBytes = 4096, MaxSourceBytes = 128 });
    service.Start();
    using var client = new HttpClient { BaseAddress = new Uri($"http://127.0.0.1:{port}/bridge/v1/") };
    client.DefaultRequestHeaders.Authorization = new System.Net.Http.Headers.AuthenticationHeaderValue("Bearer", "test-token");

    var write = new BridgeSourceWriteRequest("req-1", "epoch-1", "1", Hash(source), "move x 1", Hash("move x 1"));
    var applied = await client.PutAsJsonAsync("chips/123/source", write);
    Require(applied.StatusCode == HttpStatusCode.OK && (await applied.Content.ReadFromJsonAsync<BridgeSourceWriteResponse>())!.Applied, "write applied");
    var conflict = await client.PutAsJsonAsync("chips/123/source", write with { RequestId = "req-2" });
    Require(conflict.StatusCode == HttpStatusCode.Conflict, "source conflict");
    var stale = await client.PutAsJsonAsync("chips/123/source", write with { RequestId = "req-3", WorldEpoch = "epoch-old" });
    Require(stale.StatusCode == HttpStatusCode.Gone, "stale world");
    provider.Writable = false;
    var denied = await client.PutAsJsonAsync("chips/123/source", write with { RequestId = "req-4", ExpectedVersion = "2", ExpectedSha256 = Hash("move x 1") });
    Require(denied.StatusCode == HttpStatusCode.Forbidden, "write permission");
    provider.Writable = true;
    var oversized = await client.PutAsJsonAsync("chips/123/source", write with { RequestId = "req-5", Source = new string('a', 129), SourceSha256 = Hash(new string('a', 129)) });
    Require(oversized.StatusCode == HttpStatusCode.RequestEntityTooLarge, "oversized source");
}

static int FreePort()
{
    using var listener = new TcpListener(IPAddress.Loopback, 0);
    listener.Start();
    return ((IPEndPoint)listener.LocalEndpoint).Port;
}

static string Hash(string value) => Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();

sealed class MemoryProvider : IBridgeSourceMutationProvider
{
    private readonly object gate = new();
    private BridgeSource source;
    public bool Writable { get; set; }

    public MemoryProvider(string text, bool writable)
    {
        Writable = writable;
        source = new BridgeSource("epoch-1", "123", "housing-1", "ic10", "1", TestHash.Hash(text), text);
    }

    public BridgeHello GetHello() => new("1.0", "test", "test", "test", "singlePlayer", new(true, "epoch-1", "1"), new(true, true, Writable, false, false), new(128, 10, 8));
    public BridgeSnapshot GetSnapshot() => new("epoch-1", source.Version, Array.Empty<BridgeScope>(), new[] { new BridgeChip("123", "housing-1", "Housing", "housing", "chip", "ic10", true, new(true, Writable, source.Version, source.Sha256)) }, Array.Empty<BridgeWarning>());
    public BridgeSource? GetSource(string chipId, string worldEpoch) => chipId == "123" && worldEpoch == source.WorldEpoch ? source : null;

    public BridgeSourceWriteResult TryWriteSource(string chipId, BridgeSourceWriteRequest request, int maxSourceBytes)
    {
        lock (gate)
        {
            if (request.WorldEpoch != source.WorldEpoch) return new(BridgeSourceWriteStatus.StaleWorld, null, null);
            if (chipId != source.ChipId) return new(BridgeSourceWriteStatus.UnknownChip, null, null);
            if (!Writable) return new(BridgeSourceWriteStatus.Denied, null, null);
            if (Encoding.UTF8.GetByteCount(request.Source) > maxSourceBytes) return new(BridgeSourceWriteStatus.Oversized, null, null);
            if (request.ExpectedVersion != source.Version || request.ExpectedSha256 != source.Sha256) return new(BridgeSourceWriteStatus.Conflict, null, source);
            var next = new BridgeSource(source.WorldEpoch, source.ChipId, source.HousingReferenceId, source.Language, (int.Parse(source.Version) + 1).ToString(), request.SourceSha256, request.Source);
            source = next;
            return new(BridgeSourceWriteStatus.Applied, new(next.WorldEpoch, next.ChipId, next.Version, next.Sha256, true), null);
        }
    }
}

static class TestHash
{
    public static string Hash(string value) => Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();
}
