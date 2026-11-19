# Measured capacity — November 18, 2026

**Sustained 100–500-player capacity is not verified.** The 100-connection,
zero-offline-resident 30-second fixture met the timing budgets, but its
120-second repeat reached 603 ms p95 simulation-clock lag and failed the
proposed 250 ms budget. Additional residents and the 250/500-connection cases
failed more severely. Small simulation-work timings alone hide scheduling
failure. No public deployment, service provisioning or live-save change occurred.

## Exact source and environment

Measured source base: `282f6abd1bdb75d0477622f8952667073dc2b751` in
`mobdon/ChaoGardenMMOServer`, plus this isolated Rust capacity patch. It fixes
the three upstream compile errors without adopting the unaccepted Expo,
NestJS/Prisma or TypeScript simulation. Later coordination/auth scaffold commits
remain separate upstream history. The working preview is untouched.

The final measured release binary SHA-256 is
`4c372866426d7bdd4e5e0df186b0ee92b09b2132f7a297909532935c1deba1bb`.
See `measurements/2026-11-18/BUILD.json` for source hashes. Rust/Cargo 1.100.0,
locked existing dependencies, Windows 10.0.19045, Intel Core Ultra 9 285K,
24 logical CPUs, 33,157,505,024 bytes RAM; generator Node 26.13.0 ran on the
same machine. Other host workloads were not isolated. The temporary-data
volume's disk model/IOPS were not characterized. These are local measurements,
not Linux/cloud sizing evidence.

## Workload and measurement definitions

`tools/capacity.mjs` creates synthetic regular-role profiles in a fresh temporary
directory, disables bootstrap, and starts its own Rust release binary on
`127.0.0.1:0`. It accepts no external target URL. No live profiles, native assets
or services are used. Profiles contain hashed credentials and remain Rust-owned;
the JavaScript tool is a traffic generator, not gameplay authority.

Each matrix case lasts 30 seconds after setup. Of 100/250/500 requested sockets,
2% are raw non-reading WebSocket peers; fast peers use real validated room
intents across hub/Neutral/Hero/Dark, movement at 10 Hz, a save request every
1.1 seconds, and one reconnect every two seconds. Slow peers stay in hub.
Residents are **additional disconnected Chao**, distributed across gardens
(half Neutral, a quarter Hero, a quarter Dark). Four zones share one simulation
and mutex. At 500 connections, 3 and 8 room transitions were still pending after
the three-second warmup in the zero/500-resident cases; all 500 sockets were
initially admitted. These failed transitions remain recorded in `setup`.

The fixture uses flat fallback terrain. Race/combat/shop/positive minigame
rewards have correctness tests, **not representative network load coverage**.
No party transitions, real Android radios, TLS overhead, asset downloads,
database migration, multi-hour soak or 500-player restart recovery was tested.
LOAD-100/250/500 and public-release acceptance remain open.

Server p95 values below are sampled histogram **bucket bounds**. Historical
histogram exposition read atomics separately; the soak contains a one-sample
bucket/count inconsistency from a concurrent scrape. Treat server quantiles as
approximate. The final candidate synchronizes record/render per histogram and
tests concurrent scrape invariants; this metrics-only repair postdates the
matrix binary. See `CANDIDATE-BUILD.json` and its small gated smoke. The matrix
was not repeated against that candidate, and no new capacity claim follows.
Client lag is `max(0, wall time - serverTime)`, so it includes skipped simulation
time and is not pure network transit. Client quantiles use a bounded 20,000-value
sample; all save/reconnect samples are retained. Inspect intervals and cadence
alongside lag. Proposed gates are tick work p95 below 50 ms and lag p95 below
250 ms; the harness additionally requires cadence at least 19 Hz near the
20 Hz target. Snapshot traffic excludes WebSocket/TLS/TCP framing and events.

## Results

| Sockets / extra residents / seconds | Tick Hz | Tick work p95 ≤ ms | Lock wait p95 ≤ ms | Snapshot build p95 ≤ ms | Save commit p95 ≤ ms | Client lag p95 ms | Snapshot MiB/s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 / 0 / 30 | 19.989 | 0.5 | 10 | 10 | 5 | 8.951 | 27.461 |
| 100 / 100 / 30 | 11.944 | 0.5 | 25 | 50 | 25 | 11,160.303 | 54.287 |
| 250 / 0 / 30 | 4.277 | 0.5 | 25 | 25 | 5 | 18,895.272 | 67.628 |
| 250 / 250 / 30 | 1.558 | 1 | 50 | 50 | 10 | 30,141.179 | 55.414 |
| 500 / 0 / 30 | 1.062 | 1 | 25 | 25 | 10 | 34,801.542 | 73.181 |
| 500 / 500 / 30 | 0.386 | 2.5 | 100 | 100 | 50 | 44,992.348 | 48.872 |
| 100 / 0 / 120 | 19.899 | 0.5 | 10 | 25 | 5 | 603.032 | 27.037 |

Falling traffic at overload is missed work/disconnects, not efficiency. The
100/0 short run alone generated about 230 Mbit/s of snapshot payload. No cheap
host or mobile bandwidth suitability is established.

