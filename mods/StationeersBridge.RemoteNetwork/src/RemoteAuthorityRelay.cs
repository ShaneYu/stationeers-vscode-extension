using System;
using System.Collections.Concurrent;
using System.Threading;
using Assets.Scripts;
using Assets.Scripts.Networking;
using LaunchPadBooster.Networking;
using StationeersBridge.RemoteNetwork.Core;

namespace StationeersBridge.RemoteNetwork;

internal static class RemoteAuthorityRelay
{
    private const int MaxPending = 64;
    private const int MaxFieldLength = 65536;
    private const int TimeoutMilliseconds = 10000;
    private static readonly ConcurrentQueue<PendingRequest> ClientQueue = new();
    private static readonly ConcurrentDictionary<string, PendingRequest> Pending = new(StringComparer.Ordinal);
    private static readonly ConcurrentQueue<ServerRequest> ServerQueue = new();
    private static Func<string, string, ChipSourceReadResult>? _read;
    private static Func<string, ChipSourceWriteRequest, ChipSourceWriteResult>? _write;
    private static Func<ulong, bool>? _authorizeWrite;

    internal static bool IsConfigured => _read is not null && _write is not null;

    internal static void Configure(
        Func<string, string, ChipSourceReadResult> read,
        Func<string, ChipSourceWriteRequest, ChipSourceWriteResult> write,
        Func<ulong, bool> authorizeWrite)
    {
        _read = read;
        _write = write;
        _authorizeWrite = authorizeWrite;
    }

    internal static void Tick()
    {
        if (NetworkManager.IsClient && !NetworkManager.IsServer)
            ProcessClientQueue();
        if (NetworkManager.IsServer)
            ProcessServerQueue();
    }

    internal static ChipSourceReadResult ReadFromHost(string chipId, string worldEpoch)
    {
        if (!NetworkManager.IsClient || NetworkManager.IsServer || !IsConfigured)
            return new(ChipSourceReadStatus.Unavailable);
        var pending = new PendingRequest(Guid.NewGuid().ToString("N"), 0, chipId, worldEpoch, null);
        return EnqueueAndWait(pending).Read ?? new(ChipSourceReadStatus.Unavailable);
    }

    internal static ChipSourceWriteResult WriteToHost(string chipId, ChipSourceWriteRequest request)
    {
        if (!NetworkManager.IsClient || NetworkManager.IsServer || !IsConfigured)
            return new(ChipSourceWriteStatus.Unavailable);
        var pending = new PendingRequest(Guid.NewGuid().ToString("N"), 1, chipId, request.WorldEpoch, request);
        return EnqueueAndWait(pending).Write ?? new(ChipSourceWriteStatus.Unavailable);
    }

    internal static void ReceiveRequest(long connectionId, RemoteAuthorityRequestMessage request)
    {
        if (!NetworkManager.IsServer || string.IsNullOrWhiteSpace(request.RequestId) || request.RequestId.Length > 128 || request.ChipId.Length > MaxFieldLength)
            return;
        ServerQueue.Enqueue(new ServerRequest(connectionId, request));
    }

    internal static void ReceiveResponse(RemoteAuthorityResponseMessage response)
    {
        if (!NetworkManager.IsClient || string.IsNullOrWhiteSpace(response.RequestId)) return;
        if (Pending.TryRemove(response.RequestId, out var pending))
        {
            pending.Read = response.Read;
            pending.Write = response.Write;
            pending.Completed.Set();
        }
    }

    private static PendingRequest EnqueueAndWait(PendingRequest pending)
    {
        if (Pending.Count >= MaxPending || ClientQueue.Count >= MaxPending)
            return pending with { Read = new(ChipSourceReadStatus.Unavailable), Write = new(ChipSourceWriteStatus.Unavailable) };
        Pending[pending.RequestId] = pending;
        ClientQueue.Enqueue(pending);
        if (!pending.Completed.Wait(TimeoutMilliseconds))
            Pending.TryRemove(pending.RequestId, out _);
        return pending;
    }

    private static void ProcessClientQueue()
    {
        for (var count = 0; count < 8 && ClientQueue.TryDequeue(out var pending); count++)
        {
            if (!Pending.ContainsKey(pending.RequestId)) continue;
            ModNetworkingExtensions.SendToHost(new RemoteAuthorityRequestMessage
            {
                RequestId = pending.RequestId,
                Operation = pending.Operation,
                ChipId = pending.ChipId,
                WorldEpoch = pending.WorldEpoch,
                WriteRequest = pending.WriteRequest,
                SenderClientId = NetworkManager.LocalClientId,
            });
        }
    }

    private static void ProcessServerQueue()
    {
        for (var count = 0; count < 8 && ServerQueue.TryDequeue(out var pending); count++)
        {
            var client = Client.Find(pending.ConnectionId);
            if (client is null) continue;
            var response = new RemoteAuthorityResponseMessage { RequestId = pending.Message.RequestId };
            if (pending.Message.Operation == 0)
            {
                response.Read = _read!(pending.Message.ChipId, pending.Message.WorldEpoch);
            }
            else if (pending.Message.Operation == 1 && pending.Message.WriteRequest is not null && _authorizeWrite!(client.ClientId))
            {
                response.Write = _write!(pending.Message.ChipId, pending.Message.WriteRequest);
            }
            else
            {
                response.Write = new(ChipSourceWriteStatus.Denied);
            }

            ModNetworkingExtensions.SendToClient(response, client);
        }
    }

