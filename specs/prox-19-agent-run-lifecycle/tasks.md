# Tasks: agent-run lifecycle reference rail

- [x] `RunPhaseV1` (5 ordered), `RunWaitV1` (2), `RunEndingV1` (2), `RunTerminalV1` (3), with doc comments.
- [x] `RunStateV1` (`Phase | Waiting | Failed | Ended`) and `RunEventV1` (12).
- [x] `RUN_TRANSITIONS` (12 edges) + `run_transition` lookup.
- [x] `run_pending_events`: every non-terminal state exposes the events that continue it.
- [x] `RunLifecycleV1::advance` — one edge or `Rejected`/`Ended`, state unchanged on error.
- [x] `validate_run_transitions` + `validate_run_lifecycle` shape enforcement.
- [x] Tests: order, approval hold and resume, denial ending, missing-input hold and resume,
      expiry, recovery loop, final-state absorption, off-rail rejection, invalid rails,
      canonical consistency.
- [x] Wire `pub mod agent_run_lifecycle;` in `lib.rs`.
- [x] `cargo test -p lean-ctx-protocol --lib agent_run_lifecycle` (10 passed),
      `cargo clippy -p lean-ctx-protocol --all-targets --all-features -- -D warnings`,
      `cargo fmt -p lean-ctx-protocol -- --check`.
- [x] Drift probes: `approved → reviewing`, no-op'd `require_continuable`, and `completed`
      non-terminal each fail the rail tests; file restored and suite green afterwards.