| Sockets / residents / seconds | Snapshot interval p95 ms | Save ack p95 ms (samples) | Reconnect first snapshot p95 ms | Server CPU cores | End RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 100 / 0 / 30 | 111.594 | 15.564 (27) | 9.223 | 0.594 | 29.695 |
| 100 / 100 / 30 | 186.898 | 133.266 (27) | 102.952 | 1.336 | 42.383 |
| 250 / 0 / 30 | 439.950 | 295.399 (1) | 268.752 | 1.251 | 77.445 |
| 250 / 250 / 30 | 1,527.163 | 1,478.153 (7) | 1,018.925 | 1.353 | 119.410 |
| 500 / 0 / 30 | 2,248.925 | 1,387.648 (4) | 909.831 | 1.344 | 226.242 |
| 500 / 500 / 30 | 7,621.895 | 2,089.666 (2) | 1,740.692 | 1.424 | 223.340 |
| 100 / 0 / 120 | 112.634 | 27.024 (109) | 22.027 | 0.574 | 28.508 |

CPU is process CPU-time delta divided by wall time (one core = one logical CPU
fully occupied), and includes a small resource-sampling boundary discrepancy.
RSS is an end sample; 500/500 process peak was 325.969 MiB. A one/two-save p95
has very limited meaning. The 120-second run had 59 reconnects, no unexpected
fast-peer disconnects and stable RSS, but that does not erase its failed lag gate.
Reconnect timing measures the first canonical snapshot, **not** activity/room
resumption: existing final-disconnect behavior returns direct players to hub
and cancels unfinished activities.

Every 30-second case had 14 reconnects and retained the expected unique profile
IDs after clean shutdown (100, 200, 250, 500, 500, 1,000 respectively). Historical
harnesses asserted uniqueness before emitting results. This is saved identity
count evidence, not crash recovery or complete transaction validation.

No cross-zone player leaks, missing self, broadcast-lag entries, oversized
snapshots or disk-save failures were observed. Non-reading peers hit send
deadlines in the first five matrix cases: 2/2/5/5/10 disconnects. Event queue
high water was 27/26/145/171/386, then 407 in 500/500 (configured 512).
At 500/500, overload accumulated normal readers' incoming movement and 159
hit the 80-message/s processing limit. Final connection gauge was 343. No
send-deadline counter increment occurred in that case: TCP buffers did not
necessarily fill before scheduling/rate failures. Deterministic blocked-sink
tests separately exercise the deadline.

Save cooldowns follow simulation time; overloaded runs rejected wall-clock
save attempts (17/17/12 in 250/250, 500/0, 500/500). Harness revision 3 clears
rejected requests to observe later saves. Earlier 250/0 revision 2 held one
rejected request outstanding, leaving only one save sample. Revision 4 preserved
failed setup instead of aborting before measurement. Historical exit code 0
did **not** validate timing; use the current fail-closed gate evaluator, whose
missing-evidence failures are explicit. Raw observations are not rewritten.

## Resident cost and filtering comparison

The release `capacity_core` microbenchmark runs 400 ticks and 200 snapshot builds
per case without sockets/disk. It asserts actual Watch opt-in for every resident.

| Connected + residents | Tick p95 ms | Resident decision hook p95 ms | Snapshot build/encode p95 ms |
| --- | ---: | ---: | ---: |
| 100 + 0 | 0.0771 | 0 | 0.7215 |
| 100 + 100 | 0.1199 | 0.0032 | 1.9002 |
| 250 + 0 | 0.1835 | 0 | 1.6168 |
| 250 + 250 | 0.3290 | 0.0071 | 4.2577 |
| 500 + 0 | 0.3774 | 0 | 3.1672 |
| 500 + 500 | 0.6652 | 0.0135 | 9.6753 |

The resident timer wraps custom Watch decisions only; movement, terrain and
profile iteration remain in total tick time. It is not retail Chao AI. The
500-resident whole-tick p95 increase is about 0.288 ms in this fixture; repeated
snapshot construction and contention dominate the service's failure.

For 500 clients without residents, reconstructed original global snapshot shape
is 554,810 bytes versus 138,988–139,642 filtered bytes, about 75% smaller. With
500 residents, global is 1,277,306 bytes versus 138,988–381,066 by room. These are
serialized fixture comparisons, not an unmodified upstream network benchmark.

`baseline/` contains an earlier **already filtered** build before releasing the
mutex ahead of final JSON encoding. In 100/0, mean snapshot work/lock wait went
from 4.926/3.181 ms to 4.085/2.164 ms in a later local run; traffic stayed the
same. This is not an isolated statistical speedup guarantee. Baseline revision-1
unexpected-close accounting had a callback race; do not infer peer failures
from that field. Final matrix callback accounting is corrected.

## Boundaries, limitations and next work

- Room-filtered players/residents/rings/enemies/mission and public effects;
  native races separate course variants, Karate bouts remain private, and self
  data stays present. Existing wire field shapes and canonical ownership remain.
