# P3.03 RemoteNetwork game checklist

Run this checklist on the exact supported stack in the feasibility report and
retain sanitized logs under `docs/live-integration/evidence/`.

- [ ] Print one `ItemKitRemoteNetwork`, place it, and confirm the Stationpedia
      name is `Remote Network`.
- [ ] Confirm the `Network` faceplate and both data ports are usable.
- [ ] Label and clear the device with the ordinary labeller interaction.
- [ ] Confirm the device remains present and its label survives save/menu/reload.
- [ ] Confirm zero passive power draw and no global `StructureLogicMemory` hash
      or recipe change.
- [ ] Connect neither, one, and both ports; verify only connected ports attach.
- [ ] Same physical network + same label: one scope, `anchorCount > 1`.
- [ ] Same physical network + different labels: separate scopes with shared chips.
- [ ] Same label + different physical networks: separate scopes.
- [ ] One anchor with both ports on one network: one scope and two attachments,
      with chips deduplicated.
- [ ] One anchor with ports on different networks: two scopes for one label.
- [ ] Empty label: warning identifies the anchor and no deployable scope.
- [ ] Deconstruct/rebuild, cable disconnect/reconnect, and world switch: index
      refreshes at the safe lifecycle boundary.

P3.02 did not yet provide runtime evidence for duplicate/bridged/reconnect
fixtures or multiplayer. Those cases are intentionally checklist items, not
production claims.
