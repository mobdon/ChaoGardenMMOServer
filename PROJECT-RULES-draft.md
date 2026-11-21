# Sonic MMO: tool kit rules (draft for approval)

Filled in from the kit's RULES-TEMPLATE.md for this project. Nothing here is live yet. Once approved, paste the block
into the project instructions (or a `CLAUDE.md` in each repo, if you want it committed). Lines marked **[choose]** are
my suggestions where you need to pick.

Assumed layout (change if you prefer something else):

- Kit: `/mnt/project-files/toolkit/claude-toolkit` (the project's shared folder, so it survives containers and is not
  committed to either repo). **[choose]** the alternative is committing it to `tools/claude-toolkit` in the server repo.
- Shared data (task-observer log, claude-mem save): `/mnt/project-files/toolkit-data`.
- Local clones in a cloud session: `/home/user/ChaoGardenMMOServer` and `/home/user/ChaoGardenMMOClient`.

```text
EVERY SESSION
1. First run:
     KIT_DIR=/mnt/project-files/toolkit/claude-toolkit SHARED_DIR=/mnt/project-files/toolkit-data \
     REPO_DIR=<the clone this thread changes; default /home/user/ChaoGardenMMOServer> \
     bash /mnt/project-files/toolkit/claude-toolkit/install-all.sh
   It ends with check-tools.sh and exits non-zero if any step or check failed: read its FAIL lines. Plugins and MCP
   servers it installs load in sessions started after it. In the session that ran it, use a required Skill by reading
   its SKILL.md (`find ~/.claude/plugins/cache -path '*skills/<name>/SKILL.md'`) and following it.
   (Skip this step when the cloud Setup script already ran the install: check /tmp/toolkit-setup.log, then re-run the
   check with the same env as above plus KIT_KEYS_IN_ENV=1. The Setup-time check runs before keys load, so a skillsmp
   FAIL in that log is expected; a bare `bash check-tools.sh` without the env gives false FAILs.)
2. Work in the local clone of the repo the task belongs to. Server Rust authority, crates and protocol:
   /home/user/ChaoGardenMMOServer. Browser, Tiny Garden and Android widget presentation:
   /home/user/ChaoGardenMMOClient. Keep the two histories separate. The Setup script restores the server clone's graphify graph from
   /mnt/project-files/toolkit-data/graphify/ and updates it (seconds, not a rebuild). After each commit run
   `graphify update .` and copy graphify-out/ back there (`cp -r graphify-out/. /mnt/project-files/toolkit-data/graphify/<repo>/`).
   graphify-out/ never goes in git.
3. Invoke the task-observer skill and run its session-start scan:
   `bash ~/.claude/skills/task-observer/scripts/session-start-scan.sh /mnt/project-files/toolkit-data/task-observer`.
4. End of a session: `bash $KIT_DIR/claude-mem-save.sh`, then `headroom learn --project . --apply` in the clone (Gemini
   key is in the env). Record base commit, changed paths, tests and remaining limits in the server repo's
   docs/coordination/BOARD.md as AGENTS.md requires.
- Call scripts as `bash x.sh` / `python3 x.py` (mounted folders can drop the execute bit).
- After any change to the kit: `bash $KIT_DIR/check-tools.sh --worker` (expect 0 FAIL).

REPO RULES
- Read AGENTS.md in the repo first; its Rust and repository requirements win over anything here.
- Push only to the session's claude/... branch and open a pull request. Merges, deploys, live preview changes, live
  data or save migration, and anything touching credentials need my explicit go.
- Never commit ROMs, installed game files, converted native assets, saves, credentials, zips or graphify-out/.
- Notes, reports and hand-off docs go in /mnt/project-files/ (subfolder per topic); coordination records go in the
  server repo's docs/coordination/ as AGENTS.md describes.

TOOLS
- graphify: the code map. `graphify explain <symbol or file>` / `graphify path <A> <B>` for known names; `graphify
  query` only with a pointed question naming a function or file. Ask it before grepping or opening big files; update
  it after every merge or commit.
- Serena (MCP, tools interface): find_symbol, find_referencing_symbols, get_symbols_overview, replace_symbol_body.
  graphify finds the file or system, Serena then finds the exact symbol (Rust via rust-analyzer, JS/TS via the typescript server in both repos).
  Plain Edit only for small in-line changes and files Serena can't parse. If only serena_repl shows, the install is
  broken: fix it first.
- claude-mem: memory only. Search it before re-investigating anything. Its code tools (smart_search/smart_outline/
  smart_unfold) are denied in settings so code symbols have one path: Serena.
- Headroom: compress big outputs (headroom_compress / headroom_retrieve). MCP only, never in the API path.
- Context7 (MCP): current docs for a library (tokio, axum/serde, wasm-bindgen, Vite, Playwright) before guessing its
  API. Its key lives only in CONTEXT7_API_KEY.
- superpowers: brainstorming, writing-plans, using-git-worktrees, test-driven-development,
  verification-before-completion, systematic-debugging; requesting-code-review for the final review.
- ponytail: the smallest complete change; a ponytail review on the final diff before a PR.
- Caveman: sub-agents and workers only. Replies to me stay in full sentences.
- rtk: compacts git status, git log and ls only; reads, greps, diffs and test runs stay exact. cargo build, cargo
  clippy, cargo fmt --check and rustc output must stay exact too: if one looks cut, re-run it as
  `rtk bypass <command>`. Write `git --no-pager log ...` when you need the log exact.
- MarkItDown: convert files I upload (PDF, Word, Excel, PowerPoint) with `markitdown <file>` before reading them.
- impeccable: client web/UI layer only, never for native Chao assets or effects (native assets first, per AGENTS.md).
- SkillsMP: find a skill for a new kind of job. Read a found skill's source before adopting it; never install one that
  duplicates a kit tool. Key in SKILLSMP_API_KEY (free tier: 50 searches a day).
- claude-code-setup: run "recommend Claude Code automations for this project" when the setup is reviewed or a new
  stack arrives; adopt only what the other tools don't already cover.
- task-observer: log each correction, tool clash or slow rule with its new-observation.sh into
  /mnt/project-files/toolkit-data/task-observer (never the repo); workers never log, they end with an
  "OBSERVATIONS:" line and the controller logs it; I approve every rule change at a review.
- Syntax hook: "syntax check failed" after an Edit/Write means fix it now.
- Commit guard: the clone's pre-commit hook refuses archives, C2PA-stamped PNGs and line-ending flips. Never bypass it
  with --no-verify.
- Browser QA: Playwright with the preinstalled Chromium; never run `playwright install` in a cloud session.

TOOL HEALTH (keep them working, no conflicts)
- One tool per job, no overlap: code map = graphify; code symbols and edits = Serena; memory = claude-mem; library docs =
  Context7; big outputs = Headroom; output compaction = rtk (Bash only, one PreToolUse rewriter). Don't use a second
  tool for a job that has an owner here.
- The bar is check-tools.sh at 0 FAIL and 0 WARN. Any FAIL or WARN at session start: fix the kit before other work,
  log it with task-observer, and tell me what changed.
- Every fix to the setup goes into install-all.sh (and a matching check in check_tools.py) in the shared folder, never
  only into this container, so the next fresh session gets it.
- Adding, upgrading or removing a tool, plugin, hook or MCP server: say so first, then make sure it registers once at
  user scope, doesn't add a second Bash rewriter or a second tool for a job above, and passes check-tools.sh --worker.
  If it clashes, disable the overlapping part (permissions.deny) instead of keeping both.
- Keys only in cloud env vars (SKILLSMP_API_KEY, CONTEXT7_API_KEY, GEMINI_API_KEY). Never print, log, commit or write
  a key value to memory; the check scans for leaks.
- Update the date and counts in /mnt/project-files/toolkit/check-results/ after each full check.

WORKERS (work that fans out)
- Use headless workers only when a task clearly splits into independent parts, and say so first.
- Start one with `bash $KIT_DIR/workers/run-worker.sh <worktree> <brief>` (Bash run_in_background, timeout 7200000).
  A late change goes in its inbox (~/wt/reports/inbox-<name>.md, under a `## <short heading>` line).
  Stop one with TaskStop or `bash $KIT_DIR/workers/stop-worker.sh <name>`, never `pkill -f <pattern>`.
