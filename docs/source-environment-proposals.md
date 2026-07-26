# Source-driven environment proposals

The language server exposes the custom request `ic10/proposeEnvironment` for an
open IC10 document:

```json
{ "uri": "file:///workspace/controller.ic10" }
```

The response is a versioned, read-only preview contract. It always contains
`schemaVersion: 1`, `previewOnly: true`, and the exact source URI. Requesting a
proposal never creates or overwrites a simulation environment.

The semantic scanner uses the parsed IC10 document, instruction operand
metadata, resolved aliases and defines, and bundled Stationpedia device access
metadata. It proposes:

- an IC housing and program reference;
- `d0`–`d5` devices and their aliases;
- ranked prefab candidates with confidence and reasons;
- required readable/writable fields, slot fields, and device memory;
- batch prefab/name groups;
- data-network participants and `db` channels;
- explicit unresolved assumptions for dynamic or incompatible references.

Every source observation carries a UTF-16 line/column range suitable for LSP
and VS Code navigation. File, remote, and virtual URIs are preserved rather
than converted through local filesystem paths.

The extension-side `EnvironmentProposalService` validates that the response
belongs to the requested document and converts it to an
`EnvironmentProposalPreview`. A preview has default candidate selections,
blockers, and `canApply`; the environment editor remains responsible for
showing the preview and requiring an explicit guarded apply action.
