//! Builds the simulation's [`CampaignRules`] from the player's DB tables (see
//! `ntw_sim::campaign::rules` for what each field means and where it comes from).

use std::collections::BTreeMap;

use ntw_data::GameDatabase;
use ntw_sim::campaign::{BuildingRules, CampaignRules, UnitAutoresolve, UnitRules};
use ntw_sim::campaign::rules::TechRules;
use ntw_sim::unit_kind::Category;

/// The faction key whose director rival score is always 0, besides the rebels: `0x00C3FD50` compares
/// the faction's key with this literal (CONFIRMED).
const NO_RIVAL_FACTION: &str = "pirates";

/// Reports, once at load, a link a row of `table` (the row of `row`) cannot make because `key` is not
/// a key of `target`: the exe's table lookup logs "In table %S: '%S' is not a valid key for this
/// table" there (e.g. `0x00F21A30`) and drops the link. Caveat: the unit-tech linker (`0x00EA9B10`) drops a
/// row naming no built unit with no log at all; ours logs that case too (a deliberate extra, once per load).
fn invalid_key(table: &str, row: &str, target: &str, key: &str) {
    log::warn!("In table {table}: '{key}' is not a valid key of {target} (row of {row}); the link is dropped");
}

/// Copies every table the campaign rules use, with `campaign`'s per-campaign variable overrides
/// (`campaigns_campaign_variables_junctions`) applied and the exe's rule switches for the key
/// ([`crate::features::original`]; a source with its own sets them in [`crate::source::open`]).
/// Content comes from the DB only (so mods that
/// add rows are picked up).
pub fn rules_from_db(db: &GameDatabase, campaign: &str) -> CampaignRules {
    let c = &db.campaign;
    let mut r = CampaignRules { campaign: campaign.to_string(), limits: db.limits.clone(), features: crate::features::original(campaign).validated(), ..Default::default() };
    for v in &c.variables {
        r.variables.insert(v.key.clone(), v.value);
    }
    for o in c.variable_overrides.iter().filter(|o| o.campaign == campaign) {
        r.variables.insert(o.variable.clone(), o.value);
    }
    for u in &db.units {
        let stats = db.unit_stats(&u.key);
        let men = stats.map_or(0, |s| s.num_men.max(0) as u32);
        r.units.insert(
            u.key.clone(),
            UnitRules {
                cost: u.recruitment_cost,
                upkeep: u.upkeep,
                turns: u.unknown_38.max(1) as u32,
                is_naval: ntw_sim::unit_kind::category(&u.category).is_naval(),
                men,
                autoresolve: unit_autoresolve(db, &u.key),
                category: u.category.clone(),
                unit_class: u.unit_class.clone(),
                campaign_cost: u.unknown_3c,
                flag_21: u.unknown_9c,
                militia: stats.is_some_and(|s| s.unknown_205),
                campaign_stealth: stats.is_some_and(|s| s.unknown_204),
                unit_cap: u.unit_cap,
                travel_speed: u.unknown_44,
                unit_resource: u.recruitment_scope.clone().filter(|s| !s.is_empty()),
            },
        );
    }
    r.hiding_ground = c.ground_types.iter().filter(|g| g.hides).map(|g| g.key.clone()).collect();
    r.general_units = c.agent_cultures.iter().filter(|a| a.agent == "General").filter_map(|a| Some((a.culture.clone(), a.unit.clone()?))).collect();
    r.historical = c
        .historical_characters
        .iter()
        .map(|h| ntw_sim::campaign::rules::HistoricalCandidate {
            key: h.key.clone(),
            male: h.gender != "f",
            kind: h.agent.clone(),
            faction: h.faction.clone(),
            years: (h.year_from, h.year_to),
            note: h.note.clone(),
        })
        .collect();
    // `building_chains` #2 parsed as the exe does (`0x004F3720`: an optional '-', digits; empty = 0).
    let chain_class = |chain: &str| -> i32 {
        c.building_chains.iter().find(|x| x.key == chain).and_then(|x| x.class_number.as_deref()).and_then(|s| s.parse().ok()).unwrap_or(0)
    };
    // `building_chains` #1 as the technology thread (`0x00F21E60`, exact case; anything else is 3).
    let chain_thread = |chain: &str| -> i32 {
        match c.building_chains.iter().find(|x| x.key == chain).and_then(|x| x.group.as_deref()) {
            Some("military") => 0,
            Some("industry") => 1,
            Some("enlightenment") => 2,
            _ => ntw_sim::campaign::rules::NO_THREAD,
        }
    };
    for b in &db.building_levels {
        r.buildings.insert(
            b.key.clone(),
            BuildingRules {
                chain: b.chain.clone(),
                level: b.level,
                cost: b.cost,
                turns: b.construction_turns.max(1) as u32,
                chain_class: chain_class(&b.chain),
                chain_thread: chain_thread(&b.chain),
                ..Default::default()
            },
        );
    }
    for e in &c.building_effects {
        if let Some(b) = r.buildings.get_mut(&e.building) {
            b.effects.push((e.effect.clone(), e.value));
        }
    }
    for a in &c.building_units {
        if let Some(b) = r.buildings.get_mut(&a.building) {
            b.units_allowed.push(a.unit.clone());
        }
    }
    for u in &c.building_upgrades {
        if let Some(b) = r.buildings.get_mut(&u.building) {
            b.upgrades_to.push(u.upgrade.clone());
        }
    }
    for v in c.building_faction_variants.iter() {
        if let Some(b) = r.buildings.get_mut(&v.building) {
            b.factions.push(v.faction.clone());
        }
    }
    for v in c.building_culture_variants.iter() {
        if let Some(b) = r.buildings.get_mut(&v.building) {
            b.cultures.push(v.culture.clone());
        }
    }
    for s in &c.chain_slots {
        r.chain_slots.entry(s.chain.clone()).or_default().push(s.slot_type.clone());
    }
    for x in c.commodity_demand.iter() {
        r.commodity_demand.push((x.commodity.clone(), x.driver.clone(), x.factor, x.weight));
    }
    for t in db.technologies.iter() {
        // The thread is the building level's chain record +0xC (linker `0x00F21A30`); a technology
        // without a known level has none. ORIGINAL BUG: the exe logs the unknown key, leaves record
        // +0x5C null and its availability walk `0x008F91F0` then dereferences it (a crash); ours
        // logs it here and never opens the technology (`update_tech_availability`).
        let thread = match r.buildings.get(&t.building_level) {
            Some(b) => b.chain_thread,
            None => {
                invalid_key("technologies", &t.key, "building_levels", &t.building_level);
                ntw_sim::campaign::rules::NO_THREAD
            }
        } as usize;
        r.technologies.insert(t.key.clone(), TechRules { cost: t.research_cost, building_level: t.building_level.clone(), thread, ..Default::default() });
    }
    // `technology_required_building_levels_junctions` (linker `0x00F21A30`): a row naming no
    // technology or no building level is dropped (logged once, here).
    for q in c.tech_building_levels.iter() {
        let table = "technology_required_building_levels_junctions";
        if !r.buildings.contains_key(&q.building) {
            invalid_key(table, &q.technology, "building_levels", &q.building);
        } else if let Some(t) = r.technologies.get_mut(&q.technology) {
            t.required_buildings.push(q.building.clone());
        } else {
            invalid_key(table, &q.technology, "technologies", &q.technology);
        }
    }
    // `unit_required_technology_junctions`, linked at load by `0x00EA9B10` (CONFIRMED): for each built
    // unit, each row naming it (exact case) in table order adds its technology to the unit's list, unless
    // the technology is no `technologies` key (logged, row dropped). A row naming no built unit matches
    // nothing and is dropped without a log in the exe; ours logs it too (once per campaign load, here).
    for u in c.unit_techs.iter() {
        let table = "unit_required_technology_junctions";
        if !r.units.contains_key(&u.unit) {
            invalid_key(table, &u.technology, "units", &u.unit);
        } else if r.technologies.contains_key(&u.technology) {
            r.unit_techs.entry(u.unit.clone()).or_default().push(u.technology.clone());
        } else {
            invalid_key(table, &u.unit, "technologies", &u.technology);
        }
    }
    // The building level linker `0x00DDF300` drops a row whose technology is not a `technologies` key
    // (logged once, here).
    for b in c.building_techs.iter() {
        if r.technologies.contains_key(&b.technology) {
            r.building_techs.entry(b.building.clone()).or_default().push(b.technology.clone());
        } else {
            invalid_key("building_level_required_technology_junctions", &b.building, "technologies", &b.technology);
        }
    }
    for g in c.government_relations.iter() {
        r.government_relations.insert((g.government.clone(), g.other.clone()), (g.limit, g.value));
    }
    for t in c.attitude_thresholds.iter() {
        r.attitude_thresholds.insert(t.key.clone(), t.value);
    }
    // The campaign director's variables, in table order (its lookup takes the first match).
    r.director.configs = c
        .cdir_configs
        .iter()
        .map(|x| ntw_sim::campaign::director::CdirConfig {
            campaign: x.campaign.clone(),
            faction: x.faction.clone().unwrap_or_default(),
            variable: x.variable.clone(),
            value: x.value.clone().unwrap_or_default(),
        })
        .collect();
    r.director.no_rival_keys = vec![NO_RIVAL_FACTION.to_owned()];
    r.ai_personalities = ai_personalities_from_db(db);
    warn_missing_capture_tunables(&r.ai_personalities);
    // A repeated key keeps the last row, as the original: its table build (`0x00F7AA60`, overrides
    // `0x00F7A6E0`) keeps every row and inserts each row index into the key map with `0x007C7A60`, which
    // overwrites the index of a key already there; the lookups (`0x00BF5A60` / `0x00C55CC0`) read that
    // map. Each repeat is reported once.
    for s in c.negotiation_strings.iter() {
        let key = (s.event.clone(), s.culture.clone(), s.government.clone());
        if let Some(old) = r.negotiation_strings.insert(key, s.string.clone()) {
            log::warn!("diplomacy_negotiation_strings: ({}, {}, {}) is listed twice; the last row ({}) wins over {old}", s.event, s.culture, s.government, s.string);
        }
    }
    for s in c.negotiation_override_strings.iter() {
        let key = (s.event.clone(), s.culture.clone(), s.government.clone(), s.faction.clone());
        if let Some(old) = r.negotiation_overrides.insert(key, s.string.clone()) {
            log::warn!(
                "diplomacy_negotiation_faction_override_strings: ({}, {}, {}, {}) is listed twice; the last row ({}) wins over {old}",
                s.event, s.culture, s.government, s.faction, s.string
            );
        }
    }
    for f in db.factions.iter() {
        r.faction_subcultures.insert(f.key.clone(), f.subculture.clone());
        // The faction's culture: `cultures_subcultures` of its subculture (sc_european_west →
        // european, sc_egy_european → egy_european), the keys `building_culture_variants` uses.
        if let Some(s) = c.characters.subcultures.get(&f.subculture) {
            r.faction_cultures.insert(f.key.clone(), s.culture.clone());
        }
        // An absent id is the empty string the exe's record holds (and compares).
        r.faction_movie_ids.insert(f.key.clone(), f.movie_id.clone().unwrap_or_default());
    }
    // An absent third column is the empty string the exe's record holds (and compares).
    r.culture_alternates = c.characters.cultures.iter().map(|k| (k.key.clone(), k.alternate.clone().unwrap_or_default())).collect();
    r.movie_events = c
        .movie_event_strings
        .iter()
        .map(|m| ntw_sim::campaign::outcome::MovieEventRow { event: m.event.clone(), culture: m.culture.clone(), movie: m.movie.clone(), id: m.id.clone() })
        .collect();
    r.message_events = c
        .message_event_strings
        .iter()
        .map(|m| ntw_sim::campaign::message_events::MessageEventRow {
            event: m.event.clone(),
            culture: m.culture.clone(),
            text: m.text.clone(),
            picture: m.picture.clone(),
            layout: m.layout.clone(),
            requires_response: m.requires_response,
            auto_show: m.auto_show,
            icon: m.icon.clone(),
            image: m.image.clone(),
        })
        .collect();
    for m in c.effects.religion_conversion.iter() {
        r.conversion_mods.insert((m.religion.clone(), m.other.clone()), m.value);
    }
    for s in c.naval_stats.iter() {
        let raw = [s.c61, s.c62, s.c63, s.c64, s.c65, s.c66, s.c67, s.c68, s.c69, s.c70, s.c71, s.c72, s.c73, s.c74, s.c75, s.c76, s.c77, s.c78, s.c79, s.c80, s.c81, s.c82, s.c83, s.c84, s.c85, s.c86, s.c87, s.c88, s.c89, s.c90, s.c91, s.c92, s.c93, s.c94, s.c95, s.c96, s.c97, s.c98, s.c99, s.c100, s.c101, s.c102, s.c103, s.c104, s.c105, s.c106, s.c107, s.c108, s.c109, s.c110, s.c111, s.c112, s.c113, s.c114, ];
        let hull: f32 = (0..18).map(|k| raw[3 * k + 1] as f32).sum();
        let sink_weight: f32 = (0..18).map(|k| f32::from_bits(raw[3 * k + 2] as u32)).sum::<f32>() / 18.0;
        // The `units` row: its category gives the range class (`0x0070D370` table) and the merchant flag, its class
        // the bombard factor (CAMPAIGN_FIDELITY.md §Naval autoresolve).
        let unit = db.units.get(&s.key);
        let category = ntw_sim::unit_kind::category(unit.map_or("", |u| u.category.as_str()));
        let class = unit.map_or("", |u| u.unit_class.as_str());
        r.ships.insert(
            s.key.clone(),
            ntw_sim::campaign::naval::ShipRules {
                crews: [s.c17, s.c18, s.c19],
                hull,
                sink_weight,
                morale: s.c1 as f32,
                fire: [s.c20, s.c21],
                guns: 0,
                range: category.range_class(),
                bombard: matches!(class, "naval_bomb_ketch" | "naval_rocket_ship"),
                merchant: category == Category::NavalMerchant,
            },
        );
    }
    for ch in c.chains.iter() {
        r.chain_kinds.insert(ch.key.clone(), ch.kind.as_deref().and_then(|s| s.trim().parse().ok()).unwrap_or(0));
    }
    // `technology_required_technology_junctions` (linker `0x00F21A30`'s second pass): a row naming an
    // unknown technology is dropped (logged once, here); a requirement of the same tree (the same chain
    // pair) becomes the single requirement +0x60, each such row overwriting the last.
    let pair = |r: &CampaignRules, tech: &str| r.technologies.get(tech).and_then(|t| r.buildings.get(&t.building_level)).map(BuildingRules::chain_pair);
    for q in c.tech_requirements.iter() {
        let table = "technology_required_technology_junctions";
        if !r.technologies.contains_key(&q.required) {
            invalid_key(table, &q.technology, "technologies", &q.required);
            continue;
        }
        let same_tree = pair(&r, &q.technology).is_some_and(|p| pair(&r, &q.required) == Some(p));
        if let Some(t) = r.technologies.get_mut(&q.technology) {
            t.requires.push(q.required.clone());
            if same_tree {
                t.parent = Some(q.required.clone());
            }
        } else {
            invalid_key(table, &q.technology, "technologies", &q.technology);
        }
    }
    // `technology_faction_junctions`: the row builder `0x00F20F00` keeps each faction once (a hash set).
    for q in c.tech_factions.iter() {
        if let Some(t) = r.technologies.get_mut(&q.technology)
            && !t.factions.contains(&q.faction)
        {
            t.factions.push(q.faction.clone());
        }
    }
    for g in c.government_types.iter() {
        r.government_classes.insert(g.key.clone(), (g.upper_class.clone(), g.lower_class.clone()));
        r.government_ranks.insert(g.key.clone(), g.value);
    }
    for x in c.religion_relations.iter() {
        r.religion_relations.insert((x.religion.clone(), x.other.clone()), x.value);
        r.religion_attitudes.insert((x.religion.clone(), x.other.clone()), x.attitude);
    }
    for n in &c.trade_nodes {
        r.trade_nodes.insert(n.key.clone(), (n.commodity.clone(), n.base, n.per_ship, n.cap));
    }
    for t in &c.tax_levels {
        r.tax_levels.insert(t.key.clone(), t.rate);
    }
    for e in &c.government_effects {
        r.government_effects.entry(e.government.clone()).or_default().push((e.effect.clone(), e.value));
    }
    for a in &c.agents {
        r.agent_action_points.insert(a.key.clone(), a.action_points);
        r.agent_caps.insert(a.key.clone(), (a.cap_base, a.cap_ceiling));
        if let Some(rel) = a.unknown_50.as_ref().filter(|s| !s.is_empty()) {
            r.agent_religions.insert(a.key.clone(), rel.clone());
        }
        r.agent_sight.insert(a.key.clone(), a.unknown_1c);
        if !a.residence_limited {
            r.residence_exempt_agents.insert(a.key.clone());
        }
        if !a.primary_attribute.is_empty() {
            r.agent_primary_attributes.insert(a.key.clone(), a.primary_attribute.clone());
        }
        if let Some(folder) = a.portrait_folder.as_ref().filter(|s| !s.is_empty()) {
            r.agent_portrait_folders.insert(a.key.clone(), folder.clone());
        }
    }
    // Each faction's unit set (`0x00F74CB0`): its grouping's units, then its own permission rows in table order.
    for f in &db.factions {
        let grouping = c.unit_groupings.iter().filter(|g| g.grouping == f.faction_group).map(|g| g.unit.as_str());
        let rows = c.unit_factions.iter().filter(|p| p.faction == f.key).map(|p| (p.unit.as_str(), p.allowed));
        r.faction_units.insert(f.key.clone(), ntw_sim::campaign::rules::faction_unit_set(grouping, rows));
    }
    // Only `enabled` rows link a government type to the unit (`0x00E91320`: status 0, `0x00ED5710`).
    for p in c.unit_governments.iter().filter(|p| p.status == "enabled") {
        r.unit_governments.entry(p.unit.clone()).or_default().push(p.government.clone());
    }
    for f in &db.factions {
        r.faction_categories.insert(f.key.clone(), f.category.clone());
    }
    r.effects = effect_rules_from_db(db);
    r.characters = character_rules_from_db(db);
    r.historical_events = crate::historical_events::rules(db);
    r
}

