using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Newtonsoft.Json;
using StationeersBridge.RemoteNetwork.Core;

namespace StationeersBridge.RemoteNetwork;

internal sealed class BridgeHttpService : IDisposable
{
    private readonly HttpListener _listener = new();
    private readonly Func<object> _hello;
    private readonly Func<object> _scopes;
    private readonly Func<string, string, ChipSourceReadResult> _source;
    private readonly Func<string, ChipSourceWriteRequest, ChipSourceWriteResult> _writeSource;
    private readonly string _token;
    private volatile bool _stopping;
    private const int MaxRequestBytes = 131072;

    internal BridgeHttpService(int port, string token, Func<object> hello, Func<object> scopes, Func<string, string, ChipSourceReadResult> source, Func<string, ChipSourceWriteRequest, ChipSourceWriteResult>? writeSource = null)
    {
        if (port < 1 || port > 65535) throw new ArgumentOutOfRangeException(nameof(port));
        if (string.IsNullOrWhiteSpace(token)) throw new ArgumentException("A pairing token is required.", nameof(token));
        _token = token;
        _hello = hello;
        _scopes = scopes;
        _source = source;
        _writeSource = writeSource ?? ((_, _) => new ChipSourceWriteResult(ChipSourceWriteStatus.Denied));
        _listener.Prefixes.Add($"http://127.0.0.1:{port}/bridge/v1/");
        _listener.Prefixes.Add($"http://[::1]:{port}/bridge/v1/");
    }

    internal void Start()
    {
        _listener.Start();
        _ = Task.Run(AcceptLoop);
    }

    public void Dispose()
    {
        _stopping = true;
        if (_listener.IsListening) _listener.Stop();
        _listener.Close();
    }

    private void AcceptLoop()
    {
        while (!_stopping)
        {
            HttpListenerContext context;
            try { context = _listener.GetContext(); }
            catch when (_stopping) { break; }
            catch { continue; }
            _ = Task.Run(() => Handle(context));
        }
    }

    private void Handle(HttpListenerContext context)
    {
        try
        {
            if (!IsLoopback(context.Request.RemoteEndPoint))
            {
                Write(context, 401, new { error = new { code = "unauthorized", message = "The bridge accepts loopback requests only." } });
                return;
            }

            if (!string.Equals(context.Request.HttpMethod, "GET", StringComparison.Ordinal) && !string.Equals(context.Request.HttpMethod, "PUT", StringComparison.Ordinal))
            {
                Write(context, 405, new { error = new { code = "method_not_allowed", message = "The requested bridge method is not supported." } });
                return;
            }

            var path = context.Request.Url?.AbsolutePath.TrimEnd('/') ?? string.Empty;
            if (context.Request.ContentLength64 > MaxRequestBytes)
            { Write(context, 413, new { error = new { code = "payload_too_large", message = "The request exceeds the configured limit." } }); }
            else if (path == "/bridge/v1/pair" && string.Equals(context.Request.HttpMethod, "GET", StringComparison.Ordinal)) Write(context, 200, new { token = _token });
            else if (!string.Equals(context.Request.Headers["Authorization"], $"Bearer {_token}", StringComparison.Ordinal))
                Write(context, 401, new { error = new { code = "unauthorized", message = "A valid loopback pairing token is required." } });
            else if (path == "/bridge/v1/hello") Write(context, 200, _hello());
            else if (path == "/bridge/v1/scopes") Write(context, 200, _scopes());
            else if (path.StartsWith("/bridge/v1/chips/", StringComparison.Ordinal) && path.EndsWith("/source", StringComparison.Ordinal) && string.Equals(context.Request.HttpMethod, "GET", StringComparison.Ordinal)) HandleSource(context, path);
            else if (path.StartsWith("/bridge/v1/chips/", StringComparison.Ordinal) && path.EndsWith("/source", StringComparison.Ordinal) && string.Equals(context.Request.HttpMethod, "PUT", StringComparison.Ordinal)) HandleWriteSource(context, path);
            else Write(context, 404, new { error = new { code = "not_found", message = "The requested bridge route was not found." } });
        }
        catch
        {
            try { Write(context, 503, new { error = new { code = "bridge_unavailable", message = "The bridge could not complete the read." } }); }
            catch { }
        }
        finally
        {
            context.Response.Close();
        }
    }

    private void HandleSource(HttpListenerContext context, string path)
    {
        var chipId = path["/bridge/v1/chips/".Length..^"/source".Length];
        var worldEpoch = context.Request.QueryString["worldEpoch"];
        if (string.IsNullOrWhiteSpace(worldEpoch) || chipId.Length == 0 || chipId.Any(character => character is < '0' or > '9'))
        { Write(context, 400, new { error = new { code = "invalid_request", message = "chipId and worldEpoch are required." } }); return; }

        var result = _source(chipId, worldEpoch);
        switch (result.Status)
        {
            case ChipSourceReadStatus.StaleWorld:
                Write(context, 410, new { error = new { code = "stale_world", message = "The world changed; refresh discovery before retrying.", retryable = true } }); return;
            case ChipSourceReadStatus.UnknownChip:
                Write(context, 404, new { error = new { code = "unknown_chip", message = "The requested chip is not available." } }); return;
            case ChipSourceReadStatus.Lua:
                Write(context, 403, new { error = new { code = "capability_unsupported", message = "Lua source is not provided by this bridge." } }); return;
            case ChipSourceReadStatus.Unavailable:
                Write(context, 503, new { error = new { code = "bridge_unavailable", message = "The bridge could not complete the read." } }); return;
            case ChipSourceReadStatus.Success when result.Source is not null:
                var source = result.Source;
                Write(context, 200, new
                {
                    worldEpoch = source.WorldEpoch,
                    chipId = source.ChipReference,
                    housingReferenceId = source.HousingReference,
                    language = source.Language,
                    length = source.Length,
                    version = source.Version,
                    sha256 = source.Sha256,
                    source = source.Source,
                }); return;
            default:
                Write(context, 503, new { error = new { code = "bridge_unavailable", message = "The bridge could not complete the read." } }); return;
        }
    }

