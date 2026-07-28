using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;

namespace StationeersBridge.Core;

public interface IBridgeSnapshotProvider
{
    BridgeHello GetHello();
    BridgeSnapshot GetSnapshot();
    BridgeSource? GetSource(string chipId, string worldEpoch);
}

public sealed class BridgeServiceOptions
{
    public int Port { get; init; } = 3032;
    public required string BearerToken { get; init; }
    public int MaxRequestBytes { get; init; } = 8192;
    public int MaxSourceBytes { get; init; } = 65536;
    public int MaxConnections { get; init; } = 8;
    public int MaxRequestsPerSecond { get; init; } = 10;
    public ISet<string> AllowedOrigins { get; init; } = new HashSet<string>(StringComparer.Ordinal);
}

public sealed class BridgeReadOnlyService : IDisposable
{
    private readonly HttpListener _listener = new();
    private readonly IBridgeSnapshotProvider _provider;
    private readonly BridgeServiceOptions _options;
    private readonly SemaphoreSlim _workSlots;
    private readonly ConcurrentDictionary<string, int> _requests = new();
    private int _connections;
    private volatile bool _stopping;

    public BridgeReadOnlyService(IBridgeSnapshotProvider provider, BridgeServiceOptions options)
    {
        _provider = provider;
        _options = options;
        ValidateOptions(options);
        _workSlots = new SemaphoreSlim(options.MaxConnections, options.MaxConnections);
        _listener.Prefixes.Add($"http://127.0.0.1:{options.Port}/bridge/v1/");
        _listener.Prefixes.Add($"http://[::1]:{options.Port}/bridge/v1/");
    }

    public void Start()
    {
        _listener.Start();
        _ = AcceptLoopAsync();
    }

    public void Dispose()
    {
        _stopping = true;
        if (_listener.IsListening) _listener.Stop();
        _listener.Close();
        _workSlots.Dispose();
    }

    private async Task AcceptLoopAsync()
    {
        while (!_stopping)
        {
            HttpListenerContext context;
            try { context = await _listener.GetContextAsync().ConfigureAwait(false); }
            catch when (_stopping) { break; }
            catch { continue; }
            _ = Task.Run(() => HandleAsync(context));
        }
    }

    private async Task HandleAsync(HttpListenerContext context)
    {
        var workSlotAcquired = false;
        if (Interlocked.Increment(ref _connections) > _options.MaxConnections)
        {
            Interlocked.Decrement(ref _connections);
            await WriteError(context, 429, "connection_limit", "Too many bridge connections.", true).ConfigureAwait(false);
            return;
        }
        try
        {
            if (!IsLoopback(context.Request.RemoteEndPoint) || !Authorized(context.Request))
            { await WriteError(context, 401, "unauthorized", "A valid loopback pairing token is required.", false).ConfigureAwait(false); return; }
            var origin = context.Request.Headers["Origin"];
            if (origin is not null && !_options.AllowedOrigins.Contains(origin))
            { await WriteError(context, 403, "origin_denied", "Browser origin is not allowed.", false).ConfigureAwait(false); return; }
            if (!WithinRateLimit(context.Request.RemoteEndPoint?.Address.ToString() ?? "unknown"))
            { await WriteError(context, 429, "rate_limited", "Bridge request rate limit reached.", true).ConfigureAwait(false); return; }
            if (context.Request.ContentLength64 > _options.MaxRequestBytes)
            { await WriteError(context, 413, "payload_too_large", "Request exceeds the configured payload limit.", false).ConfigureAwait(false); return; }
            workSlotAcquired = await _workSlots.WaitAsync(0).ConfigureAwait(false);
            if (!workSlotAcquired)
            { await WriteError(context, 429, "queue_saturated", "The bounded bridge work queue is full.", true).ConfigureAwait(false); return; }
            var path = context.Request.Url?.AbsolutePath.TrimEnd('/') ?? string.Empty;
            if (context.Request.HttpMethod != "GET")
            { await WriteError(context, 405, "read_only", "The bridge currently exposes GET routes only.", false).ConfigureAwait(false); return; }
            if (path == "/bridge/v1/hello") await WriteJson(context, 200, _provider.GetHello()).ConfigureAwait(false);
            else if (path == "/bridge/v1/scopes") await WriteJson(context, 200, _provider.GetSnapshot()).ConfigureAwait(false);
            else if (path.StartsWith("/bridge/v1/chips/", StringComparison.Ordinal) && path.EndsWith("/source", StringComparison.Ordinal)) await HandleSource(context, path).ConfigureAwait(false);
            else await WriteError(context, 404, "not_found", "The requested bridge route was not found.", false).ConfigureAwait(false);
        }
        catch { await WriteError(context, 503, "bridge_unavailable", "The bridge could not complete the read.", true).ConfigureAwait(false); }
        finally { if (workSlotAcquired) _workSlots.Release(); Interlocked.Decrement(ref _connections); context.Response.Close(); }
    }

