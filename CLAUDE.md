# NapoleonRust — instructions for Claude

A complete 1:1 remake of Napoleon: Total War in Rust + Bevy, reading all assets at runtime from the
user's own install. This file is the single source of project rules. Keep it short.

## Session start

1. `git pull`, then `git status`.
2. Check for unfinished work left by a session the usage budget stopped: uncommitted changes in
   `git status`, and `git worktree list` for worker worktrees with uncommitted or unmerged work.
   Review it, then finish, merge or discard it before starting anything new.
3. Read `docs/HANDOFF.md` (current state: open bugs, who works on what, pending in-game checks).
4. Read only the `docs/BACKLOG.md` section you are working on (e.g. `## 0.`), not the whole file.
5. Set up ghidra-mcp (below) before any reverse-engineering work.

Do not re-plan the project, restart completed work, or read whole `analysis/` files. They are
large: grep for the section you need, and read that file's "Where I am" / "Next" part first.

## Goal (the rules every change follows)

- **Complete:** frontend, campaign, land and sea battles, multiplayer, save/load, settings, advisor,
  video, audio, UI scripting.
- **Faithful, 1:1:** same formulas, constants, RNG, turn and tick order, data; schemas match field
  for field. Screens are built from the original layout files (`Version039` layouts, `.twui`, `.cuf`,
  UI `.luac`), never redrawn by hand. No placeholder menus or substitute screens (test harnesses like
  `--view-model` are fine). Anything not yet matched is marked `PROVISIONAL` or `PLACEHOLDER` in code.
- **No engine limits (the one exception to 1:1):** no hardcoded content counts (factions, regions,
  religions, cultures, units, buildings, techs, characters...), army/unit/battle size caps, fixed
  tables or small ID types. Defaults and vanilla data still give the original game exactly;
  gameplay caps (20-unit army, unit scale, battle cap) become settings defaulting to the original.
  Code that lifts a limit states the original's limit. Saves must not cap counts either.
