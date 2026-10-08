# Campaign gameplay: turn loop, movement, provinces, battles, saves

Worker: campaign-play (branch `work/campaign-play`). Tags: CONFIRMED / INFERRED / UNKNOWN;
stand-ins PLACEHOLDER / PROVISIONAL.

## Where I am / what's next
- **Done (model, tested):** step-wise turn loop in the startpos faction order, economy from DB
  values, recruitment and construction commands, A* movement on a grid built from `regions.esf` +
  road splines, attack / merge / enter-settlement intents, autoresolve via `ntw_sim` autoresolve,
  save writer (patches the source ESF tree) with a round-trip test, Lua host stepping (test with
  the real eur startpos and scripts: no script errors over 4 turns).
- **Done (Bevy):** markers follow the model, selection, right-click orders with a path preview
  (green = this turn, red = later turns), walk animation, the original HUD
  (`ui\campaign ui\layout` run by `UiScriptHost`: funds, date, season icon, End Turn button calling
  `CampaignUI.EndTurn()`), F5 save to `%APPDATA%\NapoleonRust\save_games\quick_save.save`,
  loading a save (`--campaign-save <file>` and the front end's Load Game).
- **Next:** HUD script errors (`campaign_hud.lua:98` global `player_details` nil — needs more `CampaignUI.*`;
  `message_handler.lua:64` `stack_base` nil), unit cards / settlement panels / recruitment UI,
  campaign AI, naval battles, the scripts' `save_value` slots in saves, ground types.

## How to run
```text
cargo run -p napoleon -- --campaign eur_napoleon [--campaign-faction austria]
cargo run -p napoleon -- --campaign x --campaign-save "<path to a .save>"     (read-only)
harness: --screenshot shot.png [--campaign-demo] [--campaign-demo-move] [--campaign-end-turn N]
tests:   cargo test -p ntw_campaign --release --test campaign_play -- --nocapture
```
Controls (PROVISIONAL bindings): left click selects; right click (no drag) moves / attacks /
merges / enters a settlement; right or middle drag pans; wheel zooms; Return or the HUD button
ends the turn; F5 saves.

## Evidence found in this round
| finding | tag |
|---|---|
| `LOCOMOTABLE` #8 = the type's base action points = `agents` #1 (General 26, colonel 25, admiral 90, gentleman 29) in the eur startpos and the vanilla saves (round 6) | CONFIRMED |
| `LOCOMOTABLE` #9 = action points left (0..44 for generals in saves, lower after moving) | INFERRED |
| `FACTION_ARRAY` order starts with the player's faction (france in eur); used as turn order | INFERRED |
| `GOVERNMENT` #1 = `government_types` key (`gov_empire` for france) | CONFIRMED |
| `REGION_SLOT` #3 key gives the slot type: `settlement:<r>:<town>:<type>:<n>`, else `campaign_map_slots` / `campaign_map_towns_and_ports` | CONFIRMED |
| `REGION_SLOT_MANAGER/ROAD_SLOT` = {bool, `REGION_SLOT`} holding the `sRoads*` building | CONFIRMED |
| `BUILDING_MANAGER` = {bool has building, [BUILDING], bool has construction, [BUILDING_CONSTRUCTION_ITEM]} | CONFIRMED (saves) |
| `BUILDING_CONSTRUCTION_ITEM` v1 {u32 (1 new / 0 upgrade, INFERRED), bool, u32 turns done, u32 total, u32 cost, key} | INFERRED meanings |
| `RECRUITMENT_ITEM` inner #1 = region id; #3 turns left; #4/#7 cost | #1 CONFIRMED, others INFERRED |
| `REGION` #32 u32[8] grows over turns; used as region GDP | INFERRED |
| `REGION` #9..#15 (e.g. 1500 1600 1600 900 900 900 -3) | UNKNOWN (town wealth / growth?) |
| HUD: the engine calls the global `UpdateFactionFundsAndDate{funds, season, year, round_description}` of `ui/campaign ui/layout.root`; End Turn button calls `CampaignUI.EndTurn()` and polls `CampaignUI.CanEndTurn()` | CONFIRMED (bytecode) |

## PROVISIONAL rules (our own, to be replaced when found)
- Phase order inside a turn (`ntw_sim/src/campaign/turn.rs`), AI does nothing.
- ~~Income~~ RESOLVED (CONFIRMED, analysis/fidelity/CAMPAIGN_FIDELITY.md §Economy): each class pays round(effective rate × (GDP + town wealth)); tax efficiency penalty; faction_gdp_other by the major-power flag; paid at the round end for all factions. Still PROVISIONAL there: character / tech tax bonuses, upkeep modifiers, GDP / town wealth growth recomputation.
- Public order = Σ happy_* + repression_* effects (tax level, government, buildings) + garrison repression and automated policing (CONFIRMED, CAMPAIGN_FIDELITY.md); the other happiness sources are still PROVISIONAL.
- Movement: 1-unit grid built from the original `pathfinding.esf` polygons (land, sea, off-map, road strips; see CAMPAIGN_DATA.md §1), the A* search is ours; off-road cost 1.0 AP/unit, road cells cost `road_level_<n>_action_point_cost`
  with n = region's sRoads level + 1; navies sea cells only; no zones of control; AP refill to base.
- Recruitment time = `units` #6; queue size = `recruitment_points` effects; recruits join a
  commander-less garrison force; full refund on cancel.
- Autoresolve potentials from unit stats, r = r2 = 0, loser stays, defenders of a lost settlement die.

## Questions for Ghidra (frontend/audio worker owns it)
1. Campaign turn phase order (event vtables near 0x01356300). PARTLY RESOLVED: the economy of every faction is settled at the round end (0x00948CF0, CAMPAIGN_FIDELITY.md).
2. Economy: town wealth / GDP / tax formulas (region+0xD8, +0xDC getters), `tax_efficiency_*` use. RESOLVED: taxes, tax efficiency, the REGION fields, and the per-round GDP / town wealth growth recomputation (0x00A6AFC0; all 238 start-position regions reproduced), trade route values (CAMPAIGN_FIDELITY.md).
3. `pathfinding.esf` layout (vertices, u32 face array, `grid_data`) and the campaign path search;
   off-road cost; how `campaign_ground_types` multipliers apply.
4. Where tax levels are stored in the ESF (`FACTION_ECONOMICS` u8[25]?). RESOLVED: `GOVERNORSHIP_TAXES` u8 rates (the exe reads them at +8 / +9); `FACTION_ECONOMICS` holds the 10-turn history of 25 income / expense categories (CAMPAIGN_FIDELITY.md).
5. `units` #6 (`unknown_38`) meaning (recruitment turns?) and the AP refill/bonus rule. RESOLVED: #6 = recruitment turns (CONFIRMED, CAMPAIGN_FIDELITY.md §Public order / recruitment); AP: CAMPAIGN_FIDELITY.md §Action points.
6. Autoresolve side potentials and the retreat rule. Open; partly read (side strength = sum of unit potentials, wipeout threshold), see CAMPAIGN_FIDELITY.md §Autoresolve.
