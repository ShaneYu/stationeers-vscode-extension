# Third-party notices

## Stationeers

Stationeers, Stationpedia, and related names, text, images, and game content
are copyright and/or trademarks of RocketWerkz and its licensors.

This independent community project is not affiliated with, endorsed by, or
sponsored by RocketWerkz.

The extension contains generated IC10 reference data and selected device,
ingot, and ice thumbnail images derived from a locally installed copy of
Stationeers. These materials are included to identify IC10 instructions and
in-game objects in editor documentation. They are not covered by this
project's MIT License. All rights in those materials remain with their
respective owners.

Attribution in this notice does not itself grant permission to redistribute
third-party material. Maintainers are responsible for confirming that each
public release has an appropriate licence, permission, or applicable legal
basis for the third-party content it includes.

## Open-source dependencies

The extension and language server use open-source dependencies under their
respective licences. Dependency names and resolved versions are recorded in
the source repository's `Cargo.lock` and `package-lock.json`. Those licences
apply to the dependency code; the project's MIT License applies only to
material distributed under this project's control.

The local Lua test runner embeds PUC Lua 5.2 through the MIT-licensed `mlua`,
`mlua-sys`, and `lua-src` crates. Lua is Copyright © 1994–2015 Lua.org,
PUC-Rio, and is distributed under the MIT license. Resolved crate versions are
recorded in `Cargo.lock`.

## StationeersLua editor metadata snapshot

`packages/vscode/assets/lua/stationeers-v1/stationeerslua-0.2.3/` contains a
fallback copy of the Lua editor metadata from the
`orbitalfoundrymoddingcrew.stationeers-lua` extension version `0.2.3`.
StationeersLua is distributed under the MIT License. The snapshot is included
only for users who do not have that extension installed; when it is installed,
Stationeers Toolkit uses its current library directory instead.
