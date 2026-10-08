# Contributing to Napoleon Total War Rust Rewrite

Thank you for helping. This project rebuilds Napoleon: Total War 1:1, so most work is finding out
exactly what the original game does and writing new Rust code that does the same. This guide covers
the setup you need and the rules every change follows.

## 1. Set up

1. **Windows and your own copy of Napoleon: Total War** from Steam. The game is read from
   `C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War`; set `NAPOLEON_INSTALL_DIR` if
   yours is somewhere else. That folder is only ever read, never changed.
2. **Rust** (stable, 1.95 or newer, through [rustup](https://rustup.rs)). Check the build with
   `cargo test --workspace`.
3. **ghidra-mcp**, for studying the original executable. Install
   [bethington/ghidra-mcp](https://github.com/bethington/ghidra-mcp) by following its README. It
   runs inside [Ghidra](https://ghidra-sre.org) and lets you, and AI assistants, read the original
   `Napoleon.exe` through Ghidra.
   - Import `Napoleon.exe` from **your own** install into a Ghidra project and let the auto-analysis
     finish (about 47,000 functions). Keep the project outside this repository.
   - Start Ghidra with the ghidra-mcp plugin enabled; its server listens on `http://127.0.0.1:8089`.
   - The repository's [`.mcp.json`](.mcp.json) starts the ghidra-mcp bridge for Claude Code. It
     expects ghidra-mcp in `C:\ghidra-mcp-setup\ghidra-mcp`. Install it there, or change the path in
     your local copy without committing it.

## 2. Pick something to work on

- [`docs/BACKLOG.md`](docs/BACKLOG.md) lists everything left to do, by section. Open an issue first
  to say what you are taking, so two people don't do the same work.
- Read the notes for that area in [`analysis/`](analysis) before changing code. The files are large:
  search for the section you need.

## 3. If you use an AI assistant

Have it follow the project workflow, not its own:

- **[`CLAUDE.md`](CLAUDE.md) is the rule book.** Claude Code loads it automatically; point any other
  assistant at it before it starts. Its "Goal", "Hard rules", "Code quality", "Evidence tags" and
  "Done means" sections apply to every change.
- **Use the ghidra-mcp workflow guides** for all work on the executable. They are in the ghidra-mcp
  repository under `docs/prompts/`: read `TOOL_USAGE_GUIDE.md` first, then the guide for the task
  (`FUNCTION_DOC_WORKFLOW_V5.md` for functions, `STRING_LABELING_CONVENTION.md` for strings,
  `DATA_TYPE_INVESTIGATION_QUICK.md` for structures, and the others listed in `CLAUDE.md`).
- **Trace, don't guess.** Behaviour a change depends on is traced in Ghidra first. A guess is tagged
  `INFERRED` and never presented as fact.
- **Review AI output before you open a pull request.** You are responsible for what you submit.
- The "Agent workflow" and usage-budget parts of `CLAUDE.md` describe the maintainer's own sessions
  (who may push to `main`, worker limits). As a contributor, work on a branch in your fork instead.

## 4. Rules for every change

- **Never commit files from the original game,** including extracted files, models, textures, sounds
  or text. Everything is read from the player's install at run time.
- **Never paste decompiled code** into source or notes, and never commit Ghidra projects or raw
  Ghidra output. Describe the behaviour in plain words in the notes, then write new Rust from that.
- **Match the original 1:1:** the same formulas, constants, random numbers, turn order and data.
  Screens come from the original layout files, not drawn by hand. The one exception is the
  original engine's hard limits (factions, regions, army and battle sizes...), which are removed;
  the defaults still match the original.
- **Tag findings** `CONFIRMED`, `INFERRED` or `UNKNOWN`, and mark stand-ins `PROVISIONAL` or
  `PLACEHOLDER`. Removing a tag needs its evidence cited: an exe address, a debugger result, a data
  file, a test or an in-game check.
- **Write findings into the analysis notes** for that area, so the next person doesn't redo them.
- **Keep `cargo test --workspace` passing** and add no new `cargo clippy` warnings. Add a test for
  each bug you fix.
- **Don't run `cargo fmt`** (there is no rustfmt config, so it rewrites whole crates), and don't
  rewrite files with Windows PowerShell's `Get-Content | Set-Content` (it breaks UTF-8).
- No Python in this repository.
- **No personal information** in files or commits: no real names, emails or Windows usernames.
  Write paths as `%USERPROFILE%` in docs and `$env:USERPROFILE` in PowerShell.

## 5. Open a pull request

1. Fork the repository and make a branch for your change.
2. Run `cargo test --workspace` and `cargo clippy --workspace`.
3. Open a pull request that says what changed, how you checked it (tests, Ghidra addresses, in-game
   comparison) and which tags changed.

By contributing, you agree that your contribution is licensed under the MIT License or the Apache
License 2.0, at the user's option, like the rest of the project.
