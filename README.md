# Chao Garden MMO server

Rust authority for the browser Chao Garden prototype. This is a custom gameplay
implementation, not a port of Sonic Adventure 2's native engine. Native models,
motions, textures, music, and terrain can be imported by the separate client tools
from the owner's game installation.

Client repository: `mobdon/ChaoGardenMMOClient`.
Server repository: `mobdon/ChaoGardenMMOServer`.

## TypeScript monorepo (pnpm + Turborepo)

The TypeScript workspace lives alongside the Rust authority:

| Path | Package | Role |
| --- | --- | --- |
| `apps/mobile` | `@chaogarden/mobile` | Expo React Native client |
| `apps/api` | `@chaogarden/api` | NestJS API server (auth, sessions, gameplay bridge) |
| `packages/contracts` | `@chaogarden/contracts` | Shared Zod schemas / API contracts |
| `packages/simulation` | `@chaogarden/simulation` | Deterministic game logic (pure, no platform deps) |
| `packages/config` | `@chaogarden/config` | Shared TypeScript, ESLint and Prettier settings |

### Prerequisites

- Node.js >= 20
- pnpm >= 9 (`corepack enable` works out of the box)
- Docker (for local PostgreSQL and Redis)
- Rust toolchain only if you are also running the authority server below

### Local startup

```sh
corepack enable
pnpm install --frozen-lockfile   # or plain `pnpm install` after adding a dep

docker compose up -d             # PostgreSQL (5432) + Redis (6379)

cp apps/api/.env.example apps/api/.env       # fill in a real JWT_SECRET
cp apps/mobile/.env.example apps/mobile/.env # only public values here

pnpm dev                         # API + mobile dev servers together
```

### Everyday commands (from the repo root)

```sh
pnpm dev                 # turbo run dev (long-running)
pnpm build               # build all workspaces in dependency order
pnpm lint                # ESLint everywhere
pnpm typecheck           # tsc --noEmit everywhere
pnpm test                # Vitest/Jest suites
pnpm format              # Prettier write (`pnpm format:check` for CI)
pnpm clean               # remove build artifacts
turbo run typecheck --filter=@chaogarden/api   # a single package
```

Everything must work from a clean checkout: `pnpm install --frozen-lockfile`
followed by `pnpm lint && pnpm typecheck && pnpm test && pnpm build`.

### Conventions

- Strict TypeScript everywhere; shared compiler options in `packages/config/tsconfig/*`.
- ESLint flat configs per workspace extending `@chaogarden/config/eslint`; Prettier at the root.
- Environment is validated with Zod on startup (`apps/api/src/config/env.ts`);
  copy the `.env.example` files to `.env` and never commit secrets.
- Package boundaries are lint-enforced: the mobile client cannot import
  server-only modules (`@nestjs/*`, `@prisma/*`, `ioredis`, ...), the API cannot
  depend on the mobile client, and the simulation package forbids platform
  dependencies.
- Local services: `docker-compose.yml` provides PostgreSQL and Redis.

### CI

Continuous integration runs `pnpm install --frozen-lockfile` (with the pnpm
store cached), then lint, typecheck, tests, and builds on every push and pull
request. The workflow definition is checked in at `docs/ci/github-actions.yml`
(this repository's automation policy does not allow tooling tasks to write
`.github/workflows/` directly; copy it to `.github/workflows/ci.yml` when
enabling CI).

## Run Rust

Install stable Rust, then run from this repository:

```sh
cargo test --locked
cargo run --release --locked
```

The listener defaults to `127.0.0.1:3001`. The browser client proxies `/api` and
`/ws` to it. For a separately hosted client, configure the browser origin before
binding the server beyond loopback.

The runtime data directory defaults to `./data` relative to the process working
directory. Set `CHAO_DATA_DIR` to an absolute path to keep profile and asset
locations stable. For the original combined development workspace, point it at
the client's `.local` directory to retain the existing owner profile and imports.

PowerShell example:

```powershell
$env:CHAO_DATA_DIR = 'C:\path\to\ChaoGardenMMOClient\.local'
$env:GAME_HOST = '127.0.0.1'
$env:GAME_PORT = '3001'
cargo run --release --locked
```

Configuration:

| Variable | Default | Meaning |
| --- | --- | --- |
| `CHAO_DATA_DIR` | `./data` | Persistent profiles, terrain, imported assets, cached inventory |
| `GAME_HOST` | `127.0.0.1` | Listener IP address |
| `GAME_PORT` | `3001` | Listener port; `0` selects an available port |
| `CHAO_OWNER_BOOTSTRAP` | disabled | `1` or `true` grants the first profile owner powers; loopback only |
| `CHAO_ALLOWED_ORIGINS` | empty | Comma-separated HTTP(S) browser origins |
| `CHAO_SHUTDOWN_STDIN` | disabled | `1` accepts a `shutdown` line (or launcher EOF) on stdin for a clean save and exit |

Owner bootstrap must be explicitly enabled for a new private development data
directory. Existing owner profiles retain their server-stored role with bootstrap
disabled. Request fields never grant ownership.