- One canonical latest input, 2 KiB incoming frames, 80 incoming messages/s per
  socket; no input task queue. Runtime drains `Game.events` after mutations and
  ticks into the bounded broadcast. Lag disconnects rather than silently losing
  reliable events. All-room broadcast retention still precedes filtering.
- One in-flight outbound frame, explicit write-buffer bound, default 1 MiB
  snapshot bound and 1-second deadline for initial/control/snapshot frames;
  connection semaphore and new-profile cap. The snapshot cap is checked after
  serialization, so transient CPU/RAM still scales with room population.
- One mutex still owns every room. Full-store JSON export/fsync/rename blocks
  mutations and simulation. Tiny receipt/credit still commit together; Watch
  rollback and durable Save acknowledgments remain. Shop/mission/prototype-race
  notices retain periodic-save semantics, not universal crash-durable receipts.
  Duplicate bearer sockets share one Chao but can issue competing intents;
  exclusive activity/device leases are another workstream.
- No shards, room admission caps, directory/party handoff or independent mission
  instances. Default 600 connections is a safety bound, not measured capacity.
  The private deployment draft's 100 setting does not certify sustained 100
  clients or 100 clients plus arbitrary residents. Public accounts/store and
  Android reachability remain unaccepted/unverified.

Prioritize repeated per-recipient public-state construction, payload/interest
size, room/resident admission budgets and scheduling fairness before worker
distribution. Cache/delta changes need self/reward freshness and invalidation
tests; naive `Value.clone()` still allocates. Repeat representative/longer load
after each measured improvement. Never split writers over one JSON store or
acknowledge rewards before atomic commit. Do not claim 500-player soak/recovery
when the short gate already fails.

## Reproduce

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release --locked --bin chao-game-server --example capacity_core
mkdir -p measurements/repeat
cargo run --release --locked --example capacity_core > measurements/repeat/core.json
node tools/capacity.mjs --players 100 --residents 0 --seconds 30 --out measurements/repeat/network-100-0.json
node tools/capacity.mjs --players 100 --residents 100 --seconds 30 --out measurements/repeat/network-100-100.json
node tools/capacity.mjs --players 250 --residents 0 --seconds 30 --out measurements/repeat/network-250-0.json
node tools/capacity.mjs --players 250 --residents 250 --seconds 30 --out measurements/repeat/network-250-250.json
node tools/capacity.mjs --players 500 --residents 0 --seconds 30 --out measurements/repeat/network-500-0.json
node tools/capacity.mjs --players 500 --residents 500 --seconds 30 --out measurements/repeat/network-500-500.json
node tools/capacity.mjs --players 100 --residents 0 --seconds 120 --out measurements/repeat/network-soak-100-0.json
node tools/summarize-capacity.mjs measurements/repeat
node tools/capacity-gates.mjs measurements/repeat
node --test tools/capacity-gates.test.mjs
python3 -m unittest discover -s deploy -p 'test_*.py' -v
```

Use Node 22+ native WebSocket and Python 3.10+. Commands are independent; failed
load cases should return nonzero after saving evidence. Windows resource sampling
uses Get-Process, Linux uses /proc and ps. PowerShell can invoke the same programs
and use New-Item/Set-Content instead of the POSIX directory/redirection examples.
The real HTTP/WS regressions cover canonical service round trips and Tiny reward
failure/replay/restart boundaries. See [deployment and Android acceptance](CAPACITY-OPERATIONS.md)
for private TLS, monitoring and tested backup/restore tooling. Caddy/systemd/
Prometheus config execution and physical Android reachability remain untested.

Final local validation: 90 Rust host tests, 21 Tiny core tests (one separate
licensed-local-reference test ignored), 22 Tiny-game tests, 11 gate tests and
five Python backup/preflight tests passed; strict Clippy and rustfmt passed.
The final candidate's 12-client/12-resident 10-second smoke passed the strict
gate with nine saved acknowledgments, four reconnects and 24 preserved canonical
profiles/credentials/roles/resident opt-ins. This checks the candidate's real
service path and coherent metrics, not its 100–500-player capacity. The read-only
gate evaluator exits 1 for the historical matrix/soak and preserves their raw
evidence. `GATE-RESULTS.jsonl` records current-policy evaluation, including old
missing-evidence failures; the current candidate smoke passes reevaluation.

### Integration with care and away competitions

Current room filtering includes away competitors in their own rooms. Save
timing wraps the existing care, Tiny reward and competition transaction, and
JSON decoding preserves fractional care timestamps across reload. The combined
source passed 153 Rust host tests, 63 Node tests, five backup/preflight tests,
formatting and strict Clippy. Three tests still require local native fixtures.

The 12-client, 12-resident release smoke initially missed the tick and snapshot
age gates at 18.78 Hz and 584.56 ms p95. A 30-second repeat at the same client
and resident counts passed at 19.96 Hz and 53.82 ms p95, with 27 save
acknowledgments, zero cross-room leaks, and all 24 canonical identities,
credentials, owner flags and resident choices preserved. The release binary
SHA-256 was `21c4450e6505fa75a1c39f54193d20a668738aa1a695075ade69870dfc343c81`.
Sustained 100–500-player capacity and deployment acceptance remain open.