    private sealed record PendingRequest(
        string RequestId,
        byte Operation,
        string ChipId,
        string WorldEpoch,
        ChipSourceWriteRequest? WriteRequest)
    {
        internal readonly ManualResetEventSlim Completed = new(false);
        internal ChipSourceReadResult? Read;
        internal ChipSourceWriteResult? Write;
    }

    private sealed record ServerRequest(long ConnectionId, RemoteAuthorityRequestMessage Message);
}

internal sealed class RemoteAuthorityRequestMessage : INetworkMessage
{
    internal string RequestId = string.Empty;
    internal byte Operation;
    internal string ChipId = string.Empty;
    internal string WorldEpoch = string.Empty;
    internal ChipSourceWriteRequest? WriteRequest;
    internal ulong SenderClientId;

    public void Process(long clientId)
    {
        if (NetworkManager.IsServer)
            RemoteAuthorityRelay.ReceiveRequest(clientId, this);
    }

    public void Deserialize(RocketBinaryReader reader)
    {
        RequestId = ReadBounded(reader, 128);
        Operation = reader.ReadByte();
        ChipId = ReadBounded(reader, 128);
        WorldEpoch = ReadBounded(reader, 128);
        if (Operation == 1)
        {
            WriteRequest = new ChipSourceWriteRequest(
                ReadBounded(reader, 128), WorldEpoch, ReadBounded(reader, 128), ReadBounded(reader, 64),
                ReadBounded(reader, 65536), ReadBounded(reader, 64));
        }
        SenderClientId = reader.ReadUInt64();
    }

    public void Serialize(RocketBinaryWriter writer)
    {
        writer.WriteString(RequestId);
        writer.WriteByte(Operation);
        writer.WriteString(ChipId);
        writer.WriteString(WorldEpoch);
        if (Operation == 1 && WriteRequest is not null)
        {
            writer.WriteString(WriteRequest.RequestId);
            writer.WriteString(WriteRequest.ExpectedVersion);
            writer.WriteString(WriteRequest.ExpectedSha256);
            writer.WriteString(WriteRequest.Source);
            writer.WriteString(WriteRequest.SourceSha256);
        }
        writer.WriteUInt64(SenderClientId);
    }

    private static string ReadBounded(RocketBinaryReader reader, int maxLength)
    {
        var value = reader.ReadString();
        return value.Length <= maxLength ? value : throw new InvalidOperationException("Network field exceeds its bound.");
    }
}

internal sealed class RemoteAuthorityResponseMessage : INetworkMessage
{
    internal string RequestId = string.Empty;
    internal ChipSourceReadResult? Read;
    internal ChipSourceWriteResult? Write;

    public void Process(long clientId)
    {
        if (NetworkManager.IsClient)
            RemoteAuthorityRelay.ReceiveResponse(this);
    }

    public void Deserialize(RocketBinaryReader reader)
    {
        RequestId = reader.ReadString();
        var readStatus = (ChipSourceReadStatus)reader.ReadInt32();
        Read = new(readStatus, ReadSource(reader));
        var writeStatus = (ChipSourceWriteStatus)reader.ReadInt32();
        Write = new(writeStatus, ReadResponse(reader), ReadSource(reader));
    }

    public void Serialize(RocketBinaryWriter writer)
    {
        writer.WriteString(RequestId);
        writer.WriteInt32((int)(Read?.Status ?? ChipSourceReadStatus.Unavailable));
        WriteSource(writer, Read?.Source);
        writer.WriteInt32((int)(Write?.Status ?? ChipSourceWriteStatus.Unavailable));
        WriteResponse(writer, Write?.Response);
        WriteSource(writer, Write?.Current);
    }

    private static ChipSource? ReadSource(RocketBinaryReader reader)
    {
        if (!reader.ReadBoolean()) return null;
        return new ChipSource(reader.ReadString(), reader.ReadString(), reader.ReadString(), reader.ReadString(), reader.ReadInt32(), reader.ReadString(), reader.ReadString(), reader.ReadString());
    }

    private static void WriteSource(RocketBinaryWriter writer, ChipSource? source)
    {
        writer.WriteBoolean(source is not null);
        if (source is null) return;
        writer.WriteString(source.WorldEpoch); writer.WriteString(source.ChipReference); writer.WriteString(source.HousingReference); writer.WriteString(source.Language);
        writer.WriteInt32(source.Length); writer.WriteString(source.Version); writer.WriteString(source.Sha256); writer.WriteString(source.Source);
    }

    private static ChipSourceWriteResponse? ReadResponse(RocketBinaryReader reader)
    {
        if (!reader.ReadBoolean()) return null;
        return new ChipSourceWriteResponse(reader.ReadString(), reader.ReadString(), reader.ReadString(), reader.ReadString(), reader.ReadString(), reader.ReadInt32(), reader.ReadBoolean());
    }

    private static void WriteResponse(RocketBinaryWriter writer, ChipSourceWriteResponse? response)
    {
        writer.WriteBoolean(response is not null);
        if (response is null) return;
        writer.WriteString(response.WorldEpoch); writer.WriteString(response.ChipReference); writer.WriteString(response.HousingReference); writer.WriteString(response.Version); writer.WriteString(response.Sha256);
        writer.WriteInt32(response.Length); writer.WriteBoolean(response.Applied);
    }
}
