namespace StationeersToolkit.Core;

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
        GetRole(isServer, isClient, isBatchMode: false);

    public static BridgeRuntimeRole GetRole(bool isServer, bool isClient, bool isBatchMode) =>
        isBatchMode
            ? BridgeRuntimeRole.DedicatedServer
            : isServer && isClient
            ? BridgeRuntimeRole.Host
            : isServer
                ? BridgeRuntimeRole.DedicatedServer
                : isClient
                    ? BridgeRuntimeRole.Client
                    : BridgeRuntimeRole.SinglePlayer;

    public static bool ShouldStartIdeBridge(bool isServer, bool isClient) =>
        ShouldStartIdeBridge(isServer, isClient, isBatchMode: false);

    public static bool ShouldStartIdeBridge(bool isServer, bool isClient, bool isBatchMode) =>
        GetRole(isServer, isClient, isBatchMode) != BridgeRuntimeRole.DedicatedServer;

    public static string CapabilityState(bool isServer, bool isClient) =>
        CapabilityState(isServer, isClient, isBatchMode: false);

    public static string CapabilityState(bool isServer, bool isClient, bool isBatchMode) =>
        ShouldStartIdeBridge(isServer, isClient, isBatchMode)
            ? "loopback_bridge_allowed"
            : "dedicated_server_listener_suppressed";
}
