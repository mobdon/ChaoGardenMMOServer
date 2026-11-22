# Tool kit: how every thread uses it (Sonic MMO)

The kit is installed in the cloud environment ("domains", via its Setup script) and on the owner's PC (Claude Code
sessions in C:\Users\Admin\Documents\ChatGPT\mmo and its worktrees). Plugins and MCP servers load at session start.
If a tool below is missing from your tool list, say so in your reply instead of working around it silently.

## Before you change code
1. **claude-mem**: search it first (`search` → `timeline` → `get_observations`) for earlier work on the same system.
2. **graphify**: find the file or system. `graphify explain "<symbol or file>"` and `graphify path "<A>" "<B>"` from
   the repo/worktree root. Run `graphify update .` after you commit. graphify-out/ is never committed.
3. **Serena**: find and edit the exact symbol (Rust via rust-analyzer, JS/TS): `get_symbols_overview`,
   `find_symbol`, `find_referencing_symbols`, `replace_symbol_body`. Use plain Edit only for small in-line changes.
   claude-mem's smart_outline/smart_search are not for code.
4. **Context7**: look up current docs for a library (tokio, axum, serde, wasm-bindgen, Vite, Playwright) before
   guessing its API.

## While you work
- **superpowers** skills: systematic-debugging for bugs, test-driven-development for new logic,
  verification-before-completion before you say anything is done.
- **ponytail**: smallest complete change; run ponytail-review on your diff before opening a PR.
- **MarkItDown**: `markitdown <file>` for PDF/Word/Excel/PowerPoint uploads.
- **SkillsMP**: search for a skill when you hit a new kind of job (needs SKILLSMP_API_KEY).
- **Syntax hook**: "syntax check failed" after an Edit/Write means fix it now.
- rtk is cloud only (not on Windows): it condenses git status/log/ls; re-run as `rtk bypass <cmd>` if output looks cut.
- On the owner's PC, sessions opened before 2026-11-20 07:00 UTC must restart to load the kit.

## Game data (standing rule)
Native porting only. Every value, model, motion, effect, table and timing comes from the real game files at
C:\Users\Admin\Desktop\sonic adventure 2 batt, with the source file and offset recorded. No guessed, tuned,
placeholder or wiki values, and no non-native data or engines unless the owner says so for that piece. If it isn't in the
files, stop and report what is missing.

Kit and rules: /mnt/project-files/toolkit/ (claude-toolkit/).
