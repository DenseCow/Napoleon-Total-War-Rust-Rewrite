# Campaign-side DB tables and the .loc format (Worker 3)

**Sources.** `data.pack` holds the tables at `db\<name>_tables\<name>`. `local_en.pack` and `local_en_patch.pack` hold `text\*.loc`. Everything was read only.

**Tool.** My own std-only reader lives in `analysis\worker3\campaign_tools` (`src\db.rs`). It follows Worker 2's header convention. Commands:

| command | what it does |
|---|---|
| `db TABLE SCHEMA [n]` | parse the table and print rows |
| `db-check TABLE SCHEMA` | parse and flag implausible numeric columns |
| `db-col TABLE SCHEMA k` | value histogram of column k |
| `db-hex TABLE [n]` | hex dump of the table |
| `db-tail TABLE PREFIX` | exhaustive search over the fixed-width tail after a known prefix, ranked by value plausibility |
| `loc-info` / `loc-get KEY..` / `loc-find SUBSTR [n]` | .loc summary, lookup and search |

The binary is `target-w3\release\campaign_tools.exe`.

**Schema codes.** These follow Worker 2:

| code | field |
|---|---|
| `s` | UTF-16 string: u16 code-unit count, then UTF-16LE |
| `o` | optional string: u8 flag 0/1, then `s` if the flag is 1 |
| `b` | u8 bool |
| `i` | i32 |
| `f` | f32 |

**What CONFIRMED means here.** A schema is "CONFIRMED (parse)" when it parses **every row and ends exactly at EOF**, and the values are plausible in every column. The file carries no column names, so **names are INFERRED** from loc keys, values, scripts and ESF. An exact-EOF parse can still be ambiguous when a run of zero-valued columns allows 4 x bool versus 1 x i32. Such cases are flagged "zero-run ambiguity". Worker 1's BUILDER code is the arbiter for them.

> **Correction to `worker2\db_schemas.tsv`.** Several of its "ok" schemas parse to EOF but are **misaligned**. Examples: values like 196608, 589824 or 459776 are byte-shifted, and `cultures` is read as i16 pairs. My corrected schemas below were checked by hex and by value plausibility. The affected tables:
>
> | table | worker2 schema | corrected schema |
> |---|---|---|
> | building_levels | | `ssibbiiiiiiiiiiiiiibiiii` |
> | ancillaries | | `sssbbbiii` |
> | character_traits | | `sibis` |
> | government_types | | `sbbiss` |
> | cultures | | `sio` |
> | cultures_subcultures | | `ssis` |
> | slots | `sib` | `sbbbbb` |
> | historical_characters | `sibssiis` | `sbsssiis` |
> | campaign_variables | default inference gives i16 pairs | `sf` |
> | factions | FAIL | decoded below |

---

## 1. .loc format (CONFIRMED by an exact-EOF parse of all 4 files)

```
0x00  u8[2]  FF FE                (UTF-16LE BOM)
0x02  u8[4]  4C 4F 43 00          ("LOC" + NUL, single-byte ASCII)
0x06  u32    version = 1
0x0A  u32    entry_count
0x0E  entry_count x { u16 n; u16[n] key (UTF-16LE); u16 m; u16[m] text (UTF-16LE); u8 bool }
EOF
```

| file | entries | bool=true |
|---|---|---|
| local_en `text\localisation.loc` | 32,582 | 0 |
| local_en `text\ui.loc` | 2,787 | 22 |
| local_en_patch `localisation.loc` | 33,030 | 0 |
| local_en_patch `ui.loc` | 2,820 | 26 |

- The trailing bool's meaning is UNKNOWN. It is only ever true in `ui.loc`; INFERRED to be a "do not translate / tooltip" flag.
- The patch pack is a superset that overrides the base pack, so a 1:1 loader layers the packs in pack-priority order (INFERRED; the pack-order rule belongs to Worker 2).
- Text uses `||` as an in-string paragraph separator (e.g. diplomacy tooltips) and literal `\n` sequences.

### Key scheme (CONFIRMED on many samples)
The key is `<table>_<field>_<primary key>`. For multi-column keys, the key columns are concatenated with **no separator**.

