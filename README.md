# Chao Garden MMO server

Rust authority for the browser Chao Garden prototype. This is a custom gameplay
implementation, not a port of Sonic Adventure 2's native engine. Native models,
motions, textures, music, and terrain can be imported by the separate client tools
from the owner's game installation.

Client repository: `mobdon/ChaoGardenMMOClient`.
Server repository: `mobdon/ChaoGardenMMOServer`.

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
  imports/                        # allowlisted .glb, .png, and .mp3 files
```

Profiles store SHA-256 token hashes, progression, alignment, cosmetic ownership,
and validated vehicle selection. Node reference profiles are compatible with the
Rust server. Coordinates and connection state reset to a safe hub spawn on load.
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
- `GET /ws?token=...`: authenticated WebSocket; receives snapshots at 10 Hz.
- Simulation runs at 20 Hz. Movement messages carry `dx`, `dz`, and `jump` intent.
- Actions carry `type: "action"` and an action name. Client coordinates, currency,
  stats, roles, damage, and reward claims are ignored.

Gameplay covers hub/gardens and alignment gates, training, fixed-price shopping,
rings, ordered races, cooperative robot missions, and Chaos evolution. The
owner's `mount-dark-walker` action sets `vehicle` to `"dark-chao-walker"`;
`dismount-vehicle` clears it. The mount currently preserves ordinary movement
and combat rules. Vehicle selection authorizes presentation; native mech
physics and weapons are not implemented.

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