- Every brief starts from $KIT_DIR/workers/brief-common-template.md.
- Before merging a worker branch: `python3 $KIT_DIR/workers/audit.py <worktree>` (MISSING must say none) and a
  ponytail review of its diff.

GAME DATA (native porting only)
- The source game is installed on my PC at C:\Users\Admin\Desktop\sonic adventure 2 batt. That folder is the only
  source of truth for game data. Read it through the linked computer's folder access (or a Remote Control session
  there); never copy it into a repo or the shared folder, and never commit anything from it.
- Only port natively: every model, motion, texture, effect, sound, stat, formula, table, timing and string must be
  read or converted from the real files in that folder, with the source file and offset/entry recorded next to the
  result.
- Never invent, guess, approximate or fill in game data. No placeholder values, no "typical" numbers, no data
  remembered from wikis or other games, no handmade stand-ins. If a value can't be found in the files, stop, say
  exactly what is missing and where you looked, and leave it unimplemented.
- No non-native game data or engines unless I explicitly say so for that piece: no substitute engine, physics,
  animation system, shader or gameplay logic standing in for the game's own. Port the game's own behaviour from its
  files/code; a different engine or invented system needs my named go first.
- Never fake data in tests either: fixtures come from the real files (kept local) or are clearly synthetic test
  inputs that never ship as game data.
- Don't assume: when a format, field meaning or behaviour isn't proven from the files or a decompile/disassembly of
  them, mark it "unverified" and ask me before building on it.

RULES
- Verify before saying something is NOT in the source: find the exact file it would be in, check it, and search for
  any words I quote. Until then call the claim "unchecked".
- Say "tested offline" or "tested live" plainly; never call something fixed without the check that shows it. A mock-only
  test is not integration (AGENTS.md).
- When a tool is skipped, clashes or misfires: fix the setup, log it with task-observer, and propose the rule change.
  Rules for keys, security and deploys need my named approval.
```