| loc key | text |
|---|---|
| `factions_screen_name_france` | "France" |
| `factions_screen_adjective_scotland` | "Scottish" |
| `factions_attack_desc_<key>` / `factions_defend_desc_<key>` | 77 each |
| `regions_onscreen_eur_france` | "France" |
| `regions_battle_name_egy_amman` | "Amman" |
| `regions_sea_onscreen_name_black_sea` | (sea regions; not a shipped table) |
| `technologies_onscreen_name_military5_percussion_cap` | |
| `technologies_short_description_*`, `technologies_long_description_*` | |
| `character_trait_levels_onscreen_name_C_Gent_Research_Civil_1` | "Lawyer" |
| `character_trait_levels_colour_text_*` | |
| `ancillaries_onscreen_name_*`, `ancillaries_colour_text_*` | |
| `building_culture_variants_name_fFort1_wooden_artillery_fort` + `european` | "Wooden Fort" (key = building + culture) |
| `building_chains_chain_tooltip_<chain>` | |
| `names_name_` + `names_german_catholic` + `Maximilian` | "Maximilian" (key = names group + name) |
| `effects_description_admin_cost_mod` | |
| `ministerial_positions_strings_on_screen_<title>` | |
| `slots_gdp_values_onscreen_name_port4` | |
| `campaign_ground_types_onscreen_name_*` | |
| `historical_characters_on_screen_name_<key>` | |
| `campaign_map_towns_and_ports_onscreen_name_town:egy_amman:zarqa` | |
| `names_forts_fort_name_<group><name>` | |
| `diplomacy_strings_string_<key>` | |
| `mission_text_text_<key>` | |

**ESF keys resolve (CONFIRMED with `loc-get`):**

| ESF key | text |
|---|---|
| `names_name_names_german_catholicMaximilian` | "Maximilian" |
| `start_pos_settlements_onscreen_name_settlement:eur_france:paris-1491848185` | "Paris" |
| `start_pos_factions_description_-1759426676` | "PLACEHOLDER" |
| `mission_text_text_eur_france_capture_vienna_heading` | "The Fall of Austria" |
| `unit_regiment_names_localisation_lookup_unit_name_euro_cavalry_units_001` | "1st Regiment of Horse" |
| `agent_culture_details_onscreen_name_ministereuropean` | "Minister" |
| `historical_characters_on_screen_name_eur_guillaume_brune` | "Guillaume Marie Anne Brune" |

