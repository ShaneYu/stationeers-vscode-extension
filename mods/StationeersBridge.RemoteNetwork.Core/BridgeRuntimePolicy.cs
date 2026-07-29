namespace StationeersBridge.RemoteNetwork.Core;

public enum BridgeRuntimeRole
{
    Unknown,
    SinglePlayer,
    Client,
    Host,
    DedicatedServer,
}

public static class BridgeRuntimePolicy
{
    public static BridgeRuntimeRole GetRole(bool isServer, bool isClient) =>
        isServer && isClient
            ? BridgeRuntimeRole.Host
            : isServer
                ? BridgeRuntimeRole.DedicatedServer
                : isClient
                    ? BridgeRuntimeRole.Client
                    : BridgeRuntimeRole.SinglePlayer;

    public static bool ShouldStartIdeBridge(bool isServer, bool isClient) =>
        GetRole(isServer, isClient) != BridgeRuntimeRole.DedicatedServer;

    public static string CapabilityState(bool isServer, bool isClient) =>
        ShouldStartIdeBridge(isServer, isClient)
            ? "loopback_bridge_allowed"
            : "dedicated_server_listener_suppressed";
}