/// The campaign AI's personalities from the merged database (`campaign_ai_personalities` and
/// `campaign_ai_personality_junctions`, mods included): the one table the model's AI rules and the
/// campaign AI (`ntw_ai`) read. As the exe's loader `0x00DD5870` builds them (CONFIRMED): the default is the
/// FIRST record whose flag (record +0xC) is set (`0x00DD58DC`: kept only while none is held yet); the default's
/// junction rows apply to every record (pass 1), every other row only to a record not flagged default (pass 2,
/// `0x00DD5996`). So a later record flagged default keeps none of its own rows (it reads the default's values)
/// and a row naming no record applies to none; each is logged once per load.
pub fn ai_personalities_from_db(db: &GameDatabase) -> ntw_sim::campaign::personalities::AiPersonalities {
    let c = &db.campaign;
    let mut p = ntw_sim::campaign::personalities::AiPersonalities::default();
    let mut extra_defaults = std::collections::BTreeSet::new();
    for r in c.ai_personalities.iter() {
        if r.default {
            if p.default.is_empty() {
                p.default = r.key.clone();
            } else if r.key != p.default {
                extra_defaults.insert(r.key.as_str());
            }
        }
        p.by_key.entry(r.key.clone()).or_default();
    }
    for key in &extra_defaults {
        log::warn!(
            "campaign_ai_personalities: {key:?} is flagged default after {:?}; as in the original, the first is the default and {key:?} keeps none of its own tunables",
            p.default
        );
    }
    let mut orphans = 0usize;
    for t in c.ai_personality_tunables.iter().filter(|t| !extra_defaults.contains(t.personality.as_str())) {
        match p.by_key.get_mut(&t.personality) {
            Some(own) => {
                own.insert(t.tunable.clone(), t.value);
            }
            None => orphans += 1,
        }
    }
    if orphans > 0 {
        log::warn!("campaign_ai_personality_junctions: {orphans} rows name no campaign_ai_personalities record; as in the original, they apply to none");
    }
    p
}

