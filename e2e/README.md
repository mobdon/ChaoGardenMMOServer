# End-to-End and Load Testing Suite

Release-focused tests for the Chao Garden MMO server, mobile client, and Tiny
Garden phone widget. Nothing in this directory touches production services:
integration tests run against throwaway PostgreSQL/Redis instances, load runs
target `TEST_BASE_URL` (staging), and Maestro flows run on local simulators
against a locally started server.

## Suites

| Suite | Entry point | Duration | Runs in CI? |
| --- | --- | --- | --- |
| Smoke | `scripts/smoke-test.sh staging` | < 1 min | yes (deploy gate) |
| Integration + contract | `e2e/scripts/run-integration.sh` | ~2 min | yes |
| Maestro full | `e2e/scripts/run-e2e.sh` | ~15 min | nightly / manual |
| Load (k6) | `e2e/scripts/run-load.sh` | ~20 min | nightly / manual |

All machine-readable reports land in `e2e/reports/` and are uploaded as CI
artifacts (`integration`, `k6`, `maestro` prefixes).

## Local usage

```sh
# 1. Integration + contract tests (isolated Postgres/Redis via Docker)
e2e/scripts/run-integration.sh

# 2. Deterministic fixtures only (inspect what the tests seed)
node e2e/fixtures/seed.js --print

# 3. Maestro mobile journeys (requires a booted iOS simulator or Android
#    emulator, plus Maestro CLI: https://maestro.mobile.dev)
e2e/scripts/run-e2e.sh --app-id mobdon.chaogarden.mobile

# 4. Load scenarios against a test deployment (never production)
TEST_BASE_URL=https://staging.chaogarden.example e2e/scripts/run-load.sh garden_sync

# 5. Verify every recorded report passes the thresholds in thresholds.json
node e2e/scripts/verify-thresholds.js
```

Reset local test state safely (temporary data dirs only; refuses to run when
`CHAO_DATA_DIR` points at a live/owner directory):

```sh
e2e/scripts/reset-test-state.sh
```

## Maestro flows (`maestro/flows`)

`guest-onboarding.yaml`, `adoption.yaml`, `feeding.yaml`, `training.yaml`,
`decoration-placement.yaml`, `offline-restart.yaml`, `login-refresh.yaml`,
`friend-visit.yaml`, `widget-deep-link.yaml`.

Each flow drives the real binary (simulator or emulator), starts the local
Rust authority on an ephemeral port with a disposable `CHAO_DATA_DIR`, and
asserts on UI text plus a final `/api/status` probe recorded to
`e2e/reports/maestro/`.

## Load scenarios (`k6`)

`garden-sync.js` (read-heavy snapshot polling), `care-command-burst.js`
(write burst with idempotency keys), `websocket-connect.js`,
`room-fanout.js` (join + broadcast fan-out), `reconnect-storm.js`
(resume-only sessions and re-subscription). Shared thresholds live in
`k6/thresholds.js`; summaries are written as JSON to `e2e/reports/k6/`.

## Contract compatibility (`integration/contract.test.js`)

Pins the mobile client, phone widget, and WebSocket event shapes against the
API, including the two older supported snapshot versions (`v1`, `v2`) that the
widget snapshot cache may still hold. Snapshot versions are additive; removing
a field requires bumping `snapshotVersion` in the contract fixture and keeping
a migration entry.

## Thresholds

See `thresholds.json`. Failures are enforced in `verify-thresholds.js` and by
k6's own `thresholds` option (a failing k6 threshold exits non-zero). Key
gates: API p95 latency per endpoint, error rate, dropped-event rate, database
connection ceiling, zero duplicate economy mutations, and WebSocket
handshake/resume success rates.

## Diagnostics without leaking secrets

- Server logs go to `e2e/reports/logs/` via `reset-test-state.sh` / the runner
  scripts; the server already redacts token values from request logs.
- Sessions are bearer credentials: runners export `TEST_TOKEN_FILE`
  (a file path, never an inline env var), and `verify-thresholds.js` scrubs
  anything matching `(token|authorization)["':\s=]+[A-Za-z0-9._-]{8,}` from
  report JSON before it is written.
- CI never prints `API_DEPLOY_TOKEN`, `TEST_TOKEN`, or database URLs; use the
  `${VAR:+set}` form when echoing configuration.

## CI wiring

The nightly `release-test` job (added outside this directory per repo policy)
should call `e2e/scripts/run-integration.sh`, upload `e2e/reports/` with
`actions/upload-artifact` using `if: always()`, and run
`e2e/scripts/verify-thresholds.js` as the final gate. The deploy workflow keeps
using `scripts/smoke-test.sh` as the fast gate.
