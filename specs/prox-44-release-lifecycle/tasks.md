# Tasks: deployment release lifecycle reference rail

- [x] `ReleasePhaseV1` (5 ordered), `ReleaseWaitV1` (2), `ReleaseEndingV1` (4), with doc comments.
- [x] `ReleaseStateV1` (`Phase | Waiting | RollingBack | Ended`) and `ReleaseEventV1` (14).
- [x] `RELEASE_TRANSITIONS` (15 edges) + `release_transition` lookup.
- [x] `release_pending_events`: every non-terminal state exposes the events that continue it.
- [x] `ReleaseLifecycleV1::advance` — one edge or `Rejected`/`Ended`, state unchanged on error.
- [x] `validate_release_transitions` + `validate_release_lifecycle` shape enforcement.
- [x] Tests: order, approval, denial, rebuild loop, health pause, both rollback endings,
      terminal absorption, off-rail rejection, invalid rails, canonical consistency.
- [x] Wire `pub mod release_lifecycle;` in `lib.rs`.
- [x] `cargo test -p lean-ctx-protocol --lib release_lifecycle` (10 passed),
      `cargo clippy -p lean-ctx-protocol --all-targets --all-features -- -D warnings`,
      `cargo fmt -p lean-ctx-protocol -- --check`.
- [x] Drift probes: `denied → ready` and no-op'd shape checks each fail the rail tests.