    private void HandleWriteSource(HttpListenerContext context, string path)
    {
        var chipId = path["/bridge/v1/chips/".Length..^"/source".Length];
        if (chipId.Length == 0 || chipId.Any(character => character is < '0' or > '9'))
        { Write(context, 400, new { error = new { code = "invalid_request", message = "chipId must contain decimal digits only." } }); return; }

        ChipSourceWriteRequest? request;
        try
        {
            using var reader = new StreamReader(context.Request.InputStream, Encoding.UTF8);
            request = JsonConvert.DeserializeObject<ChipSourceWriteRequest>(reader.ReadToEnd());
        }
        catch
        {
            Write(context, 400, new { error = new { code = "invalid_request", message = "The request body is not valid JSON." } }); return;
        }

        if (request is null || string.IsNullOrWhiteSpace(request.RequestId) || string.IsNullOrWhiteSpace(request.WorldEpoch) || string.IsNullOrWhiteSpace(request.ExpectedVersion) ||
            !IsSha256(request.ExpectedSha256) || request.Source is null || !IsSha256(request.SourceSha256) ||
            !string.Equals(request.SourceSha256, Sha256(request.Source), StringComparison.Ordinal) || !IsConservativeSource(request.Source))
        { Write(context, 422, new { error = new { code = "invalid_source", message = "The source request is invalid." } }); return; }

        var result = _writeSource(chipId, request);
        switch (result.Status)
        {
            case ChipSourceWriteStatus.Applied when result.Response is not null:
                Write(context, 200, new { worldEpoch = result.Response.WorldEpoch, chipId = result.Response.ChipReference, version = result.Response.Version, sha256 = result.Response.Sha256, applied = true }); return;
            case ChipSourceWriteStatus.StaleWorld:
                Write(context, 410, new { error = new { code = "stale_world", message = "The world changed; refresh discovery before retrying.", retryable = true } }); return;
            case ChipSourceWriteStatus.UnknownChip:
                Write(context, 404, new { error = new { code = "unknown_chip", message = "The requested chip is not available." } }); return;
            case ChipSourceWriteStatus.Lua:
            case ChipSourceWriteStatus.Denied:
                Write(context, 403, new { error = new { code = result.Status == ChipSourceWriteStatus.Lua ? "capability_unsupported" : "write_denied", message = "IC10 source writes are not available for this target." } }); return;
            case ChipSourceWriteStatus.Conflict:
                var current = result.Current;
                var details = new Dictionary<string, object?>();
                if (current is not null) { details["worldEpoch"] = current.WorldEpoch; details["chipId"] = current.ChipReference; details["version"] = current.Version; details["sha256"] = current.Sha256; }
                Write(context, 409, new { error = new { code = "source_conflict", message = "The source changed; refresh before retrying.", retryable = true, details } }); return;
            case ChipSourceWriteStatus.Oversized:
                Write(context, 413, new { error = new { code = "source_too_large", message = "Source exceeds the configured limit." } }); return;
            case ChipSourceWriteStatus.InvalidSource:
                Write(context, 422, new { error = new { code = "invalid_source", message = "Source contains unsupported control characters." } }); return;
            case ChipSourceWriteStatus.StaleTarget:
                Write(context, 410, new { error = new { code = "stale_target", message = "The chip was replaced or is no longer the selected target.", retryable = true } }); return;
            default:
                Write(context, 503, new { error = new { code = "bridge_unavailable", message = "The bridge could not complete the write.", retryable = true } }); return;
        }
    }

    private static void Write(HttpListenerContext context, int status, object value)
    {
        var bytes = Encoding.UTF8.GetBytes(JsonConvert.SerializeObject(value));
        context.Response.StatusCode = status;
        context.Response.ContentType = "application/json; charset=utf-8";
        context.Response.ContentLength64 = bytes.Length;
        context.Response.OutputStream.Write(bytes, 0, bytes.Length);
    }

    private static bool IsLoopback(IPEndPoint? endpoint) => endpoint?.Address is not null && IPAddress.IsLoopback(endpoint.Address);
    private static bool IsSha256(string? value) => value is not null && value.Length == 64 && value.All(character => character is >= '0' and <= '9' or >= 'a' and <= 'f');
    private static bool IsConservativeSource(string source) => source.All(character => character is '\r' or '\n' or '\t' || character >= ' ');
    private static string Sha256(string source) => ChipSourceWriteValidation.Hash(source);
}
