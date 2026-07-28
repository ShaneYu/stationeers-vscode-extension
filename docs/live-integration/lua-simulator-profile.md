# Local Lua simulator profile

The local simulator currently exposes a fail-closed boundary for Lua. The
declared target is Lua 5.2 with StationeersLua `0.9.5.0`, recorded as profile
`stationeerslua-0.9.5.0-lua5.2`. No Lua source is executed by `ic10-sim` yet.

This is intentional: ordinary desktop Lua is not a substitute for the
StationeersLua VM. Until a runtime with verified Lua 5.2 semantics, compatible
licensing and native packaging, and a sandbox suitable for supported targets
is selected, Lua execution returns the structured diagnostic
`lua-runtime-unavailable`. The diagnostic includes the program ID and source
path, and is surfaced as a simulator error for Lua-only scenarios or a
compatibility warning when an IC10 program can still run in a mixed scenario.

The boundary does not read or parse Lua source. It therefore cannot provide
partial or guessed StationeersLua behavior. Filesystem, process, dynamic
library, network, and host API access are all unsupported.

The machine-readable profile is checked in at
[`lua-simulator-profile.json`](lua-simulator-profile.json). Its capability
entries are evidence placeholders and remain `unsupported` until each API is
backed by documentation, a fixture, and (where practical) sanitized game
probe evidence. This profile is separate from StationeersLua remote debugging
provided by P3.08.
