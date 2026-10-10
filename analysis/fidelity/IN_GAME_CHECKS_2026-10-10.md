# In-game checks, 2026-10-10

The HANDOFF "Needs an in-game check" list, run on main 175600d5 (release build, harness window
1280x720 campaign / 1280x960 battle). Screenshots are in `%USERPROFILE%\Documents\ntw-evidence\screens\ours\`,
named `2026-10-10_<topic>.png`; originals in `ntw-evidence\screens\`. One line per check: topic, verdict, evidence or reason.

- **New-character portraits:** USER. Test `a_generated_general_shows_his_drawn_portrait` passes. The harness can't hire from the pool or field-promote a colonel: the enlist panel's confirm button id is unknown and the rows are `commander_<id>`.
- **Army recruitment tab:** FAIL. See `army_recruitment_tab.png` vs `2026-10-07_original_wellesley_army_recruitment_tab.png`.
  - The original lists 4 land options. Ours lists ~25 overlapping cards, including ships (missing icons `trade_ship_dhow`, `frigate_*`, `small_sloop`).
  - Likely site: `crates/ntw_sim/src/campaign/commander_recruitment.rs:139-148`. Only a navy filters out non-naval units; an army never filters out naval ones.
  - Also the title reads "Arthur Wellesley, Great Britain" where the original has "Arthur Wellesley".
  - Also the `ui\units\icons\cav_light_hussars.tga` icon is missing.
- **Region labels:** PASS. Tests `settlement_labels_draw_under_the_diplomatic_relations_panel` and `region_details_panel_is_filled_from_the_campaign`. The Diplomatic Relations panel matches the original (`diplomacy_labels_britain.png` vs `2026-10-08_original_diplomatic_relations_britain.png`).
  - Tooling note: harness screenshots show no settlement labels at all (`labels_wait.png`). An interactive capture shows them (`labels_interactive.png`), so the harness view never feeds `campaign_set_view` before the shot (INFERRED).
- **Battle HUD layout:** USER. The harness window is fixed, so resizing from 1920x1080 to 1280x720 and clicking at both sizes needs a person. The 1280x960 reference is `battle_hud_1280x960.png` (laid out, no overlap).
- **UI side-by-side (British campaign):** PASS.
  - (1) `technology.png`: no line across the title.
  - (2) `wellesley_army_tab.png`: Army | Recruitment tabs, portrait first. The line's "Recruitment empty PLACEHOLDER" note is stale.
  - (3) `lists.png`: generals show portraits, colonels show unit cards, and the panel sits top-right. 1920 docking is covered by the campaign_ui test.
- **Attribute icons:** USER. There is no original screenshot of an agent card or the agents-tab rows. Ours: `settlement_agents_tab.png`.
- **Settlement panel:** FAIL. Hovering or clicking a construction slot (`Building1`) shows an empty dark tooltip frame (about 430x290, no name, text or pips).
  - It stays on screen after the pointer leaves and after switching tabs. Evidence: `settlement_building_selected.png` and `settlement_recruitment.png`, top-left (the harness pointer is at 0,0).
  - Likely site: the building tooltip `crates/ntw_script/src/ui/campaign/settlement.rs:726-812` (frame tooltip → `TechTreeItem_Tooltip` `InitialiseBuilding`/`BuildingDetails`).
  - Passing by test: demolish (`demolish_button_removes_a_standing_building`), fort (`fort_selection_opens_the_map_fort_panel`) and agents tab (`agents_panel_opens_without_script_errors`; 3 Paris agents in `settlement_agents_tab.png`).
  - Recruitment prices: USER (`settlement_recruitment.png`; the cards overlap slightly, and there is no original screenshot).
- **Agent actions:** USER. Test `an_agent_action_button_opens_the_target_picker` passes. The success % needs staged targets compared with the original.
- **Fog labels:** USER. It needs turns plus camera moves, and harness shots draw no labels (see Region labels).
- **Fort selection:** PASS. `fort_selection.png`: the fort panel opens with a Construction tab, the fort and its upgrade. Test `fort_selection_opens_the_map_fort_panel` passes.
- **Promote panel:** PASS. `promote_panel.png`: the panel is centred (x 408-872 in 1280) and its bottom touches the HUD band.
- **Enlist panel traits (seen in `promote_panel.png`):** FAIL, a known PROVISIONAL. Trait names show raw keys (`C_General_Good_Field_Commander`, `NTW_General_Grand_A...`) and the second column overlaps the first. Site: `crates/ntw_script/src/ui/campaign/army.rs:680-688` (`Name` = key, `IconFilename` empty).
- **Sea battles:** USER. Compare the water, shore and foam side by side; ours are `sea_nile.png`, `sea_toulon.png` and `sea_nile_foam.png`.
  - Naval battles still use land test armies (PLACEHOLDER).
  - `nile_battle.battle_script:38` raises: `Friendly_Ship_Orient_Controller` is nil (naval script API missing).
- **Experience:** USER. It needs veteran and fresh units watched in a battle against the original.
- **Battle effects:** PASS for the measurable claims, USER for the rest.
  - `NAPOLEON_FX_CHECK=all` on NHB_Austerlitz: 0 of 15 claims FAIL (12-pdr vs 6-pdr reports, canister group, shrapnel smaller than shell, scorch sprites upright).
  - `NAPOLEON_FX_LOG=50000`: 0 `FX: no group` lines in 50k particles.
  - USER: shell scorch colour (orange) and grenade/carcass hits leaving no scorch, side by side. The frame is `fx_austerlitz.png` (the volley is off-camera).
- **Battle unit card error (seen in the Austerlitz runs):** FAIL. `template.BattleUnitCard.lua:163` `UpdateStateIcon`: "attempt to compare number with nil".
  - It is raised from `__ntw_battle_update_card` (`crates/ntw_script/src/ui/battle_prelude.lua:425-435`) on NHB_Austerlitz with `--skip-deployment`. It does not occur on `--battle`.
  - A card-info field the state icon reads is nil. Likely in `fill_card_info` / `__ntw_battle_card_info`.
- **Flags:** PASS for the test (`flag_install -- --ignored`, 5 passed). USER: Spanish and rebel units fly their own flags, against the original.
- **Before any debugger session:** PASS. `probe_script` passed 6 and `probe_install -- --ignored` passed 3. This is a standing procedure, not a check.