- **Middleware recreated as our own code:** SpeedTree, Miles mixing rules, Bink, etc.
- **Assets at runtime, read-only,** from the user's install. Ship no Creative Assembly files.
- **Moddable:** original mods (`.pack`, `user.script.txt` mod lines, loose `data\`) load with the same
  order and overrides; a content folder; open formats (glTF, image heightmaps, 2K/4K textures) as
  drop-in overrides.
- **Own multiplayer;** no Steam integration.

## Hard rules

- The Steam install `C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War` is **read-only**.
- **Never paste decompiled code** into source or docs, and never commit raw Ghidra output
  (`.gitignore` blocks `analysis/**/*.txt`). Describe behaviour, then write new Rust from it.
- No Python in this repo.
- Never run `cargo fmt`: there is no rustfmt config, so it rewrites whole crates (a worker lost its
  changes to a 10k-line diff, 2026-10-07).
- Never rewrite files with Windows PowerShell (`Get-Content | Set-Content`, `Out-File`): it reads
  UTF-8 as ANSI and writes it back double-encoded with a BOM (two branches, 2026-10-07). Edit with
  the Edit/Write tools or bash; `crates/napoleon/tests/encoding.rs` catches it.
- Never write the user's name, email, Windows username or personal paths into the repo (code, docs,
  scripts, commit messages). Paths use `%USERPROFILE%` in docs, `$env:USERPROFILE` in PowerShell and
  the `USERPROFILE` variable in Rust; commits use the checkout's git config, never `-c user.*`.
  The public repo gets snapshots only through `tools/publish_public.sh` (manager, after a merge);
  never add it as a remote or push the private history to it.
- Keep `cargo test --workspace` passing and add no new `cargo clippy` warnings.

## Code quality (optimal, not just working)

- **Simplest correct design first.** Pick the design with the fewest moving parts (threads, channels,
  caches, states) that meets the 1:1 behaviour. If two review rounds keep finding bugs in the same
  design, replace the design instead of patching it.
- **One source of truth.** A game rule lives once (in the model) and the UI, commands and AI call it;
  never re-implement a lookup or rule in a second place.
- **Hot paths stay cheap.** No allocation, file I/O, decoding, locking or freeing of large buffers on
  the audio thread; no per-frame rebuilding of tables, strings or layouts the data didn't change.
  Do heavy work once, off the main thread, and cache it with a clear invalidation rule.
- **No silent failure.** Errors are handled where they occur, logged once (not every frame or play),
  and never retried in a loop; every wait has a timeout.
- **Measure, don't guess.** Performance claims carry a number from a release build (and say if debug
  differs); keep debug builds playable.
- Idiomatic Rust and Bevy (ECS systems, `Arc` for shared immutable data), no new clippy warnings,
  tests for each fixed bug.

## Evidence tags

`CONFIRMED` (verified by source, runtime, test or strong evidence), `INFERRED` (evidence, not
verified), `UNKNOWN`, `BLOCKED`. Never upgrade a tag on assumption. Source of truth, in order: code,
test results, `docs/BACKLOG.md`, analysis notes, git history.

## Done means

Implemented, builds, tests pass, `/code-review medium` run and its blocking findings fixed, checked in
game at its tier, findings recorded in the analysis file, BACKLOG item ticked and its Progress table
updated, committed. Blocking: crashes, wrong or unfaithful behaviour, data loss, silent failures,
hot-path cost, a rule duplicated in a second place, a comment that is now wrong. The rest (clean-ups,
refactors, cold-path allocations) goes to the BACKLOG "Polish" section, one line each, and doesn't
hold the merge. A worker editing a file clears that file's polish lines; past ~20 lines, clearing
the list comes before new features. After the first full review, later rounds review only the fix
commits. Exe behaviour a fix depends on is traced in Ghidra first, not guessed. Compiling, believing it is right, or resembling the original is not done; for fidelity
work, behaviour beats compilation. An item stays INFERRED / PROVISIONAL until its in-game check is
done. **Done means zero tags:** a BACKLOG item is ticked only when its code has no PROVISIONAL,
PLACEHOLDER or INFERRED left, and a section is complete only at zero; the game is finished at zero.
`tools/tag_count.sh` counts them per crate (Progress table). A tag is removed only with its evidence
cited (exe address, debugger result, data file, test or user check); a review blocks a tag removed
without evidence and a guess left untagged. In-game check tiers (batched in HANDOFF "Needs an in-game check", each line with its exact
command; a merge never waits for them):

| Change | Check |
|---|---|
| CONFIRMED from shipped data / real-file test | one quick look in the batch |
| New and visible | one quick look in the batch |
| INFERRED / PROVISIONAL (exe behaviour, hand-picked values, layouts) | side-by-side with the original |
| Logic only, covered by tests | none |

## Agent workflow (Claude sessions)

**Only the local manager pushes to `main`**, and code only after the full Done checklist (review clean,
tests, merge). Every other session, including cloud/web sessions (claude.ai/code) and the sandbox,
commits to its own branch and asks for a merge; it never pushes to `main`, even for a "quick fix".
A PreToolUse hook (`tools/guard_main_push.sh`) refuses such pushes in any session that isn't on the
user's machine. (GitHub can't enforce this: branch protection needs Pro on a private repo.)

**Never change the branch of the main checkout** (`%USERPROFILE%\Documents\NapoleonRust`): it holds
the session's hooks and rules. Reviews and workers read a branch from its worktree or with
`git diff`/`git show`; no checkout, switch, reset or stash there (a review fork did it 2026-10-07).

At most 1 manager + 3 workers. The manager assigns non-overlapping tasks, reviews results, decides what
merges, and updates `docs/HANDOFF.md`. Workers inspect existing work before changing it, use
isolated branches/worktrees when they edit, test, commit, and report what changed and how it was
verified. No open-ended waits: every background wait has a timeout. Stop every process you started
(cargo runs, loops, monitors) before reporting; the manager checks none are left before it reports.
The sandbox's free-model workers follow `.opencode/agents/` instead.

Token discipline (never at the cost of reviews, tests or in-game checks):
- **Worker reports ≤150 words, this shape:** status (done / partial / blocked); commit hash; files
  changed; tests run and result; tag changes (e.g. INFERRED → CONFIRMED); open issues. No narration,
  no pasted code. The manager reads the diff when it needs detail.
- **Briefs point, don't paste:** give file paths, line numbers and section names, not file contents.
- **Filter output:** `cargo test -q` / `cargo build -q`, keeping only `FAILED|panicked|error\[` lines;
  game logs through `Select-String "ERROR|WARN|UNKNOWN"`. When something fails, rerun just that test
  or command with full output before diagnosing.

## ghidra-mcp (Napoleon.exe reverse engineering)

Installed at `C:\ghidra-mcp-setup\ghidra-mcp`; Ghidra at `%USERPROFILE%\OneDrive\Desktop\RE`;
`.mcp.json` starts the bridge. The tools only work while Ghidra runs with Napoleon.exe open. Set it
up yourself, don't ask the user:

1. `curl.exe -s http://127.0.0.1:8089/check_connection`. If it fails, start Ghidra **with scripts
   enabled**: `$env:GHIDRA_MCP_ALLOW_SCRIPTS='1'; Start-Process $env:USERPROFILE\OneDrive\Desktop\RE\ghidraRun.bat`,
   then poll every 10 s for up to 3 minutes.
2. `open_program Napoleon.exe`, then `analysis_status`: `function_count` must be ~47,000. If it is
   far lower, run `reanalyze` (it times out on the call but finishes in Ghidra) and `save_program`.
3. If no ghidra-mcp tools exist at all, the server didn't load: ask the user to approve it in `/mcp`.

Updating (branch `dev`): `exit_ghidra`, `git pull` in the install folder, then
`.venv\Scripts\python.exe -m tools.setup build` and `... deploy --ghidra-path $env:USERPROFILE\OneDrive\Desktop\RE`
(it restarts Ghidra; check `/mcp/health` shows the new build). Its settings are in that folder's
`.env`; Maven, `JAVA_HOME` and `GHIDRA_MCP_ALLOW_SCRIPTS=1` are user environment variables. Then ask
the user to reconnect ghidra-mcp in `/mcp` (the bridge is an editable install).

Use everything ghidra-mcp offers (all of it is allowed, including `run_script_inline` /
`run_ghidra_script` at any time). To save tokens, `.mcp.json` loads only the `listing`, `function`,
`program`, `xref`, `comment` and `symbol` groups at start; every other tool (datatype, analysis,
documentation, debugger, emulation, project, server...) is one `search_tools` or `load_tool_group`
call away, so never assume a tool is missing. The bridge runs with
`GHIDRA_MCP_REQUIRE_PROGRAM_SELECTORS=1`, so every call passes `program="/Napoleon.exe"` (the full
path: debugger traces add more programs named `Napoleon.exe` to the project). Save
after every few edits. **Read in parallel, write one at a time:** any number of workers may read
(decompile, xrefs, search, memory), but only the holder of the writer lock changes the program
(rename, comment, create function, set type, save, analysis). Parallel writes wait for analysis and
looped every MCP thread forever, losing the unsaved edits (2026-10-07). Take the lock with
`mkdir "$LOCALAPPDATA/napoleon-ghidra-writer.lock"` (fails if held; then write your name and time
into `owner.txt` inside it), write one batch, `save_program`, then `rm -r` the lock. Never hold it
while building, testing or coding. A lock older than 15 minutes is stale: take it over. Until you
hold it, keep names and plate comments in your analysis file and apply them in your next batch. Before a bulk pass, back the project up with `archive_project` to
`%USERPROFILE%\Documents\ghidra-backups`.

The guides are in `C:\ghidra-mcp-setup\ghidra-mcp\docs\prompts\`. Read `TOOL_USAGE_GUIDE.md` first,
then the one for the task:
- Every exe function you work out: `FUNCTION_DOC_WORKFLOW_V5.md`, finished with
  `analyze_function_completeness`. For many functions, `FUNCTION_DOC_WORKFLOW_V5_BATCH.md`.
- Strings: `STRING_LABELING_CONVENTION.md`. Structs and tables: `DATA_TYPE_INVESTIGATION_QUICK.md`,
  `GLOBAL_DATA_ANALYSIS_WORKFLOW.md`, `DATA_SECTION_WORKFLOW.md`. Missed code:
  `ORPHANED_CODE_DISCOVERY_WORKFLOW.md`.
- Naming: `docs\HUNGARIAN_NOTATION.md`, `docs\NAMING_CONVENTIONS.md`, `docs\THIS_POINTER_TYPING.md`.
- Plate comments: `docs\PLATE_COMMENT_BEST_PRACTICES.md`, `PLATE_COMMENT_EXAMPLES.md`. Similar functions:
  `docs\WORKFLOW_DOCUMENTATION_PROPAGATION.md`. Struct size changes: `docs\STRUCT_RESIZE_WORKFLOW.md`.

Every worker brief that may touch the exe lists all of these, and the worker's report says which it applied.

The 95 scripts in `C:\ghidra-mcp-setup\ghidra-mcp\ghidra_scripts\` run with `run_ghidra_script` by
absolute path. The `.py` ones need Ghidra's Jython extension, so prefer the `.java` versions.
Dynamic checks: `emulate_function`, `analyze_dataflow`. Live debugging uses Ghidra's own debugger
(`debugger_launch_offers`), which needs Window > Debugger opened once in the CodeBrowser. Launch the
original with `debugger_launch`, offer `dbgeng extra options` with `cwd` = the install folder
(`dbgeng (.bat)` has no working directory, so the game crashes on the loading screen at
`0x0118757E` when a shader fails to load; plain `dbgeng` fails),
`python_executable=C:\ghidra-mcp-setup\dbg-venv\Scripts\python.exe` (a venv holding Ghidra's bundled
wheels; never pip-install them globally), and `windbg_dir=C:\Windows\System32`. At the first loader
stop, make exceptions second-chance only: `run_script_inline` finds the Debugger tool's
`DebuggerTraceManagerService` target and runs `ghidradbg.util.dbg.cmd('sxd av')` (also `ld ud ct et
eh ch dz gp ii sov`, `sxi out`) through `Target.execute`. It stops twice at the loader: resume both times.
The debugger holds the game open after it quits (Task Manager can't end it): end the session by
stopping the two `python.exe ... local-dbgeng.py` processes, then the game if it is still there.
Night sessions may run the debugger alone, only when no `Napoleon.exe` is running, and only for
what needs no player input (start-up, loading, the main menu, tables in memory). Anything that needs
clicks in the original goes in `docs/FOR_USER.md` as a debugger sitting. The module is `napoleon.retail.exe`, rebased, so translate addresses with
`debugger_static_to_dynamic`. Never attach a raw dbgeng/pybag script to the user's running game: a
failed detach ends the process and loses their progress (it happened 2026-10-07). Launch under Ghidra
instead, and set breakpoints only right before the action you're watching. The
WinDbg proxy on port 8099 isn't part of this setup. `.luac` scripts still go through `luac_dump`.
Findings go into the analysis notes as described behaviour. Ghidra holds the names and comments.

## Where things are

- `docs/HANDOFF.md`: current state. Replace stale content instead of appending logs.
- Usage budget (`tools/usage_budget.ps1`, run by hooks). When the user says to switch to night or
  day mode, run `powershell -NoProfile -ExecutionPolicy Bypass -File tools/usage_budget.ps1 mode night`
  (or `day`) and follow what it prints.
- `docs/FOR_USER.md`: the night report: what night sessions need from the user, and what they finished
  ("Done tonight"), one line each. When the user has handled a line, record the result where it
  belongs (HANDOFF, BACKLOG, notes) and delete the line; clear a night's done list once they've read it.
- `docs/BACKLOG.md`: everything left to do, by section.
- `analysis/<area>/*.md`, `analysis/fidelity/*_FIDELITY.md`: findings per system.
- `docs/DESIGN.md`, `docs/ARCHITECTURE_REPORT.md`: crate design, original engine overview.
- `docs/archive/`: old session logs. Only read them when chasing a specific past finding.