/// Logs once, when the rules load, each capture-choice tunable ([`ntw_sim::campaign::capture_ai::TUNABLES`])
/// the default personality lacks: a personality without it reads 0 there. Vanilla's default personality
/// has all seven (test `the_default_personality_has_the_capture_tunables`).
fn warn_missing_capture_tunables(p: &ntw_sim::campaign::personalities::AiPersonalities) {
    if p.by_key.is_empty() {
        return;
    }
    for key in ntw_sim::campaign::capture_ai::TUNABLES {
        if p.tunable(&p.default, key).is_none() {
            log::warn!("campaign AI: the default personality {:?} has no {key}; the AI capture choice reads 0 for it", p.default);
        }
    }
}

/// Compiles every effect source (slot 0-F, `ntw_sim::campaign::effects`): the effect → bonus mapping
/// from the `effect_bonus_value_*_junction` tables, then each source's set.
pub fn effect_rules_from_db(db: &GameDatabase) -> ntw_sim::campaign::effects::EffectRules {
    use ntw_sim::campaign::effects::{BonusKind, EffectKey, EffectRules};
    let c = &db.campaign;
    let t = &c.effects;
    let mut mapping: BTreeMap<String, Vec<EffectKey>> = BTreeMap::new();
    let mut map = |effect: &str, kind: BonusKind, bonus: &str, qualifier: &str| {
        mapping.entry(effect.to_string()).or_default().push(EffectKey { kind, bonus: bonus.to_string(), qualifier: qualifier.to_string() });
    };
    for m in &t.bonus_basic {
        map(&m.effect, BonusKind::Basic, &m.bonus, "");
    }
    for m in &t.bonus_unit_category {
        map(&m.effect, BonusKind::UnitCategory, &m.bonus, &m.category);
    }
    for m in &t.bonus_unit_class {
        map(&m.effect, BonusKind::UnitClass, &m.bonus, &m.class);
    }
    for m in &t.bonus_pop_class {
        map(&m.effect, BonusKind::PopClass, &m.bonus, &m.class);
    }
    // The agent-keyed bonuses: exe bonus type 0 with the agent bonus enum's id (`0x0145B498`), the same keys as
    // the saved entries, so DB and saved sources add up ([`ntw_sim::campaign::effects::agent_bonus_key`]); a
    // bonus name outside that enum keeps its name (nothing in the exe reads one).
    for m in &t.bonus_agent {
        match ntw_sim::campaign::effects::agent_bonus_key(&m.bonus, &m.agent) {
            Some(k) => map(&m.effect, k.kind, &k.bonus, &k.qualifier),
            None => map(&m.effect, BonusKind::Agent, &m.bonus, &m.agent),
        }
    }
    for m in &t.bonus_religion {
        map(&m.effect, BonusKind::Religion, &m.bonus, &m.religion);
    }
    // The population class × religion bonuses (exe type 7): the same key as the saved entries, so DB and saved
    // sources add up. The exe's row builder (`0x00F86F00`) drops, with a log, a row whose class or religion is
    // not a key; ours keeps it, as for every junction here (the class and religion tables are not loaded; every
    // vanilla row resolves: `real_install` `population_class_and_religion_junction_rows`).
    for m in &t.bonus_pop_class_religion {
        let k = EffectKey::pop_class_religion(&m.bonus, &m.class, &m.religion);
        map(&m.effect, k.kind, &k.bonus, &k.qualifier);
    }
    // The chain-keyed bonuses: exe bonus type 2, the same keys as the saved entries (`mod_cost` 0, `mod_gdp` 1,
    // `mod_tw_growth` 2, `mod_commodity_production` 3; BONUS_NAMES 0x98..0x9B), so DB and saved sources add up.
    for m in &t.bonus_chain {
        if let Some(id) = ["mod_cost", "mod_gdp", "mod_tw_growth", "mod_commodity_production"].iter().position(|b| *b == m.bonus) {
            map(&m.effect, BonusKind::Saved(2), &id.to_string(), &m.chain);
        }
    }
    let mut r = EffectRules::new(mapping);
    // `taxes_keys` + `taxes_effects_jct`: each (class, level) key's bundle, compiled through the mapping (a
    // bundle with no `taxes_effects_jct` rows has no effects: not an error).
    let mut bundles: BTreeMap<&str, Vec<(String, f32)>> = BTreeMap::new();
    for e in &c.tax_effects {
        bundles.entry(e.bundle.as_str()).or_default().push((e.effect.clone(), e.value));
    }
    for k in &c.tax_keys {
        r.set_tax_bundle(&k.class, &k.level, bundles.get(k.bundle.as_str()).cloned().unwrap_or_default());
    }
    // Each source's rows (in table order), compiled into its set by its `insert_*` method.
    compile_by_source(&mut r, t.technology.iter(), |x| x.technology.clone(), |x| (&x.effect, x.value), EffectRules::insert_technology);
    compile_by_source(&mut r, c.building_effects.iter(), |x| x.building.clone(), |x| (&x.effect, x.value), EffectRules::insert_building_local);
    compile_by_source(&mut r, t.building_factionwide.iter(), |x| x.building.clone(), |x| (&x.effect, x.value), EffectRules::insert_building_factionwide);
    compile_by_source(&mut r, c.government_effects.iter(), |x| x.government.clone(), |x| (&x.effect, x.value), EffectRules::insert_government);
    compile_by_source(&mut r, t.trait_level.iter(), |x| x.level_key.clone(), |x| (&x.effect, x.value), EffectRules::insert_trait_level);
    compile_by_source(&mut r, t.ancillary.iter(), |x| x.ancillary.clone(), |x| (&x.effect, x.value), EffectRules::insert_ancillary);
    compile_by_source(&mut r, t.ministerial.iter(), |x| (x.post.clone(), x.level), |x| (&x.effect, x.value as f32), EffectRules::insert_ministerial);
    compile_by_source(&mut r, t.difficulty.iter(), |x| (x.difficulty, x.human), |x| (&x.effect, x.value), EffectRules::insert_difficulty);
    for a in &t.trait_attribute {
        r.trait_attribute.entry(a.level_key.clone()).or_default().push((a.attribute.clone(), a.value));
    }
    for a in &t.ancillary_attribute {
        r.ancillary_attribute.entry(a.ancillary.clone()).or_default().push((a.attribute.clone(), a.value));
    }
    for l in &t.trait_levels {
        r.trait_levels.entry(l.trait_key.clone()).or_default().push((l.threshold, l.level, l.level_key.clone()));
    }
    for v in r.trait_levels.values_mut() {
        v.sort();
    }
    for m in &t.ministerial_effectiveness {
        r.ministerial_effectiveness.insert((m.government.clone(), m.value), m.modifier);
    }
    r
}

