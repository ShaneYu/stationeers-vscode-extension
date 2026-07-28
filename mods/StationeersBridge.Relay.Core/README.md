# StationeersBridge.Relay.Core

This is the minimal P3.07 authority boundary and contract layer. It does not
implement Stationeers game RPC, player identity extraction, or a network
listener. `UnsupportedRelayTransport` reports `server_companion_required` and
fails closed until a verified authoritative transport exists.

The authority service accepts identity only from `AuthenticatedPlayer`, checks
authentication, authoritative-process binding, session binding, expiry,
revocation, kill switch, and current policy after queue delay. It bounds the
global and per-player queues, caches idempotent results, and audits every IC10
mutation without recording request payload/source text. `SinglePlayerRelay`
uses the same authority service through an internal short circuit.
