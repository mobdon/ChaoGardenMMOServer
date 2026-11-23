# Capacity operations and deployment review

Prepared November 18, 2026 for the **Rust** `chao-game-server` authority.
These files are review artifacts. No service, tunnel, DNS entry, certificate,
cloud resource or firewall rule was created. The live local preview and its
owner profile must remain untouched. Measured results belong in the capacity
report; a configured connection limit is not proof of supported population.

The generated Expo mobile app, NestJS `apps/api`, TypeScript simulation and
Prisma delivery through `282f6ab` are **UNACCEPTED** under the user's Rust
requirement. They are not a replacement for the working Rust authority or
evidence of integrated accounts, PostgreSQL persistence or device ownership.
Do not run their API, database migrations, old Terraform or `scripts/migrate.sh`
as part of this runbook. This review applies only to `deploy/` and the Rust
binary; it does not accept or roll out those competing implementations.

Before integrating this branch, read the current server coordination
`docs/coordination/BOARD.md`, `PROTOCOL.md`, `VIBE-REVIEW-2026-11-18.md` and
`VIBE-PR7-REVIEW-2026-11-18.md`. The proposed account/device v2 contract remains
an integration dependency, not a deployed API. Shared runtime changes must be
reviewed with those owners; publishing source does not authorize migration or
runtime rollout.

## Single authority boundary

Run exactly **one** Rust process per data directory. Every Chao, activity clock,
wallet and receipt has one canonical in-process owner. Never scale replicas
against one `profiles.json`, mount a shared write volume on two workers, or run
the Node reference alongside Rust against the same directory. Zone interest
filtering is not cross-process sharding, and does not create a party handoff,
room directory or independent mission instance.

`persist` still serializes the complete profile store, fsyncs and atomically
replaces the JSON file while holding the authority mutex. Pending Tiny rewards
become live only after the save commits, preserving the receipt/wallet unit.
That synchronous store and its growth are measured limits. A queue or worker
must not acknowledge a reward before its durable commit. Reconnect retries use
the canonical profile and receipt; a new worker cannot independently replay a
pending reward. Other progression saves retain their existing semantics; this
change does not turn all gameplay actions into transactional database writes.

## Review artifacts

| File | Behavior and boundary |
| --- | --- |
| `deploy/Caddyfile.review` | TLS on `127.0.0.1:3443` only, internal CA, no redirect listener, no trust-store installation and no access log. Exact POST `/api/session` and GET `/ws` proxy to loopback Rust. Everything else returns 404. |
| `deploy/chao.env.example` | Separate absolute data directory, loopback listener, bootstrap disabled, explicit origin and bounded runtime settings. The 100-connection setting is a review starting point. |
| `deploy/chao.service` | Non-root Linux single authority, private state directory, restricted filesystem and SIGTERM shutdown. A deliberate `/etc/chao/enable-private-playtest` marker is required; the file does not enable itself. |
| `deploy/preflight.py` | Refuses bootstrap, a non-loopback authority, relative data directories and launcher stdin shutdown. Reads only the actual service environment. |
| `deploy/prometheus.yml`, `deploy/alerts.yml` | Same-host private scrape and proposed budget/failure alerts. No credential or player labels. |
| `deploy/backup.sh`, `deploy/backup.py` | Offline, checksummed full-data backup; verified restore into a new directory. No upload, deletion policy or service purchase. |

The local Caddy example intentionally cannot be reached from an Android phone
or the public Internet. It can validate TLS/upgrade behavior on a separate test
installation after explicit operator setup. The internal CA is not a publicly
trusted phone certificate. Caddy reverse proxy handles the WebSocket upgrade;
do not add an ordinary response buffering queue. Keep the default rejection of
`/metrics`, `/livez`, `/readyz`, `/api/status`, `/api/assets/*`, saved data, native
imports and administration. Status currently includes local inventory/path
information. Serve approved client assets separately after the asset delivery
design is accepted.

For a future approved Linux test host, review the binary/hash, create the
`chao` user/group, place the release binary at `/opt/chao/chao-game-server` and
the reviewed helper under `/opt/chao/deploy`, and place a protected env file at
`/etc/chao/chao.env`. Populate a **new test** data directory with approved
terrain/catalogs, never the preview `.local` directory. Ensure unit/environment
ownership and mode prevent unprivileged edits. Validate before any intentional
start:

```sh
caddy validate --config deploy/Caddyfile.review --adapter caddyfile
systemd-analyze verify deploy/chao.service
promtool check config deploy/prometheus.yml
promtool check rules deploy/alerts.yml
python3 -m unittest discover -s deploy -p 'test_*.py' -v
```

