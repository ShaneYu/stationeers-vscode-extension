using System;
using System.IO;
using System.Net;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Newtonsoft.Json;

namespace StationeersBridge.RemoteNetwork;

internal sealed class BridgeHttpService : IDisposable
{
    private readonly HttpListener _listener = new();
    private readonly Func<object> _hello;
    private readonly Func<object> _scopes;
    private readonly string _token;
    private volatile bool _stopping;

    internal BridgeHttpService(int port, string token, Func<object> hello, Func<object> scopes)
    {
        if (port < 1 || port > 65535) throw new ArgumentOutOfRangeException(nameof(port));
        if (string.IsNullOrWhiteSpace(token)) throw new ArgumentException("A pairing token is required.", nameof(token));
        _token = token;
        _hello = hello;
        _scopes = scopes;
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

            if (!string.Equals(context.Request.HttpMethod, "GET", StringComparison.Ordinal))
            {
                Write(context, 405, new { error = new { code = "read_only", message = "The bridge currently exposes GET routes only." } });
                return;
            }

            var path = context.Request.Url?.AbsolutePath.TrimEnd('/') ?? string.Empty;
            if (path == "/bridge/v1/pair") Write(context, 200, new { token = _token });
            else if (!string.Equals(context.Request.Headers["Authorization"], $"Bearer {_token}", StringComparison.Ordinal))
                Write(context, 401, new { error = new { code = "unauthorized", message = "A valid loopback pairing token is required." } });
            else if (path == "/bridge/v1/hello") Write(context, 200, _hello());
            else if (path == "/bridge/v1/scopes") Write(context, 200, _scopes());
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

    private static void Write(HttpListenerContext context, int status, object value)
    {
        var bytes = Encoding.UTF8.GetBytes(JsonConvert.SerializeObject(value));
        context.Response.StatusCode = status;
        context.Response.ContentType = "application/json; charset=utf-8";
        context.Response.ContentLength64 = bytes.Length;
        context.Response.OutputStream.Write(bytes, 0, bytes.Length);
    }

    private static bool IsLoopback(IPEndPoint? endpoint) => endpoint?.Address is not null && IPAddress.IsLoopback(endpoint.Address);
}
