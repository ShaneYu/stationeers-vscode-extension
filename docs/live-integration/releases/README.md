# Live-integration release evidence

Each supported release gets one sanitized report in this directory, based on
`p310-release-report.template.md` and
`../evidence/p310-release-evidence.template.json`.

The report is durable evidence, not a checklist shortcut. It must identify the
exact build inputs, automated results, real-game sequences, package audit,
known deviations, and rollback result. Keep raw captures outside version
control until redaction is reviewed.

Never mark real-game acceptance complete when a sequence is `not-run`,
`runtime-constrained`, `mock-only`, or `observed-with-blocker`.