/// Groups `rows` by their source `key` (each group in table order) and compiles each group into `r`
/// with `insert`, one of the `EffectRules::insert_*` methods.
fn compile_by_source<'a, T, K: Ord>(
    r: &mut ntw_sim::campaign::effects::EffectRules,
    rows: impl Iterator<Item = &'a T>,
    key: impl Fn(&'a T) -> K,
    row: impl Fn(&'a T) -> (&'a String, f32),
    insert: impl Fn(&mut ntw_sim::campaign::effects::EffectRules, K, Vec<(&'a str, f32)>),
) where
    T: 'a,
{
    let mut groups: BTreeMap<K, Vec<(&'a str, f32)>> = BTreeMap::new();
    for x in rows {
        let (effect, value) = row(x);
        groups.entry(key(x)).or_default().push((effect.as_str(), value));
    }
    for (k, rows) in groups {
        insert(r, k, rows);
    }
}

/// The autoresolve inputs of a land unit: the battle's record potentials (`ntw_sim::battle::strength`,
/// CONFIRMED `0x00757120` / `0x007575A0`) of a unit built from its rows like the battle setup does,
/// evaluated at 1 and 2 men to split them into a per-man part and a fixed part (both functions
/// are linear in the men). `None` without a `unit_stats_land` row (ships).
pub fn unit_autoresolve(db: &GameDatabase, key: &str) -> Option<UnitAutoresolve> {
    use ntw_sim::battle::model::LandUnit;
    use ntw_sim::battle::shooting::MissileWeapon;
    use ntw_sim::battle::strength::{melee_strength, missile_strength};
    let record = db.unit(key)?;
    let stats = db.unit_stats(key)?;
    let mounted = stats.num_mounts > 0 && stats.mount_entity.is_some();
    // The missile weapon as `napoleon::battle::setup::missile_weapon` builds it.
    let weapon = (stats.ammunition > 0)
        .then(|| db.primary_projectile(stats))
        .flatten()
        .map(|p| MissileWeapon {
            range: p.effective_range,
            accuracy: stats.accuracy as f32,
            reload_skill: stats.reload_skill,
            reload_time_s: p.reload_time,
            damage: p.damage,
            projectiles_per_shot: p.projectiles_per_shot.max(1) as u32,
            is_artillery: stats.is_artillery,
            guns: stats.num_guns.max(0) as u32,
            ballistics: ntw_sim::battle::missile::Ballistics {
                trajectory: ntw_sim::battle::missile::Trajectory::from_name(&p.trajectory_class).unwrap_or_default(),
                muzzle_velocity: p.muzzle_velocity,
                max_elevation_deg: p.max_elevation,
                accuracy_modifier: p.accuracy_modifier,
            },
        });
    let card = |men: u32| {
        let mut u = LandUnit::new(0, 0, men, (0.0, 0.0));
        u.melee_attack = stats.melee_attack;
        u.charge_bonus = stats.charge_bonus;
        u.melee_defence = stats.melee_defence;
        u.armour = stats.armour;
        u.shield = stats.unknown_188;
        u.morale_stat = stats.morale;
        u.attributes = GameDatabase::unit_attributes(stats);
        u.is_cavalry = ntw_sim::unit_kind::category(&record.category) == Category::Cavalry || mounted;
        u.unit_class = record.unit_class.clone();
        u.unit_category = record.category.clone();
        u.missile = weapon;
        u
    };
    let (one, two) = (card(1), card(2));
    let melee_per_man = melee_strength(&two) - melee_strength(&one);
    let missile_per_man = missile_strength(&two) - missile_strength(&one);
    Some(UnitAutoresolve {
        melee_per_man,
        melee_base: melee_strength(&one) - melee_per_man,
        missile_per_man,
        missile_base: missile_strength(&one) - missile_per_man,
        morale: stats.morale as f32,
        // The naval codes (6..12) all read as 6 here: no autoresolve rule tells them apart.
        category: ntw_sim::unit_kind::category(&record.category).code().min(6),
    })
}

/// The character rules (slot 0-G, `ntw_sim::campaign::characters`): traits with their thresholds,
/// antitraits and agents; ancillaries with their limits; faction subcultures and cultures.
pub fn character_rules_from_db(db: &GameDatabase) -> ntw_sim::campaign::characters::CharacterRules {
    use ntw_sim::campaign::characters::{AncillaryRule, CharacterRules, TraitRule};
    let t = &db.campaign.characters;
    let mut r = CharacterRules::default();
    for x in &t.traits {
        r.traits.insert(
            x.key.clone(),
            TraitRule { no_going_back_level: x.no_going_back_level, priority: x.priority, hidden: x.hidden, ..Default::default() },
        );
    }
    for x in &t.antitraits {
        if let Some(e) = r.traits.get_mut(&x.trait_key) {
            e.antitraits.push(x.antitrait.clone());
        }
    }
    for x in &t.trait_agents {
        if let Some(e) = r.traits.get_mut(&x.trait_key) {
            e.agents.push(x.agent.clone());
        }
    }
    for x in &db.campaign.effects.trait_levels {
        if let Some(e) = r.traits.get_mut(&x.trait_key) {
            e.thresholds.push((x.level, x.threshold));
        }
    }
    for e in r.traits.values_mut() {
        e.thresholds.sort();
    }
    for x in &t.ancillaries {
        r.ancillaries.insert(
            x.key.clone(),
            AncillaryRule {
                character: x.kind == "character",
                world_unique: x.world_unique,
                faction_unique: x.faction_unique,
                priority: x.priority,
                start_year: x.start_year,
                end_year: x.end_year,
                ..Default::default()
            },
        );
    }
    for x in &t.ancillary_agents {
        if let Some(e) = r.ancillaries.get_mut(&x.ancillary) {
            e.agents.push(x.agent.clone());
        }
    }
    for x in &t.ancillary_subcultures {
        if let Some(e) = r.ancillaries.get_mut(&x.ancillary) {
            e.subcultures.push(x.subculture.clone());
        }
    }
    for x in &t.ancillary_excluded {
        if let Some(e) = r.ancillaries.get_mut(&x.ancillary) {
            e.excluded.push(x.excluded.clone());
        }
    }
    for f in &db.factions {
        r.faction_subculture.insert(f.key.clone(), f.subculture.clone());
    }
    for s in &t.subcultures {
        r.subculture_culture.insert(s.key.clone(), s.culture.clone());
    }
    // `agent_to_agent_attributes`, copied in table order by the character maker `0x00992E60`.
    for x in &t.agent_start_attributes {
        r.agent_attributes.entry(x.agent.clone()).or_default().push((x.attribute.clone(), x.level));
    }
    r
}

/// Fills each ship type's gun count ([`ntw_sim::campaign::naval::ShipRules::guns`]) from the ships of a loaded
/// file. The count belongs to the type's ship model (`unit_stats_naval` #12; the model record's +0x7C, which the
/// potential `0x00758950` reads) and every ship stores it as its full guns (`SHIP_DAMAGE_INFO` #13): CONFIRMED
/// with the debugger, and one count per model over the 8 start positions and the vanilla saves (18 of the 26
/// models appear there). A model no ship of the file uses takes the shipped count recorded below, else the
/// number in its key (PROVISIONAL: the model record's gun list in `models_naval` is not decoded; the steam
/// ships, the ironclad, `3deck80`, `hms_elephant`, `razee44` and `rocket_ship` appear in no vanilla file).
pub fn fill_ship_guns(rules: &mut CampaignRules, db: &GameDatabase, world: &ntw_sim::campaign::World) {
    // The counts of the vanilla files, per model (`economy_check shipguns`).
    const SHIPPED: [(&str, u32); 18] = [
        ("1deck24", 24), ("1deck32", 32), ("1deck38", 38), ("2deck50", 50), ("2deck58", 58), ("2deck64", 64),
        ("2deck74", 74), ("2deck80", 80), ("3deck100", 106), ("3deck98", 98), ("4deck120", 122), ("4deck140", 140),
        ("bomb_ketch", 14), ("brig", 26), ("dhow", 3), ("galley", 4), ("merchant_64", 12), ("sloop", 18),
    ];
    let model_of: BTreeMap<&str, &str> = db.campaign.naval_stats.iter().map(|s| (s.key.as_str(), s.c12.as_str())).collect();
    let mut by_model: BTreeMap<String, u32> = SHIPPED.iter().map(|(m, g)| (m.to_string(), *g)).collect();
    for f in world.forces.values().filter(|f| f.is_navy) {
        for u in &f.units {
            if let (Some(s), Some(m)) = (world.ship_states.get(&u.id), model_of.get(u.unit_key.as_str()))
                && s.max_guns > 0
            {
                by_model.insert(m.to_string(), s.max_guns);
            }
        }
    }
    for (key, r) in rules.ships.iter_mut() {
        let Some(m) = model_of.get(key.as_str()) else { continue };
        // PROVISIONAL for the models of no vanilla file: the number in the key (`1deck38_steam` 38, `razee44` 44,
        // ...), else a guess by the historical ship (HMS Elephant a 74) or the closest shipped type.
        r.guns = by_model.get(*m).copied().unwrap_or_else(|| match *m {
            "hms_elephant" => 74,
            "rocket_ship" => 14,
            "steam_frigate" => 38,
            _ => m.rsplit(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty()).and_then(|s| s.parse().ok()).unwrap_or(0),
        });
    }
}

#[cfg(test)]
mod tests {
    use ntw_data::campaign::{AiPersonalityRecord, AiPersonalityTunableRecord};
    use ntw_data::{GameDatabase, Table};

    /// aicapture review: the exe's loader `0x00DD5870` keeps the FIRST record flagged default (`0x00DD58DC`)
    /// and applies a row only to the default (to all, pass 1) or to a record not flagged default (pass 2), so
    /// a later default-flagged record keeps none of its own rows and a row naming no record applies to none.
    #[test]
    fn the_first_default_flagged_personality_is_the_default() {
        let mut db = GameDatabase::test_fixture();
        let rec = |key: &str, default: bool| AiPersonalityRecord { key: key.into(), default };
        let row = |p: &str, t: &str, value: f32| AiPersonalityTunableRecord { row_key: format!("{p};{t}"), personality: p.into(), tunable: t.into(), value };
        db.campaign.ai_personalities = Table::from_rows(1, vec![rec("own", false), rec("first", true), rec("second", true)]);
        db.campaign.ai_personality_tunables = Table::from_rows(
            1,
            vec![row("first", "A", 1.0), row("second", "A", 2.0), row("own", "A", 3.0), row("nobody", "B", 4.0)],
        );
        let p = super::ai_personalities_from_db(&db);
        assert_eq!(p.default, "first", "the first flagged record, not the last");
        assert_eq!(p.tunable("second", "A"), Some(1.0), "a later default-flagged record reads the default's value");
        assert!(p.by_key["second"].is_empty(), "and keeps none of its own rows");
        assert_eq!(p.tunable("own", "A"), Some(3.0));
        assert!(!p.contains("nobody"), "a row naming no record makes none");
        assert_eq!(p.tunable("own", "B"), None);
    }
}
