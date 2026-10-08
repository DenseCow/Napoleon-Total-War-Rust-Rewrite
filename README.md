# Napoleon Total War Rust Rewrite

A remake of **Napoleon: Total War** (2010), written in Rust with the Bevy game engine.

The aim is a complete, faithful 1:1 remake: the same campaign map and battles, the same rules and formulas, the same screens and the same behaviour as the original, in an open codebase that is easy to mod. The one deliberate difference is that the original engine's hard limits are gone (factions, regions, religions, units per army, unit and battle sizes). The defaults still match the original.

> **You need your own copy of Napoleon: Total War from Steam.** This project ships no game files. Every model, texture, sound, map and line of text is read at run time from your install, and that folder is never modified.

> This is an unofficial fan project, not affiliated with or endorsed by Creative Assembly or SEGA. *Napoleon: Total War* and *Total War* are trademarks of their respective owners.

## Contributing

Contributions are welcome. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before you start. In short:

- **Set up [ghidra-mcp](https://github.com/bethington/ghidra-mcp).** The original game's behaviour is worked out from its executable in Ghidra, and ghidra-mcp lets you, and AI assistants, read it.
- **If you use an AI assistant, have it follow the project workflow:** the rules in [`CLAUDE.md`](CLAUDE.md) and the ghidra-mcp workflow guides in that repository's `docs/prompts/`.
- **Never commit files from the original game or decompiled code.** Describe the behaviour in the notes, then write new Rust from that description.

## Project status

**Early and in active development. Not playable as a full game yet.**

What works today:

- **Main menu**, built from the original's own layouts, fonts, art and UI scripts: campaigns, historical battles, Load Game and Options, after the original intro movies.
- **Campaign map** with terrain, borders, rivers, roads, settlements, armies and fleets. Turns end and the AI factions play theirs. Armies and fleets move by the original pathfinding, including ports, landings and zones of control. Taxes, trade, research, construction, recruitment, diplomacy and agents run on the original formulas, many of them checked against saves from the original.
- **Campaign screens**: the settlement panel with building and recruitment, the building tree, the capture screen, the army panel, and the government, technology, objectives, diplomacy and Lists screens.
- **3D land battles** on the original battle maps, with their buildings and trees. Infantry and cavalry are fully animated, with shooting, melee, morale, fatigue, charges, routing and the battle AI, at about 175 frames per second with 2,000 soldiers on screen. The historical battles run with their original scripts.
- **Saves**: campaigns save and load, and every save made by the original game can be read.
- **Video and music**: the game's Bink movies play through our own decoder, and the original music plays in battle.

Not there yet: campaign battles started from the map, naval battles, siege battles, the full battle interface, several campaign screens, final graphics and effects, multiplayer and mod loading. Everything left is listed in [`docs/BACKLOG.md`](docs/BACKLOG.md), with a progress table at the top.

## Goals

1. **Complete.** The whole game: menus, the campaign, land and sea battles, multiplayer, saves, settings, the advisor, video, audio and UI scripting.
2. **Faithful.** The same formulas, constants, random numbers, turn order and data as the original. Screens are built from the original layout files, never redrawn by hand.
3. **Moddable.** Mods for the original (`.pack` files and `user.script.txt` mod lines) will load the same way. A content folder will let you add or override data, text, textures, models, scripts and maps, and open formats such as glTF models and higher-resolution textures will work alongside the original ones.
4. **Its own multiplayer.** It does not connect to the original game's multiplayer, and there is no Steam integration.

## How it is built

The original game's behaviour is studied, written down in plain-language notes, and then implemented as new Rust code. File formats are worked out from the game's own files; rules and formulas from its executable, then tested against data and saves from the original. The project contains no code or assets from the original game.

Every finding and every piece of code is tagged by how certain it is:

| Tag | Meaning |
|---|---|
| `CONFIRMED` | Verified against the original game |
| `INFERRED` | Suggested by the evidence, not yet proven |
| `UNKNOWN` | Not yet worked out |
| `PROVISIONAL` / `PLACEHOLDER` | Code standing in until the original's behaviour is matched |

An item counts as done only when none of its code carries an `INFERRED`, `PROVISIONAL` or `PLACEHOLDER` tag.

## Requirements

- **Windows.**
- **Napoleon: Total War** from Steam. It is read from `C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War`; set `NAPOLEON_INSTALL_DIR` if yours is somewhere else.
- **Rust** stable, 1.95 or newer, installed through [rustup](https://rustup.rs).

To contribute you also need the reverse-engineering setup (step by step in [`CONTRIBUTING.md`](CONTRIBUTING.md)):

- **[Ghidra](https://github.com/NationalSecurityAgency/ghidra/releases) 12.1.4.**
- **[ghidra-mcp](https://github.com/bethington/ghidra-mcp)** (its `dev` branch), which needs:
  - **Java 21** (a JDK, with `JAVA_HOME` set),
  - **Apache Maven 3.9 or newer**,
  - **Python 3.10 or newer** with [uv](https://docs.astral.sh/uv/). Python is only for ghidra-mcp's own tools; this repository has none.
- **Git**, and an AI assistant that supports MCP (such as Claude Code) if you use one.

## Building and running

```bash
cargo run -p napoleon
```

This plays the intro movies and opens the main menu. Add `--release` for full speed; debug builds stay playable.

### Command-line options

These are for testing and jump past the menus. Pass them after `--`:

```bash
cargo run -p napoleon -- --campaign eur_napoleon --campaign-faction france
```

| Option | What it does |
|---|---|
| `--campaign <name>` | Opens a campaign map directly, for example `eur_napoleon`. |
| `--campaign-faction <key>` | The faction to play in that campaign, for example `france`. |
| `--battle` | Opens a test battle directly. |
| `--battle-map <name>` | Uses a specific battle map. Use `list` to print the names. |
| `--battle-key <key>` | Opens one of the historical battles, for example `NHB_Arcole`. |
| `--view-model <key>` | Opens the 3D model viewer for a model or unit. |
| `--no-intro` | Skips the intro movies. |
| `--screenshot <file.png>` | Saves a screenshot and exits, for automated checks. |

### Running the tests

```bash
cargo test --workspace
```

Tests that read the game install skip themselves when it isn't found. The slowest ones are ignored by default; run them with `cargo test --workspace -- --ignored`.

## Repository layout

| Crate | Purpose |
|---|---|
| [`ntw_formats`](crates/ntw_formats) | Readers for the original file formats: `.pack` archives, database tables, text, models, textures, animations, maps, UI layouts, fonts, video and more |
| [`ntw_data`](crates/ntw_data) | Typed game records (units, factions, buildings and so on) built from the database tables |
| [`ntw_sim`](crates/ntw_sim) | The deterministic simulation for battles and the campaign, with no graphics code |
| [`ntw_campaign`](crates/ntw_campaign) | Builds a campaign from the original start positions and saves, and writes saves |
| [`ntw_script`](crates/ntw_script) | The Lua 5.1 layer: campaign and battle scripts, and the UI scripts |
| [`ntw_ai`](crates/ntw_ai) | The battle AI and the campaign AI |
| [`napoleon`](crates/napoleon) | The program you run: rendering, input, audio, video and screens |

Other folders:

- [`analysis/`](analysis): the research notes, one folder per system, with what is known about the original and how certain it is.
- [`docs/`](docs): the backlog, the design and architecture documents, and a checklist for comparing against the original.
- [`tools/`](tools): helper scripts, such as counting the remaining tags and screenshotting the original for side-by-side checks.

## License

The source code is dual-licensed under the MIT License ([LICENSE-MIT](LICENSE-MIT)) or the Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), at your option.

This covers the project's own code and notes only. It grants no rights to Napoleon: Total War or any of its content.
