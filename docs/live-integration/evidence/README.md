# P3.02 evidence collection

Committed evidence is sanitized and machine-readable. Raw `[P3.02]` JSON log
records belong in the ignored `evidence/local/` directory until reviewed.

## Required fixture world

Use names beginning with `P302-`:

- one named and one unnamed `RemoteNetworkProbe`;
- one probe with each port connected to a different network;
- two probes on one physical network with the same label;
- two probes on one physical network with different labels;
- matching labels on separate physical networks;
- bridged networks and a cable that can be disconnected/reconnected;
- an IC10 housing named `P302-IC10`, containing valid source;
- a Lua housing named `P302-LUA`;
- duplicate housing labels and one unnamed housing.

## Collection sequence

1. Build and install the development mod using its README.
2. Enable `Development.Enabled` and `RegisterProbePrefab`.
3. Start a single-player world, print and place the probe kit, label fixtures,
   connect both ports, and capture `[P3.02]` records.
4. Save, exit to menu, reload, and capture the same records.
5. Disconnect/reconnect and bridge/unbridge the test networks between captures.
6. Enable `AllowSourceMutation` only after the exact `P302-IC10` housing
   contains disposable IC10 source, then set
   `SourceMutationConfirmation = MUTATE_AND_RESTORE_P302_SOURCE`. Confirm
   mutation and restoration in the log and editor.
7. Repeat in a hosted session with a remote client.
8. Enable `RunRpcOnWorldLoad` on the remote client and capture both handler and
   caller records.
9. Repeat server-side probes on a dedicated server.
10. Disable StationeersLua and repeat chip classification.

Record wall-clock date, topology name, process role, relevant counts, and the
exact assembly fingerprints in `installed-metadata-2026-07-28.json`.

## Sanitization

Remove local paths, save names, player names, Steam IDs, addresses, tokens, and
unrelated mod output. Preserve timestamps, versions, type/member names, opaque
fixture reference IDs, counts, durations, payload sizes, and failure messages.
