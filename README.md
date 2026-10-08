# Napoleon Total War Rust Rewrite

Napoleon Total War Rust Rewrite is a from-scratch remake of **Napoleon: Total War** (2010), written in Rust with the Bevy game engine.

The aim is a complete, faithful 1:1 remake. That means the same campaign map and battles, the same rules and formulas, the same screens and menus, and the same behaviour as the original game. It also means a codebase that is open, modern and easy to mod. The one deliberate difference: the original engine's hard limits are gone. These include the caps on factions, regions, religions, cultures, units per army, unit sizes and battle sizes. The defaults still match the original, but nothing in the code caps them.

> **You need your own copy on steam of Napoleon: Total War to play.** Napoleon Total War Rust Rewrite ships no game files at all. Every model, texture, sound, map and line of text is read at run time from the original game you already own, and that folder is never modified.

> Napoleon Total War Rust Rewrite is an unofficial fan project. It is not affiliated with, endorsed by or connected to Creative Assembly or SEGA. *Napoleon: Total War* and *Total War* are trademarks of their respective owners.

---

## Project status

**Early and in active development. Not playable as a full game yet.**

What works today:

- **Main menu.** It is rebuilt from the original game's own menu layouts, fonts, art and UI scripts. You can browse campaigns, historical battles, the Load Game page and Options.
- **Campaign map.** It shows terrain, borders, rivers, roads, settlements and armies. Turns can be ended, and the AI factions take their turns. Armies move with the original pathfinding rules. Taxes, trade, research, construction, recruitment, diplomacy and agents run on the original formulas, and much of this has been checked against the original game.
- **Settlement and campaign screens.** These include the settlement panel with building and recruitment, the building tree, the capture screen, and the government, technology, objectives and diplomacy lists.
- **3D land battles.** Battles play on the original battle maps with original buildings and trees. Infantry and cavalry are fully animated, with shooting, melee, morale, fatigue, charges and the battle AI. About 175 frames per second with 2,000 soldiers on screen.
- **Saving and loading** of campaigns, in the original save format.
- **Original intro movies and music.** The game's Bink videos are played by our own decoder.

Still to come, among other things: naval battles, the full battle interface, many campaign screens, multiplayer, mod loading and final graphics. The detailed list lives in [`docs/BACKLOG.md`](docs/BACKLOG.md).

## Goals

1. **Complete.** The whole game: menus, the campaign, land and sea battles, multiplayer, saves, settings, the advisor, video, audio and UI scripting.
2. **Faithful.** The same formulas, constants, random numbers, turn order and data as the original. Screens are built from the original layout files, not redrawn by hand. Anything not yet matched to the original is clearly marked `PROVISIONAL` or `PLACEHOLDER` in the code until it is.
3. **Moddable.**
   - Mods made for the original game (`.pack` files and `user.script.txt` mod lines) should load and behave the same way.
   - A content folder will let you add or override data, text, textures, models, scripts and maps, with a clear load order.
   - Common open formats will be accepted alongside the original ones, such as glTF models and higher-resolution textures.
4. **Multiplayer** between copies of Napoleon Total War Rust Rewrite, using its own networking. It will not connect to the original game's multiplayer.

Napoleon Total War Rust Rewrite has no Steam integration: no achievements, overlay or Steam API.

## How it is built

The original game's behaviour is studied, written down in plain-language notes, and then implemented as new Rust code. File formats are documented from the game's own files. Rules and formulas are documented by analysing how the original game works, then tested against data and saves produced by the original. The project contains no code or assets from the original game.

Every finding in the notes is tagged by confidence:

| Tag | Meaning |
|---|---|
| `CONFIRMED` | Verified against the original game |
| `INFERRED` | Strongly suggested by the evidence, not fully proven |
| `UNKNOWN` | Not yet worked out |

## Requirements
| `--battle-map <name>` | Uses a specific battle map. Use `list` to print the names. |
| `--battle-key <key>` | Opens one of the historical battles, for example `NHB_Arcole`. |
| `--view-model <key>` | Opens the 3D model viewer for a model or unit. |
| `--no-intro` | Skips the intro movies. |
| `--screenshot <file.png>` | Saves a screenshot and exits, for automated checks. |

Pass them after `--`, for example:

```bash
cargo run -p napoleon -- --campaign eur_napoleon --campaign-faction france
```

### Running the tests

```bash
cargo test --workspace
```

Some slower tests read the full game install and are skipped by default. Run them with `-- --ignored`.

## Repository layout

The game is split into several Rust crates, each with one job:

| Crate | Purpose |
|---|---|
| [`ntw_formats`](crates/ntw_formats) | Read-only readers for the original file formats: `.pack` archives, database tables, text, models, textures, animations, maps, UI layouts, fonts and more |
| [`ntw_data`](crates/ntw_data) | Typed game records (units, factions, buildings and so on) built from the database tables |
| [`ntw_sim`](crates/ntw_sim) | The deterministic game simulation for battles and the campaign. It has no graphics code and is fully unit-tested |
| [`ntw_campaign`](crates/ntw_campaign) | Builds a campaign from the original start positions and save files, and writes saves |
| [`ntw_script`](crates/ntw_script) | The Lua 5.1 scripting layer: campaign events, the game interface for scripts, and the UI scripts |
| [`ntw_ai`](crates/ntw_ai) | The battle AI and the campaign AI |
| [`napoleon`](crates/napoleon) | The Bevy program you actually run: rendering, input, audio, video and screens |

Other folders:

- [`analysis/`](analysis) holds the research notes: file formats, game rules, open questions, and standalone research tools.
- [`docs/`](docs) holds the design document, the architecture report, the backlog, and a checklist for comparing Napoleon Total War Rust Rewrite against the original game.

## Contributing

The project is not set up for outside contributions yet. Issues and suggestions are welcome.

If you do contribute, please follow these rules:

- **Never commit files from the original game,** including extracted files, models, textures, sounds or text.
- **Never paste decompiled code.** Describe the behaviour in the notes and write new code from that description.
- Tag findings `CONFIRMED`, `INFERRED` or `UNKNOWN`, and mark any stand-in as `PROVISIONAL` or `PLACEHOLDER`.
- Keep `cargo test --workspace` passing, and add no new `cargo clippy` warnings.

## License

The source code is dual-licensed under the MIT License ([LICENSE-MIT](LICENSE-MIT)) or the Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), at your option, as declared in `Cargo.toml`.

This license covers Napoleon Total War Rust Rewrite's own code and notes only. It grants no rights to Napoleon: Total War or any of its content.