The development launcher enables stdin shutdown when starting the Rust binary,
so Windows restarts flush pending progression before the process exits. A direct
terminal launch supports Ctrl+C; Unix deployments also support SIGTERM.

## Data and assets

The server reads:

```text
data/
  profiles.json
  game-inventory.json             # optional client-generated inventory
  terrain/{hub,garden,hero,dark}.json
  activities/{races,karate}.json   # optional locally extracted source catalogs
  activities/terrain/{worldId}.json
  imports/                        # fixed .glb, .png, .mp3 and Shadow motion JSON aliases
  imports/activities/              # four exact native activity model aliases
```

Profiles store SHA-256 token hashes, progression, alignment, cosmetic ownership,
and validated vehicle selection. Node reference profiles are compatible with the
Rust server. Direct-control profiles reset to a safe hub spawn on load. An
explicitly opted-in resident restores once at a safe eligible garden position
with a fresh routine session; service downtime creates no simulated activity.
Profile writes use an atomic temporary-file replacement. Keep backups of the
data directory before upgrading an existing installation.

Terrain is server-owned. Missing terrain files use a flat development fallback
reported by `/api/status`; malformed files fail startup. A terrain JSON object
contains `minX`, `minZ`, `step`, `width`, `depth`, and a row-major `heights` array.
A `null` sample represents missing ground.

Assets are loaded only from a fixed route allowlist under `/api/assets/`.
Audio supports byte-range requests. Original game assets and tools are local
data and are ignored by Git. A source checkout and its tests do not require
redistributed Sonic assets.

## Protocol and authority

- `GET /api/status`: health, online connections, asset inventory, terrain state.
- `POST /api/session`: JSON `name` and/or an existing `token`; returns a session.
  `resumeOnly:true` rejects an unknown/missing token with 404 and creates no
  profile, so a title-screen save slot cannot silently become another Chao.
- `GET /api/activities`: detailed original catalog/provenance; compact
  descriptors and current activity state also appear in snapshots.
- The Rust Tiny Garden game protocol, five game IDs, session-bound inputs and
  atomic earned rewards are documented in [PHONE.md](PHONE.md).
- Tiny Watch, shared garden residency while disconnected, deliberate care,
  durable opt-in, and direct-control return are documented in [WATCH.md](WATCH.md).
- `GET /ws?token=...`: authenticated WebSocket; receives snapshots at 10 Hz.
- Simulation runs at 20 Hz. Movement messages carry `dx`, `dz`, and `jump` intent.
- Actions carry `type: "action"` and an action name. Client coordinates, currency,
  stats, roles, damage, and reward claims are ignored.
- `save` explicitly flushes authoritative profiles and returns a direct
  `{"type":"event","kind":"saved","playerId":"...","serverTime":...}`
  only after a successful save. Disk errors return an error notice without a
  success acknowledgement. The browser can wait for this event before returning
  to its title screen. Save requests cannot upload profile state.

Gameplay covers hub/gardens and alignment gates, training, fixed-price shopping,
rings, ordered races, cooperative robot missions, and Chaos evolution. The
native-roster Karate tournament loop and validated-course race authority are
documented in [ACTIVITIES.md](ACTIVITIES.md). All 62 extracted original race
configurations remain disabled until genuine routes and obstacles are verified.
The
owner's `mount-dark-walker` action sets `vehicle` to `"dark-chao-walker"`;
`dismount-vehicle` clears it. While mounted, `mech-fire` (or `attack`) fires a
server-owned cannon, and held `boost: true` input activates bounded jet thrust.
These are adapted prototype rules; the native mech's physics and full technique
set have not been ported. See [MECH_MOVES.md](MECH_MOVES.md) for exact controls,
authority limits, source motion coverage, and remaining techniques.

Owner gear uses independent persistent `gear.footwear` and `gear.hands` slots.
`equip-owner-gear` accepts `item: "shadow-air-shoes"` or
`item: "knuckles-hammer-gloves"`; `unequip-owner-gear` accepts `slot: "footwear"`
or `slot: "hands"`. These extras attach native visual models and do not grant
stat boosts, Light Dash, or iron-container breaking mechanics. Roles, items, and
slots are validated by the authority.

Requests require an allowed browser origin. Loopback HTTP(S) origins are accepted
for loopback development. WebSocket payloads are limited to 2 KiB and 80 messages
per second per connection. Sessions are bearer credentials, so use HTTPS/WSS
and a suitable reverse proxy for a public deployment.

## Retained Node reference

The original implementation remains runnable for comparison and fallback:

```sh
npm ci
npm test
npm start
```

It uses the same environment variables and data format. Run one authority against
a data directory at a time; two servers must not write the same profile file.
Rust is the primary implementation. Node is a reference, not a second service
that the Rust server requires.

Rust tests exercise deterministic gameplay, persistence, HTTP validation, asset
serving, and real two-peer WebSocket communication in isolated temporary data
directories. The Node tests use synthetic asset bytes and separate temporary
directories as well, so neither suite claims or changes the live owner profile.
