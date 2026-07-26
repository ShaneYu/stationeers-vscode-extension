# Simulator compatibility

This report is generated from `data/generated/instructions.json` and
`data/conformance/manifest.json`. Do not edit it by hand.

- Bundled Stationeers data: `0.2.6403.27689`
- Generated instructions: 154
- Supported with golden fixtures: 11
- Partial: 137
- Unverified: 4
- Unsupported: 2
- Known unsupported instructions: `lr`, `rmap`

## Device behaviour boundary

The CPU and passive world model are deterministic. Device fields, slots,
memory, and cable channels are modelled as passive state. Active machine
physics remains outside the simulator unless explicitly listed in the matrix.

Dependencies currently called out by the matrix: cable channels, passive logic fields, reagent mapping, reagent quantities.

## Evidence and deviations

The machine-readable detail, including syntax, operand types, fixture IDs, and
known deviations, is in `data/generated/conformance.json`. `rol`, `ror`,
`clamp`, and `sgn` remain unverified where generated Stationpedia descriptions
do not define edge behaviour; see `examples/conformance/README.md` for the
minimal real-game capture workflow.
