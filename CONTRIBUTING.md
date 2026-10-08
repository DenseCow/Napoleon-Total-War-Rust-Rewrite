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
3. **Git.**

## 2. Set up Ghidra and ghidra-mcp

The original game's rules are worked out from its executable in [Ghidra](https://github.com/NationalSecurityAgency/ghidra),
and [ghidra-mcp](https://github.com/bethington/ghidra-mcp) lets you, and AI assistants, read it
there. Its README is the reference if a step below fails.

1. **Install the prerequisites:**
   - **Java 21** (a JDK such as OpenJDK), with the `JAVA_HOME` user environment variable pointing at it.
   - **Apache Maven 3.9 or newer**, on your `PATH`.
   - **Python 3.10 or newer** and [uv](https://docs.astral.sh/uv/). They are only for ghidra-mcp's
     own tools; this repository has no Python.
   - **Ghidra 12.1.4**: unzip the release anywhere.
2. **Get ghidra-mcp** where this repository's [`.mcp.json`](.mcp.json) expects it, on the `dev`
   branch the project uses:
   ```text
   git clone -b dev https://github.com/bethington/ghidra-mcp.git C:\ghidra-mcp-setup\ghidra-mcp
   ```
   (Elsewhere works too: then change the paths in your local `.mcp.json` and don't commit that.)
3. **Build and install it into Ghidra**, from that folder:
   ```text
   python -m tools.setup preflight      --ghidra-path <your Ghidra folder>
   python -m tools.setup ensure-prereqs --ghidra-path <your Ghidra folder>
   python -m tools.setup build
   python -m tools.setup deploy         --ghidra-path <your Ghidra folder>
   uv sync
   ```
   `uv sync` creates `.venv\Scripts\bridge-mcp-ghidra.exe`, the bridge that `.mcp.json` starts.
4. **Allow scripts:** set the user environment variable `GHIDRA_MCP_ALLOW_SCRIPTS=1`. The project's
   workflows run Ghidra scripts through ghidra-mcp. It lets connected tools run code inside
   Ghidra, and the server listens only on your own machine (`127.0.0.1`).
5. **Load the game's executable:** start Ghidra, create a project **outside this repository**,
   import `Napoleon.exe` from your own install, and let the auto-analysis finish (about 47,000
   functions). In the CodeBrowser, enable the plugin under **File > Configure > Utility > Configure >
   GhidraMCPPlugin** if `deploy` didn't.
6. **Check it:** `curl http://127.0.0.1:8089/check_connection` answers `"status": "ok"`. In Claude
   Code, approve the `ghidra-mcp` server from `.mcp.json` when asked (or under `/mcp`).

Optional:
- **Debugger sessions** (watching the original run) use Ghidra's own debugger with Windows'
  `dbgeng`. The exact launch settings are in the ghidra-mcp section of [`CLAUDE.md`](CLAUDE.md).
- `.py` Ghidra scripts need the Jython extension (**File > Install Extensions**); prefer the `.java`
  scripts, which work as they are.

## 3. Pick something to work on

- [`docs/BACKLOG.md`](docs/BACKLOG.md) lists everything left to do, by section. Open a "Claim a
  BACKLOG item" issue first, so two people don't do the same work.
- Read the notes for that area in [`analysis/`](analysis) before changing code. The files are large:
  search for the section you need.

## 4. If you use an AI assistant

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
- `CLAUDE.md` starts with an "Outside contributors" section saying which of its parts apply to you;
  the rest (agent workflow, usage budget) describes the maintainer's own sessions. Assistants other
  than Claude Code are pointed there by [`AGENTS.md`](AGENTS.md).

## 5. Rules for every change

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

## 6. Open a pull request

1. Fork the repository and make a branch for your change.
2. Run `cargo test --workspace` and `cargo clippy --workspace --all-targets`.
3. Open a pull request against `main` and fill in its template: what changed, how you checked it
   (tests, Ghidra addresses, in-game comparison) and which tags changed.
4. CI builds and tests it on Windows (a maintainer approves the first run for new contributors).
   The maintainer reviews it and squash-merges it.

Security problems go through private reporting, not issues: see [`SECURITY.md`](SECURITY.md).

By contributing, you agree that your contribution is licensed under the MIT License or the Apache
License 2.0, at the user's option, like the rest of the project.