How the `start_pos_*` keys are formed:
- They come from the **non-shipped** `start_pos_*` tables, with a signed-i32 hash suffix (e.g. `...paris-1491848185`).
- The rebel name key is region + campaign, e.g. `start_pos_regions_rebel_faction_name_egy_ammanegy_napoleon`.
- The exe knows 23 `start_pos_*` tables (Worker 1's `db_table_names.txt`: start_pos_factions, regions, settlements, slots, characters, character_traits, character_ancillaries, land_units, naval_units, diplomatic_relationships, victory_conditions, technologies, calendars, ...). **None are shipped in data.pack (CONFIRMED).** The ESF start position is therefore the only shipped source of start-state data, and our game must load `startpos.esf`. It cannot rebuild it from DB.

DB string columns that hold English text (e.g. `factions.screen_name` "Austria", `regions` "Amman") are the **editor defaults**. At runtime they are superseded by the loc key built from the same field name (INFERRED: the loc and DB values match in every sample checked).

---

## 2. factions (v3, 77 rows, 57,258 bytes): CONFIRMED parse
Schema (48 columns): `s i s s s s s s o b b b s s o o f×18 s o f f f o s b b s s s s b`

| # | type | inferred name | evidence / values | conf. |
|---|---|---|---|---|
| 0 | s | key | austria, france, egy_french_republic, spa_spain, ... | CONF |
| 1 | i | id / ui order | 1..~60 for older rows; hash-like for newer ones (egy_bedouin 0x8E774876) | INF |
| 2 | s | subculture | sc_european_west x27, south x27, ... | CONF (FK cultures_subcultures) |
| 3 | s | category | playable 16, minor 28, non-expansionist 20, rebel 13 | CONF values |
| 4 | s | screen_name (loc `factions_screen_name_`) | "Austria" | CONF |
| 5 | s | screen_adjective (loc `factions_screen_adjective_`) | "Austrian" | CONF |
| 6 | s | character names group | names_german_catholic | CONF (FK names_groups) |
| 7 | s | unit/uniform set or model faction | usa x21, france x6, britain x5, savoy, ... | INF |
| 8 | o | culture-specific variant | None x73, "Ottoman" x4 | INF |
| 9-11 | b | flags (UNKNOWN; true 13/11/11) | austria F,T,T; rebels T,F,F | UNK |
| 12 | s | unit-card icon path | data\ui\units\icons\austria | CONF |
| 13 | s | flag path | data\ui\flags\austria | CONF |
| 14 | o | republic flag path | data\ui\flags\austria_republic | CONF |
| 15 | o | rebel flag path | data\ui\flags\rebels_europe | CONF |
| 16-33 | f×18 | 3 colours, each RGB repeated twice: primary, secondary, tertiary | france 9,79,150 / 228,180,9 / 49,65,100 = **exactly ESF `FACTION_FLAG_AND_COLOURS`** | CONF (cross-check) |
| 34 | s | faction group | euro_group, france_group, italian_campaign_group, ... | CONF values |
| 35 | o | rebel faction key | austrian_rebels x73, british/french/portugese/spanish_rebels | CONF values |
| 36-38 | f×3 | colour (173,173,173 or 0) | | INF |
| 39 | o | voice/actor id string | None x44, "0", "22".."55" | INF |
| 40 | s | language/voice code | Ge, It, Fr, Uk, Ru, Tu, Du, Sp, Po, Ar, Sw, Pl | CONF values |
| 41-42 | b | flags (true 4 / 8) | | UNK |
| 43 | s | ship names group (INFERRED) | names_english x47, ... | INF |
| 44 | s | secondary names group | names_german_catholic, ... | INF |
| 45 | s | attack description (loc `factions_attack_desc_`) | "You are attacking an Austrian force." | CONF |
| 46 | s | defend description (loc `factions_defend_desc_`) | | CONF |
| 47 | b | always false | | UNK |

Example: `"france" 6 sc_european_west playable "France" "French" names_french france None false true true ... france_group austrian_rebels 173 173 173 "24" "Fr" false true names_english names_french ...`

## 3. Regions and the map

| table | ver | rows | schema | columns (inferred names) | examples |
|---|---|---|---|---|---|
| regions | 1 | 159 | `ssiiis` CONF | key, continent (FK regions_continents), r, g, b (0..255: map colour, INF), battle name (loc `regions_battle_name_`) | `egy_amman cont_middle_east 209 124 13 "Amman"`; `eur_france cont_europe 77 109 139 "France"` |
| regions_continents | 0 | 8 | `s` | key | cont_europe, cont_middle_east |
| campaign_map_settlements | 0 | 159 | `sssis` | settlement key, region, display name, i (1..3: size/level, INF), type ("settlement") | `settlement:egy_beheira:damanhour egy_beheira Damanhour 3 settlement` |
| campaign_map_slots | 0 | 145 | `sssib` | slot key, region, slot type, i, b | `gold:eur_austria:rauris eur_austria gold 0 false` |
| campaign_map_towns_and_ports | 0 | 233 | `sss` | key, town type (town-commercial / industrial / intellectual, port), display name | `town:eur_austria:graz town-intellectual Graz` |
| campaign_map_playable_areas | 0 | 5 | `sbooosiii` | theatre id (= ESF theatre "1244818741"), b, map tga, lookup tga, radar tga, preview name, width, height, i | `1244818741 false europe_map.tga europe_lookup.tga stratradar_europe.tga europe_main 605 300 0`. Matches the ESF `MAPS` and `regions.esf` theatre id (CONF) |
| slots | 1 | 21 | `sbbbbb` CONF (hex) | slot type, 5 flags (UNK names; horses t,f,f,t,f; port f,f,t,f,t) | fort, gold, horses, iron, port, settlement, ... |
| slots_gdp_values | 0 | 59 | `siif` | slot type, level 1..5, i, f | `gold 1 0 1` |
| resources | 0 | 20 | `sosis` | key, trade unit name ("sacks"), slot type, i (corn 25), pip icon | `res_coffee "sacks" settlement 0 ...coffee.tga` |
| region_unit_resources | 0 | 37 | `ss` | key, display name | armed_citizenry, balkans, bedouin. These are the ESF `RESOURCES_ARRAY` values "cossacks", "balkans", ... (CONF) |
| religions | 0 | 11 | `sis` | key, i (ordinal), pip icon | `rel_catholic 4` |
| population_classes | 0 | 3 | `sbbb` | lower, middle, upper + 3 flags | |
| cultures | 0 | 9 | `sio` CONF (hex) | key, i (id/hash), parent culture | `egy_european 232665311 european`; `european 1 None` |
| cultures_subcultures | 0 | 16 | `ssis` CONF | subculture, culture, i (id/hash), display name | `sc_european_america european 12 "American"` |

## 4. Buildings

**building_levels** (v0, 137 rows). Schema `ssibbiiiiiiiiiiiiiibiiii` CONF (hex plus `db-tail`; the 12 zero i32 columns are a zero-run ambiguity).

| # | type | name (INF) | values |
|---|---|---|---|
| 0 | s | key | fFort1_wooden_artillery_fort |
| 1 | s | chain (FK building_chains) | fFort |
| 2 | i | level index 0..4 | |
| 3,4 | b | flags, always false | UNK |
| 5 | i | construction turns | 1..12 |
| 6 | i | cost | 300..16000 |
| 7-18 | i×12 | always 0 in shipped data | UNK |
| 19 | b | unique / "great building" flag | true for sPrest_*, sCulture5_*, sArmy5, sCannon5, sAdmin5 |
| 20-23 | i×4 | **prestige** by category: military, naval, economic, cultural (INF) | sPrest_france_arcdetriomphe 40,0,0,120; sCulture5_* 0,0,0,40; sAdmin5 0,0,20,0 |

Examples: `fFort1_wooden_artillery_fort fFort 0 F F 3 4000 0×12 F 0 0 0 0`; `fFort2_western_artillery_fort fFort 1 F F 6 8000 ... F 10 0 0 0`.

| table | rows | schema | columns | examples |
|---|---|---|---|---|
| building_chains | 48 | `sooo` | chain, o, o, category | `fFort None None military` |
| building_chain_to_slots | 82 | `ss` | chain, slot type | `pNavy port` |
| building_upgrades_junction | 89 | `ss` | level, upgrades-to level | `fFort1_... → fFort2_...` |
| building_effects_junction | 354 | `ssf` | level, effect (FK effects), value | `pNavy1_shipyard naval_recruitment_points 1`; `pNavy2_dockyard happy_industrialisation_lower -1` |
| building_factionwide_effects_junctions | 98 | `ssf` | level, effect, value | `pTrade2_commercial_port tw_growth_industry_global 1` |
| building_level_required_technology_junctions | 21 | `ss` | level, technology | `pNavy3_steam_drydock economy2_steam_engine` |
| building_units_allowed | 1708 | `ssib` | level, unit, i, b | `pNavy1_shipyard Small_Brig 0 false` |
| building_culture_variants | 264 | `ssooooo` | level, culture, battle model, campaign model, o, loc-name key, model | `fFort1_... european fort_euro_wooden Fort_lvl3 None fFort1_..._european Fort_lvl3` |
| building_faction_variants | 44 | `ssooo` | level, faction, model / name / model | `sAdmin5_court_supreme austria EU_sAdmin5_court_supreme ...` |
| building_description_texts | 197 | `s` | key | |

## 5. Technologies

**technologies** (v1, 68 rows). Schema `ssiisiiiibbsi` CONF.

| # | name (INF) | values |
|---|---|---|
| 0 | key | admin1_classical_economics |
| 1 | building level where it is researched | sAdmin1_tax_office |
| 2 | tree position / column | 0..7 |
| 3 | research cost | 70..1400 |
| 4 | key repeated (loc / text key) | |
| 5-8 | 4 integer columns (0..100, mostly multiples of 10); INF: AI weights or research-point modifiers | e.g. 0,0,50,0 |
| 9,10 | b flags (true 28/29) | UNK |
| 11 | icon | innovation.tga |
| 12 | i 0/1 (28 ones) | UNK |

| table | rows | schema | columns | examples |
|---|---|---|---|---|
| technology_effects_junction | 106 | `ssf` | tech, effect, value | `admin2_colonial_funding trade_node_supply_mod 10` |
| technology_required_technology_junctions | 19 | `ss` | tech, prerequisite | `admin2_national_census admin1_national_debt` |
| technology_threads | 5 | `s` | techtree_agriculture/army/industry/... | |
| technology_faction_junctions | 3212 | `ss` | tech, faction (availability) | |

ESF `techs[]` state values 0/2/4 are not in these tables (runtime state).

## 6. Government, ministers and taxes

| table | rows | schema | columns | examples |
|---|---|---|---|---|
| government_types | 4 | `sbbiss` CONF (hex) | key, b, b, i rank (3/2/1), class with power, class without (INF) | `gov_absolute_monarchy F T 3 upper lower`; `gov_republic T F 1 middle lower` |
| government_types_to_effects | 24 | `ssf` | gov, effect, value | `gov_absolute_monarchy recruitment_cost_mod_land_all -5` |
| ministerial_positions | 26 | `si` | post, i (UI order; negatives are royal-family slots) | army 4, finance 2, justice 3, navy 5, head_of_government 1, royal_heir -1, royal_claimant -20 |
| ministerial_positions_by_gov_types | 934 | `sssss` | faction, post, gov type, gender (b/m/f), title key (loc `ministerial_positions_strings_on_screen_`) | `austria army gov_absolute_monarchy b lord_secretary_of_war` |
| ministerial_positions_to_effects | 260 | `sisii` | post, management level 0..9, effect, value, ordinal 1..3 | `army 0 recruitment_cost_mod_land_all 12 1` |
| ministerial_effectiveness_modifiers | 30 | `isi` | i, gov, i | |
| ministerial_positions_to_governorships | 5 | `ss` | governor_africa → africa | |
| governorships | 5 | `s` | africa, america, asia, europe, india | |
| taxes_levels | 5 | `si` | tax_extortionate 25, tax_high 20, tax_low 10, ... (percent, INF) | |
| taxes_keys | 10 | `sss` | class, level, effect-bundle key | `lower_classes tax_high lower_high` |
| taxes_effects_jct | 55 | `ssf` | bundle, effect, value | `lower_extortionate happy_active_lower_tax -16`; `pop_growth_tax_modifier -0.45` |

## 7. Traits and ancillaries

| table | rows | schema | columns | examples |
|---|---|---|---|---|
| character_traits | 163 | `sibis` CONF (hex) | trait, i (1..4; INF max level count or type), b (always F), i (1/2/3/9; INF no-going-back level), category | `C_Admiral_Attacker_Bad 3 F 2 Naval` |
| character_trait_levels | 475 | `sisi` | level key, level 1..n, trait, threshold points | `C_Admiral_Attack_Bad_1 1 C_Admiral_Attacker_Bad 4` (then 8, 16) |
| trait_level_effects | 564 | `ssf` | level, effect, value | `C_Admiral_Attack_Bad_1 Command_Sea_Attack -1` |
| trait_attribute_effects | 157 | `ssi` | level, attribute, value | `C_Admiral_Bad_1 command_sea -1` |
| trait_triggers | 166 | `sss` | trigger, event, **condition expression** (source of export_triggers.lua) | `C_Admiral_Bad_Trigger CharacterCompletedBattle "CharacterType(\"admiral\") and not CharacterWonBattle() ..."` |
| trait_to_antitraits / trait_to_included_agents / trait_info / trait_categories | 93 / 179 / 163 / 12 | `ss` / `ss` / `ss` / `so` | | `C_Admiral_Bad ↔ C_Admiral_Good`; `admiral`; `agent`; `Naval` |
| historical_character_traits | 928 | `ss` | character, trait level | `egy_adam_duncan C_Admiral_Brave_2` |
| ancillaries | 275 | `sssbbbiii` CONF (hex + `db-tail` unique) | key, portrait type (FK ancillary_types), category ("character"), b, b (true 83), b (true 19), chance/rarity 1..9 (INF), start year (1796/1700/1809), end year (1900/1814/1796) | `Anc_Agent_Provocateur masked_figure character F F F 9 1796 1900` |
| ancillary_to_effects | 317 | `ssf` | anc, effect, value | `Anc_Artist_Portrait prestige_military 1` |
| ancillary_to_attribute_effects | 58 | `ssi` | anc, attribute, value | `Anc_Classical_Scholar research 1` |
| ancillary_to_included_agents / _excluded_ancillaries / ancillary_included_subcultures / ancillary_types / ancillary_info | | `ss` / `ss` / `ss` / `ss` / `s` | | |

**effects** (412 rows, `soi`): effect key, icon, i (8005 / 114 ...; INF a priority / sort or bitfield). `effect_bonus_value_*_junction` (13 tables, `ss` / `sss` / `ssss`) map an effect to an internal bonus-value id, optionally scoped by agent, chain, commodity, class, religion or resource. Example: `admin_cost_mod → admin_cost_mod`.

## 8. Global campaign tunables

**campaign_variables** (121 rows) has schema **`sf`** (CONF: values like 0.6, 1.25, 1e8 only make sense as f32). Key values:

| variable | value |
|---|---|
| road_level_0..3_action_point_cost | 0.67, 0.6, 0.5, 0.4 |
| character_recruitment_base_cost | 400 |
| character_recruitment_cost_per_command_star | 300 |
| tax_efficiency_log_base | 1.25 |
| tax_efficiency_modifier | -4.5 |
| tax_efficiency_total_regions | 136 |
| maximum_attrition_pct | 0.15 |
| base_wealth_increase | 1000 |
| baseline_pop_growth | 0.3 |
| faction_gdp_other | 1000 (minor 1500) |
| happiness_war_* | -15..6 |
| settlement_looting_pct_region_gdp | 0.15 |
| autoresolve_* | 35 entries, e.g. minimum_win_chance_to_win 0.225 |

The full list comes from `db campaign_variables sf 200`. `campaigns_campaign_variables_junctions` (25 rows, `ssf`) holds per-campaign overrides, e.g. `character_recruitment_max_distance`: egy 340, eur 550, ita 340.

`campaign_difficulty_handicap_effects` (88 rows, `ibsf`): difficulty (-2..), b (INF AI-only), effect, value, e.g. `-2 true policing_cost_mod 30`.

## 9. Diplomacy

| table | rows | schema | columns | examples |
|---|---|---|---|---|
| stances | 5 | `ss` | allied, neutral, patron, ... + display | Matches the ESF stance strings (CONF) |
| diplomatic_relations_attitudes | 5 | `si` | attitude, threshold | friendly 45, hostile -85, neutral 0 |
| diplomatic_relations_government_type | 16 | `ssii` | gov A, gov B, i, i | `absolute vs republic -100 -30` |
| diplomatic_relations_religion | 121 | `ssif` | rel A, rel B, i, f (drift, INF) | `rel_animist rel_catholic -5 0.025` |
| religion_conversion_mods | 74 | `ssf` | from, to, rate | |
| diplomacy_strings | 869 | `s` | key (loc `diplomacy_strings_string_`) | `austria_monarchy_accept_friendly` |
| diplomacy_factor_strings | 25 | `s` | factor keys | abandoned_ally_in_war, abused_military_access, alliance |
| diplomacy_negotiation_strings / _faction_override_strings | 1387 / 537 | `ssss` / `sssss` | dialogue text by situation | |

The `force_diplomacy` option strings ("trade agreement", "military access", ...) are **not** a DB table. They are engine literals (question for Worker 1).

## 10. Missions and victory

| table | rows | schema | columns | examples |
|---|---|---|---|---|
| missions | 22 | `ssssssoiib` CONF | key, heading loc key, activity (FK mission_activities), source gov (FK mission_sources), category (military / development / diplomatic / discovery / espionage), text loc key, o (always ""), i, i, b (always 0/F) | `ita_subjugate_piedmont ..._heading capture_city republic military ..._text "" 0 0 F` |
| mission_activities | 15 | `ss` | activity, display | assassination, blockade_port, build, capture_city, engage_faction, make_alliance, make_trade_agreement, recruit, research, spy_on_city, ... |
| mission_sources | 3 | `ss` | absolute_monarchy "The Royal Council", constitutional_monarchy "Parliament", republic "The Government" | |
| mission_effects | 4 | `ss` | T+1, T+1000, T+2000 ... "Treasury +N" | |

The scripted episodic missions (`trigger_custom_mission`) use string rewards (`money:2000`, `grant_unit:...`). They are not driven by these tables. **There is no shipped victory-condition table.** `start_pos_victory_conditions` exists in the exe but is not shipped, so victory data lives only in the ESF `CAMPAIGN_VICTORY_CONDITIONS` (CONF absence).

## 11. Climates, ground types, seasons, commodities

| table | rows | schema | columns | examples |
|---|---|---|---|---|
| climates | 33 | `siiib` | key, **index** (equals the `regions.esf` `climate_indices` u8, CONF cross-check: lc_tundra 0, lc_boreal 1, lc_am_desert 15, sc_arctic 22), i, i (land 400/600, sea 1000/2000; INF), b is_land | `lc_eu_central_humid 14 400 600 true` |
| climate_to_tilesets | 2 | `ss` | lc_desert → desert | |
| campaign_ground_types | v1, 19 | `sfbbb` | key, movement multiplier (INF), b (forest flag: true only for forests), b (desert), b (cold attrition: the `_cold_att` variants) | plains 1.3, grassland 0.9, tundra 0.8, hills 0.6, desert 0.6, light_forest 0.5, hilly_light_forest 0.4, marsh 0.4, swamp 0.2, dense_forest 0, jungle 0. The 8 names in `regions.esf` groundtypes are a subset (CONF) |
| seasons | 4 | `sss` | key, display, numeric string | season_summer "1", season_winter "2", season_spring "0", season_autumn "0". Note: the ESF season enum differs (0 = Summer, 1 = Winter); the meaning of this column is UNKNOWN |
| commodities | 8 | `sff` | key, base price, f | `res_coffee 16 1.3` |
| commodities_demand_junction | 16 | `ssff` | commodity, demand driver, f, f | |
| historical_characters | 505 | `sbsssiis` CONF | key, b, gender, agent type, faction, birth year, death/spawn-until year (INF), description | `egy_adam_duncan T m admiral britain 1751 1804 "British Admiral"` |
| names | 13,945 | `ssssibs` | group, name, forename/surname, gender (m/f/b), i, b, hash string | |
| names_royalty | 273 | `ssisii` | faction, name, i (1..7), gender, i, hash | |
| agents | v1, 17 | `siiibsobsoii` | agent, action points? (admiral 90; INF), i, i, b, model kind (ship/human), o base agent, b, primary attribute, o, i, i | `admiral 90 20 20 T ship General F command_sea None 0 3` |

The `agents.admiral` value 90 matches the ESF LOCOMOTABLE pair (90, 90 / 66) seen for admirals. INFERRED: column 1 = movement points.

## 12. Answers to my earlier §10 questions for Worker 2
1. **Which tables hold the related data:**
   - Victory-condition kinds: none in DB; ESF only.
   - Mission types: `mission_activities` (DB) plus script literals.
   - Diplomacy option keys: engine literals, not DB.
   - Climate keys: `climates`. Ground types: `campaign_ground_types`.
   - Tech states 0/2/4: runtime only.
   - Building levels: `building_levels` + `building_chains`.
2. **ESF loc keys resolve:** yes, CONFIRMED for `names_name_*`, `start_pos_settlements_onscreen_name_*`, `start_pos_factions_description_*`, `mission_text_text_*`, `unit_regiment_names_*`, `agent_culture_details_*` and `historical_characters_on_screen_name_*`.

## 13. Cross-check with Worker 1
- **Table names.** All tables above appear in `db_table_names.txt` as `<name>_table` / `<name>_tables`.
  - 110 exe names have no shipped table. Most are naming variants (e.g. plural `effect_bonus_value_*_junctions`, `ancillary_infos`).
  - 23 are `start_pos_*` tables that are **not shipped**. These are editor-only inputs that were baked into `startpos.esf`.
- **Record classes.** `class_names_from_strings.txt` has `EMPIREUTILITY::FACTION_RECORD(::BUILDER)`, `REGION_RECORD`, `TECHNOLOGY_RECORD`, `BUILDING_LEVEL_RECORD`, `BUILDING_LEVEL_REQUIRED_TECHNOLOGY_JUNCTION_RECORD`, `START_POS_FACTION_RECORD`, `START_POS_VICTORY_CONDITION_RECORD`, `START_POS_TECHNOLOGY_RECORD`, `START_POS_REGION_RECORD`. Worker 1's BUILDER field order should settle the remaining ambiguous columns: building_levels 3-4 and 7-18, the factions bools, `effects` column 2, and the `technologies` 5-12 meanings.

## 14. UNKNOWN / open
- Zero-run ambiguity in building_levels columns 7-18, which are all zero. A real schema may hold bools there.
- Unnamed flags: factions 9-11, 41, 42, 47; slots 1-5; government_types 1-2; technologies 9, 10, 12.
- The i32 id/hash columns (factions column 1, cultures, cultures_subcultures, names) and whether they match ESF ids. They do not match the ESF FACTION ids (749327284 ...), which are runtime handles.
- The .loc trailing bool.
- The `regions` r, g, b columns versus the lookup-TGA palette. The palette region checked was zero; not verified.