The Linux daemon tools were unavailable on the Windows measurement host; their
validation and a real service stop/start remain deployment acceptance steps.
Python backup/preflight tests cover byte-preserving restore of canonical IDs,
resident state and reward receipts, missing/duplicate identity, corruption,
unsafe archive paths and refusal to overwrite an existing restore directory.
No restore of the live save was attempted. Readiness probes test a real
temporary write/fsync/unlink, but do not prove account or storage correctness.

## Monitoring

Keep Rust `3001`, Prometheus `9090` and all administrative endpoints on loopback
or a private management network. The example scrape uses loopback and needs no
public proxy route. Bind the Prometheus UI to loopback when intentionally run.
Do not enable HTTP/proxy tracing that records `/ws?token=...`, request bodies or
session responses. The current query-string bearer credential must be replaced
or scrubbed at **every** logging/tracing layer before public access.

Track these series together:

| Signal | Series |
| --- | --- |
| Tick budget and lock contention | `chao_tick_duration_seconds`, `chao_tick_lock_wait_seconds` histograms |
| Resident AI | `chao_resident_tick_duration_seconds` histogram and `chao_residents` gauge |
| Snapshot work and traffic | `chao_snapshot_duration_seconds`, `chao_snapshot_bytes_total`, `chao_snapshots_total` |
| Socket pressure | `chao_ws_connections`, `chao_slow_clients_total`, `chao_event_lag_total`, `chao_event_queue_depth`, `chao_event_queue_high_water` |
| Churn/admission | `chao_reconnects_total`, `chao_admission_rejected_total`, `chao_input_rate_disconnects_total`, `chao_oversized_snapshots_total` |
| Durable saves | `chao_save_duration_seconds`, `chao_persist_failures_total` |

Confirm exact names against `/metrics` in the final build. Histograms describe
server work; tick duration excludes lock wait, and snapshot generation time is
not end-to-end snapshot age. Measure client receive age and reconnect-to-first-
snapshot in the harness. HTTP upgrade latency is not lifetime WebSocket
latency. Scrape histograms use coarse buckets, so retain raw harness results.

Provisional gates are tick p95 under 50 ms and snapshot receive age p95 under
250 ms. Alert immediately on save failures, investigate send deadlines/event
lag, and correlate reconnect bursts with admission and CPU/RSS. Add host disk
free/inodes, CPU/RSS, clock, certificate expiry and backup-age/restore-drill
monitoring on the actual host; no host exporter, alert receiver or on-call
delivery has been configured here. Alerts in this PR are rules to review,
not evidence that a notification has been delivered.

## Backup and recovery

The initial recovery policy is a **proposal**: a daily offline backup and a
pre-upgrade backup, with the last seven daily and four weekly copies retained
in encrypted storage on a separately controlled host. Daily backups imply up
to 24 hours of loss after a total disk failure. Save fsync does not replace an
off-host backup. Measure archive size, downtime and restore time before choosing
a shorter interval. No remote retention/deletion/upload job is installed, and
RTO has not been measured. Do not promise continuous availability from this
stop-the-world JSON backup strategy.

On the dedicated Linux test host only, after confirming `/var/lib/chao` is the
sole data directory and there are no writers outside `chao.service`:

```sh
sudo bash /opt/chao/deploy/backup.sh /var/lib/chao /srv/chao-backups/unique-name.zip
python3 /opt/chao/deploy/backup.py verify /srv/chao-backups/unique-name.zip
```

The wrapper serializes backup jobs with `flock`, records whether the service
was active, stops it and checks its main process is gone. It attempts to
restart a previously active service on exit, including backup failure. Review
shutdown/save errors in the journal: the archive preserves the last committed
state, not uncommitted progress after a failed save. `backup.py` itself does
not detect live processes; `--offline` is an operator assertion and must never
be used to bypass the stop boundary.

Archives include committed profiles, terrain, catalogs and any private imported
data in that directory; they are private and must not enter Git or client
bundles. The uncommitted `profiles.json.tmp` is omitted. Files have SHA-256 and
length entries; verification rejects duplicate/path/link entries, malformed or
duplicate profile identities, credential hashes and checksum changes. Hashes
detect corruption; they are not encryption or proof against a malicious writer
who can replace an archive and its manifest. Restrict/secure the backup account.
The tool streams file contents; it limits archives to 100,000 entries/64 GiB.

For a recovery drill, stop the only authority and restore into a new sibling:

