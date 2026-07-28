using System;
using System.Diagnostics;
using Assets.Scripts.Networking;
using Cysharp.Threading.Tasks;
using LaunchPadBooster.Networking;

namespace StationeersBridge.Feasibility;

public sealed class AuthorityProbeRpc : INetworkRPC
{
    private long _requestTimestamp;
    private long _responseTimestamp;
    private long _callerId;
    private int _handlerThread;
    private bool _handlerWasServer;

    public AuthorityProbeRpc()
    {
    }

    public void SerializeCall(RocketBinaryWriter writer)
    {
        writer.WriteInt64(_requestTimestamp);
    }

    public void DeserializeCall(RocketBinaryReader reader)
    {
        _requestTimestamp = reader.ReadInt64();
    }

    public UniTask ProcessCall(long clientId)
    {
        _callerId = clientId;
        _handlerThread = Environment.CurrentManagedThreadId;
        _handlerWasServer = NetworkManager.IsServer;
        _responseTimestamp = Stopwatch.GetTimestamp();
        ProbeLog.Write(
            "authority-rpc-handler",
            _handlerWasServer ? "authoritative" : "non-authoritative",
            new
            {
                callerId = clientId.ToString(),
                handlerThread = _handlerThread,
                isClient = NetworkManager.IsClient,
                isServer = NetworkManager.IsServer,
                payloadBytes = sizeof(long),
            });
        return default;
    }

    public void SerializeResult(RocketBinaryWriter writer)
    {
        writer.WriteInt64(_responseTimestamp);
        writer.WriteInt64(_callerId);
        writer.WriteInt32(_handlerThread);
        writer.WriteBoolean(_handlerWasServer);
    }

    public void DeserializeResult(RocketBinaryReader reader)
    {
        _responseTimestamp = reader.ReadInt64();
        _callerId = reader.ReadInt64();
        _handlerThread = reader.ReadInt32();
        _handlerWasServer = reader.ReadBoolean();
    }

    public static async void RunClientProbe()
    {
        if (!NetworkManager.IsClient)
        {
            ProbeLog.Write(
                "authority-rpc",
                "not-applicable",
                new
                {
                    reason = "local process is not a network client",
                    isServer = NetworkManager.IsServer,
                });
            return;
        }

        var rpc = new AuthorityProbeRpc
        {
            _requestTimestamp = Stopwatch.GetTimestamp(),
        };
        try
        {
            await rpc.CallHost();
            var finished = Stopwatch.GetTimestamp();
            var elapsedTicks = finished - rpc._requestTimestamp;
            ProbeLog.Write(
                "authority-rpc",
                rpc._handlerWasServer ? "observed" : "failed",
                new
                {
                    callerId = rpc._callerId.ToString(),
                    handlerThread = rpc._handlerThread,
                    handlerWasServer = rpc._handlerWasServer,
                    requestPayloadBytes = sizeof(long),
                    responsePayloadBytes =
                        sizeof(long) + sizeof(long) + sizeof(int) + sizeof(bool),
                    roundTripStopwatchTicks = elapsedTicks,
                    roundTripMilliseconds =
                        elapsedTicks * 1000d / Stopwatch.Frequency,
                });
        }
        catch (Exception exception)
        {
            ProbeLog.Failure("authority-rpc", exception);
        }
    }
}
