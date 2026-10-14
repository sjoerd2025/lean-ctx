# Spec: deployment release lifecycle reference rail (refs PROX-44)

## Problem / Why
The promotion rail a release object travels — queued → building → verifying → ready →
live — exists only as documentation prose and as scattered gates (`deployment-rehearsal-v1`,
`release-integrity-v1`, `test-deployment-evidence-v1`, `src/core/updater.rs` receipts).
Nothing in the Engine states the rail as reviewable behavior: which event moves a release,
which holds pause it without ending it, which events end it, and which moves are illegal.
Approval and verification are consequently easy to conflate, and a health regression has no
state to sit in.

## Goal
`lean-ctx-protocol` publishes the frozen reference rail: five ordered phases, two wait states
that hold a release without ending it and without consuming a worker, and explicit terminal
endings. Every wait names the events that continue it, and the rail is self-validating.

## Acceptance Criteria (EARS)
- The contract SHALL define exactly five ordered promotion phases: `queued`, `building`,
  `verifying`, `ready`, `live`.
- WHEN a release is `queued` and `pick up` arrives, THE RAIL SHALL move it to `building`.
- WHEN `image built` arrives in `building`, THE RAIL SHALL move the release to `verifying`.
- WHEN `checks pass` arrives in `verifying`, THE RAIL SHALL move the release to `ready`.
- WHEN `tests fail` arrives in `verifying`, THE RAIL SHALL return the release to `building`.
- WHEN `promote` arrives in `ready`, THE RAIL SHALL move the release to `live`.
- THE CONTRACT SHALL keep verification and approval distinct states: `needs sign-off` is the
  only event that reaches `needs approval`, and it SHALL be accepted only in `verifying`.
- WHEN `approved` arrives in `needs approval`, THE RAIL SHALL resume at `ready`, never at `live`.
- WHEN `denied` arrives in `needs approval`, THE RAIL SHALL end the release as `cancelled`.
- WHEN a release is `live` and `SLO alert` arrives, THE RAIL SHALL hold it in `health paused`
  and SHALL NOT end it.
- WHEN `recovered` arrives in `health paused`, THE RAIL SHALL resume the release at `live`.
- WHEN `breach persists` arrives in `health paused`, THE RAIL SHALL enter `rolling back`.
- WHEN `restored` arrives in `rolling back`, THE RAIL SHALL end the release as `rolled back`.
- WHEN `rollback error` arrives in `rolling back`, THE RAIL SHALL end the release as `failed`.
- WHEN `abort` arrives in `building` or `verifying`, THE RAIL SHALL end the release as `aborted`;
  `abort` SHALL be rejected from `ready` and `live`.
- THE CONTRACT SHALL expose, for every non-terminal state, the pending events that continue it,
  so a held release never needs a worker to discover how to proceed.
- WHEN an event is not on the rail for the current state, THE RELEASE SHALL remain unchanged and
  the caller SHALL receive a rejection error.
- WHEN a release already ended, EVERY event SHALL fail with the ended error and SHALL NOT move it.
- THE RAIL SHALL reject a transition table that promotes without `ready`, approves without
  verification, ends as `cancelled` without denial, leaves a terminal ending, or declares one
  event twice in one state.

## Out of Scope
- Deploying anything, probing production, or taking a promotion decision. The module models the
  rail; production rollout intelligence stays Class D in `lean-ctx-enterprise`.
- A wire schema or contract-portal document. Like `circuit_breaker`, this reference carries
  behavior only; there is no payload schema to version in `docs/contracts/`.
- Persistence, timers, and worker scheduling for a held release. `health paused` observes the
  release; it does not run a rollout controller.

## Verification
- `cargo test -p lean-ctx-protocol --lib release_lifecycle` (10 rail tests).
- `cargo clippy -p lean-ctx-protocol --all-targets --all-features -- -D warnings`
- `cargo fmt -p lean-ctx-protocol -- --check`
- Drift probes: flipping `denied` to resume at `ready`, and no-op'ing the shape checks, each make
  the rail tests fail.

## Links
- Tracking issue: PROX-44 (Multica); artifact: <https://archify.si/gallery/artifacts/deployment-release.lifecycle>
- Plan: ./plan.md · Tasks: ./tasks.md