```sh
python3 /opt/chao/deploy/backup.py restore /srv/chao-backups/unique-name.zip /var/lib/chao-restored --offline
```

Keep the previous directory intact for rollback; do not merge stores or copy
only wallets/receipts independently. Correct owner/group on the restored
directory, and test the restored store with the **same reviewed binary** on an
isolated loopback port before changing the production `CHAO_DATA_DIR`. Check
one canonical Chao per credential, owner roles, rings/inventory, Tiny receipt
and resident restoration. Confirm reconnect and repeated result/save intents
do not award twice. Stop the test authority before the production process uses
that directory. A production identity/store migration must provide its own
versioned, transactional migration and rollback test. The existing JSON
`migrate` normalization command is not a PostgreSQL migration and is excluded
from this procedure; it must not rewrite the canonical save as a release step.

## Android and HTTPS/WSS acceptance

No Android device, public DNS, certificate issuance or external network path
was exercised. `localhost` on a phone refers to that phone, not this computer.
An emulator's host alias is a development convenience, not proof that a real
phone works. Never forward or tunnel the bootstrap preview to solve that gap.

After the public gates below are separately accepted, use an approved test
hostname with a valid publicly trusted certificate and complete chain on
443. Keep Rust/metrics private; proxy only reviewed application routes. Use
HTTPS session requests and WSS sockets from the actual Tiny Garden origin.
Set that exact HTTPS origin on the authority, verify the browser's existing
same-origin/CORS behavior, and test native clients separately (native requests
may lack an Origin header). Do not enable global cleartext traffic or install
a permissive trust manager in a release Android app.

Record the APK/browser version, physical Android version, hostname/certificate,
test network and timestamps for each check:

1. Wi-Fi and cellular independently resolve DNS and complete HTTPS/WSS; test
   IPv4 and IPv6 when both are advertised. No mixed-content or CORS errors.
2. Resume-only with an unknown token fails without creating a new Chao. A valid
   approved credential resumes the same canonical ID and saved revision/state.
3. Background/foreground, airplane-mode recovery and Wi-Fi/cellular switching
   reconnect with bounded retry/backoff and a fresh snapshot. Cached widget
   state is visibly stale and cannot claim a reward.
4. Save/result acknowledgement follows durable commit. Disconnect after the
   commit but before acknowledgement, reconnect/retry, and verify the receipt
   and wallet exactly once. Repeat with injected save failure on test data.
5. Revoke a paired device and test an expired/replayed credential once the
   pairing workstream is integrated. Existing shared bearer sockets can issue
   competing intents; this branch does not implement exclusive device leases.
6. Confirm public `/metrics`, `/api/status`, private assets, probes, administration
   and private data remain unreachable. Inspect proxy/app logs for token leaks.

## Gates before changing any listener to public

The configuration is deliberately private until integration validates production
identity and server-assigned roles; revocable device pairing/exclusive activity
ownership; protocol/version and abuse controls; the intended Steam/DLC
entitlement approach; lawful native-asset distribution; durable account/Chao/
inventory transactions and a tested migration; backup/restore/alert delivery;
trusted HTTPS/WSS and the physical Android tests. Local installation detection
is not entitlement verification. The public session endpoint currently permits
new profile creation and is not a production account platform.

Repeat 100/250/500 representative clients on the intended host with residents,
movement, real enabled activities, purchases, transitions, save bursts, slow
readers and reconnect storms. Then run a long soak, process restart and recovery
drill. Publish payload sizes, hardware, room distribution, p95/p99, CPU/RSS,
queue high water, disk characteristics and failures. A crowded single room
requires its own test. Stop admissions or reduce caps when a budget fails;
do not introduce shards before measuring the limiting path. Future workers
need explicit single-owner leases, transaction idempotency, destination
validation and reconnect/party handoff before sharing population.

## Primary configuration references

- Caddy reverse proxy/WebSocket handling: https://caddyserver.com/docs/caddyfile/directives/reverse_proxy
- Caddy binding: https://caddyserver.com/docs/caddyfile/directives/bind
- Caddy automatic HTTPS and local CA: https://caddyserver.com/docs/automatic-https
- Caddy global options: https://caddyserver.com/docs/caddyfile/options
- systemd service process lifecycle: https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html
- systemd state directories and sandboxing: https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html
- Prometheus histogram queries: https://prometheus.io/docs/prometheus/latest/querying/functions/#histogram_quantile
- Android network security configuration: https://developer.android.com/privacy-and-security/security-config
- Android emulator networking: https://developer.android.com/studio/run/emulator-networking
