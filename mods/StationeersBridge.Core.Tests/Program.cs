using System.Text.Json;

var root = args.Length == 1 ? args[0] : Path.GetFullPath(Path.Combine(Environment.CurrentDirectory, "..", ".."));
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
Console.WriteLine("Bridge contract fixtures: 7 assertions passed.");

JsonElement Read(string name) => JsonDocument.Parse(File.ReadAllText(Path.Combine(fixtureRoot, name))).RootElement.Clone();
void Require(bool condition, string name) { if (!condition) throw new InvalidDataException($"Failed contract assertion: {name}"); }