    private async Task HandleSource(HttpListenerContext context, string path)
    {
        var chipId = path["/bridge/v1/chips/".Length..^"/source".Length];
        var epoch = context.Request.QueryString["worldEpoch"];
        if (string.IsNullOrWhiteSpace(epoch) || chipId.Length == 0 || chipId.Any(c => c < '0' || c > '9'))
        { await WriteError(context, 400, "invalid_request", "chipId and worldEpoch are required.", false).ConfigureAwait(false); return; }
        var snapshot = _provider.GetSnapshot();
        if (!string.Equals(snapshot.WorldEpoch, epoch, StringComparison.Ordinal))
        { await WriteError(context, 410, "stale_world", "The world changed; refresh discovery before retrying.", true).ConfigureAwait(false); return; }
        var source = _provider.GetSource(chipId, epoch);
        if (source is null) { await WriteError(context, 404, "unknown_chip", "The requested chip is not available.", false).ConfigureAwait(false); return; }
        if (source.Language != "ic10") { await WriteError(context, 403, "capability_unsupported", "Lua source is not provided by this bridge.", false).ConfigureAwait(false); return; }
        if (Encoding.UTF8.GetByteCount(source.Source) > _options.MaxSourceBytes) { await WriteError(context, 413, "source_too_large", "Source exceeds the configured limit.", false).ConfigureAwait(false); return; }
        await WriteJson(context, 200, source).ConfigureAwait(false);
    }

    private bool Authorized(HttpListenerRequest request) => string.Equals(request.Headers["Authorization"], $"Bearer {_options.BearerToken}", StringComparison.Ordinal);
    private static bool IsLoopback(IPEndPoint? endpoint) => endpoint?.Address is not null && IPAddress.IsLoopback(endpoint.Address);
    private bool WithinRateLimit(string key) { var now = DateTimeOffset.UtcNow.ToUnixTimeSeconds().ToString(); var value = _requests.AddOrUpdate($"{key}:{now}", 1, (_, old) => old + 1); return value <= _options.MaxRequestsPerSecond; }
    private static void ValidateOptions(BridgeServiceOptions options) { if (options.Port is < 1 or > 65535) throw new ArgumentOutOfRangeException(nameof(options.Port)); if (string.IsNullOrWhiteSpace(options.BearerToken)) throw new ArgumentException("A pairing token is required.", nameof(options)); if (options.BearerToken.Contains('\n') || options.BearerToken.Contains('\r')) throw new ArgumentException("Token contains a forbidden character.", nameof(options)); }
    private static async Task WriteJson(HttpListenerContext context, int status, object value) { context.Response.StatusCode = status; context.Response.ContentType = "application/json; charset=utf-8"; await JsonSerializer.SerializeAsync(context.Response.OutputStream, value, new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase }).ConfigureAwait(false); }
    private static Task WriteError(HttpListenerContext context, int status, string code, string message, bool retryable) => WriteJson(context, status, new BridgeErrorEnvelope(new BridgeError(code, message, context.Request.Headers["X-Request-Id"] ?? "server-generated", retryable, new Dictionary<string, object?>())));
}
