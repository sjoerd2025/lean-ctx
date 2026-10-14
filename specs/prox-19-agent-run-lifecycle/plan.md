# Plan: agent-run lifecycle reference rail

1. Read the artifact rail: 10 nodes (5 ordered phases, 2 holds, the recovery loop, 2 terminal
   exits) and 12 labeled edges. Node kinds: start, active, decision, waiting, success, failure,
   final state.
2. Model the rail as versioned types in `crates/lean-ctx-protocol/src/agent_run_lifecycle.rs`,
   next to the existing `circuit_breaker` and `release_lifecycle` reference contracts:
   - `RunPhaseV1` (5, with `RUN_PHASE_ORDER`), `RunWaitV1`, `RunEndingV1`, `RunTerminalV1`
     (3, with `RUN_TERMINALS`) so a caller can name how a run finished.
   - `RunStateV1` = `Phase | Waiting | Failed | Ended`, so a hold is not an ending and the
     recovery loop is not terminal.
   - `RunEventV1` (12, with `RUN_EVENTS`).
   - `RunTransitionV1` + `RUN_TRANSITIONS` (12) as the single edge authority.
3. Behavior: `run_transition`, `run_pending_events` (holds name their resolution events), and
   `RunLifecycleV1::advance`, which either moves one edge or returns
   `RunLifecycleError::{Rejected,Ended}` without changing state. `terminal()` covers the fifth
   phase as well as both endings, so "finished" is one concept.
4. Enforcement: `validate_run_transitions` rejects the rails that break a stated invariant (a
   hold or recovery loop with no continuation, approval outside `executing`, a missing input
   outside `reviewing`, completion without `reviewing`, expiry without a hold, a final state
   that leaves the rail, a duplicated event). `validate_run_lifecycle` additionally requires
   that every declared pending event has an edge — and vice versa — for all 10 states.
5. Tests: promotion order, approval hold and resume, denial ending, missing-input hold and
   resume, expiry, the recovery loop, final-state absorption, off-rail rejection, rejected
   rails (each asserting the named invariant, not merely a rejection), and canonical-rail
   consistency. No serde round-trips, no length assertions.
6. Verify: `cargo test -p lean-ctx-protocol --lib agent_run_lifecycle`,
   `cargo clippy -p lean-ctx-protocol --all-targets --all-features -- -D warnings`,
   `cargo fmt -p lean-ctx-protocol -- --check`. Then probe three times: retarget `approved` to
   `reviewing`, no-op `require_continuable`, make `completed` non-terminal; each must fail the
   tests.
7. Deliver: branch, PR against `main`. No contract-portal entry (behavior only, mirroring
   `circuit_breaker`).
