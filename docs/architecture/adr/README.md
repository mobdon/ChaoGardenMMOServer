# Architecture records

Updated November 18, 2026. These records replace the generated PR #9 decisions.
"Required" records a user/repository constraint; "proposed" identifies work
that is not shipped. Neither status claims tests or a deployment occurred.

| Record | Status |
| --- | --- |
| [0001 — repositories and Rust](0001-monorepo.md) | Required boundary |
| [0002 — credentials and ownership](0002-authentication.md) | Existing bearer protocol; coordinated v2 proposed |
| [0003 — realtime transport](0003-realtime-transport.md) | Existing raw WebSocket; migration gate |
| [0004 — clocks and simulation](0004-simulation-ticks.md) | Existing adapted host clock; future trusted job clock |
| [0005 — Tiny Garden widget](0005-native-widget-data.md) | Android adapter proposal |
| [0006 — local native sources](0006-original-ip.md) | Required provenance/local-data boundary |
| [0007 — deployment](0007-deployment.md) | Held pending build, store and capacity evidence |
| [0008 — observability](0008-observability.md) | Source limitations and verification requirements |

The legacy filenames remain so links survive. The former TypeScript monorepo,
replacement creature/currency design and purported native widget deployment
are not accepted architecture. Follow [AGENTS.md](../../../AGENTS.md) and the
[coordination protocol](../../coordination/PROTOCOL.md).
