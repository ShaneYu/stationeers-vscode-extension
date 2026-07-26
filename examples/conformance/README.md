# Real-game conformance captures

Use these files only to record minimal observations from an unmodified
Stationeers build. Do not commit game assemblies, decompiled code, saves, or
unrelated Stationpedia data.

1. Copy one program into an IC housing in the named Stationeers build.
2. Run it once with the documented inputs.
3. Record only the resulting registers and IC error in a JSON file conforming
   to `capture.schema.json`.
4. Name the capture `<instruction>-<case>-<game-version>.json`.
5. Add its ID to the conformance manifest only after a reviewer can reproduce
   the observation.

The four July 2026 programs deliberately avoid asserting ambiguous results.
Captures are still required for rotation conversion/masking, reversed clamp
bounds, NaN, infinities, and signed-zero behaviour before those instructions
can move from `unverified`.
