# Plan: deployment release lifecycle reference rail

1. Read the artifact rail: 12 nodes (5 ordered phases, 2 waits, rollback, 4 terminal endings) and
   15 labeled edges. Node kinds: start, active, decision, waiting, success, failure, neutral.
2. Model the rail as versioned types in `crates/lean-ctx-protocol/src/release_lifecycle.rs`, next
   to the existing `rollout` contract:
   - `ReleasePhaseV1` (5, with `RELEASE_PHASE_ORDER`), `ReleaseWaitV1`, `ReleaseEndingV1`.
   - `ReleaseStateV1` = `Phase | Waiting | RollingBack | Ended`, so a wait is not an ending.
   - `ReleaseEventV1` (14, with `RELEASE_EVENTS`).
   - `ReleaseTransitionV1` + `RELEASE_TRANSITIONS` (15) as the single edge authority.
3. Behavior: `release_transition`, `release_pending_events` (waits name their resolution events),
   and `ReleaseLifecycleV1::advance`, which either moves one edge or returns
   `ReleaseLifecycleError::{Rejected,Ended}` without changing state.
4. Enforcement: `validate_release_transitions` rejects the rails that break a stated invariant
   (promotion without `ready`, approval without verification, cancellation without denial, a
   terminal ending that leaves, a duplicated event). `validate_release_lifecycle` additionally
   requires that every declared pending event has an edge — and vice versa — for all 12 states.
5. Tests: promotion order, approval hold and resume, denial ending, failed-checks rebuild loop,
   health pause and recovery, both rollback endings, terminal absorption, off-rail rejection,
   rejected rails, and canonical-rail consistency. No serde round-trips, no length assertions.
6. Verify: `cargo test -p lean-ctx-protocol --lib release_lifecycle`,
   `cargo clippy -p lean-ctx-protocol --all-targets --all-features -- -D warnings`,
   `cargo fmt -p lean-ctx-protocol -- --check`. Then probe twice: retarget `denied` to `ready`
   and no-op the shape checks; both must fail the tests.
7. Deliver: branch, PR against `main`. No contract-portal entry (behavior only, mirroring
   `circuit_breaker`).
