//! Builds the simulation's [`CampaignRules`] from the player's DB tables (see
//! `ntw_sim::campaign::rules` for what each field means and where it comes from).

use std::collections::BTreeMap;

use ntw_data::GameDatabase;
use ntw_sim::campaign::{BuildingRules, CampaignRules, UnitAutoresolve, UnitRules, XpCostRow, XpCostTables};
use ntw_sim::campaign::rules::TechRules;

/// Copies every table the campaign rules use, with `campaign`'s per-campaign variable overrides
/// (`campaigns_campaign_variables_junctions`) applied. Content comes from the DB only (so mods that
/// add rows are picked up).
pub fn rules_from_db(db: &GameDatabase, campaign: &str) -> CampaignRules {
    let c = &db.campaign;
    let mut r = CampaignRules { campaign: campaign.to_string(), ..Default::default() };
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
                is_naval: u.category.starts_with("naval"),
                men,
                autoresolve: unit_autoresolve(db, &u.key),
                category: u.category.clone(),
                unit_class: u.unit_class.clone(),
                value_7: u.unknown_3c,
                flag_21: u.unknown_9c,
                militia: stats.is_some_and(|s| s.unknown_205),
                campaign_stealth: stats.is_some_and(|s| s.unknown_204),
            },
        );
    }
    // The experience-adjusted cost tables `0x00ED49A0` reads (`unit_stats_land_experience_bonuses`
    // `+0x24` / `+0x28` and its naval twin `+0x1C` / `+0x20`), keyed by the chevron count. Until
    // this copy, `CampaignRules::xp_cost` is empty and every unit costs its plain `units` cost;
    // with it, a rank-9 recruit costs `flat + ROUND(cost × mult)` (see `economy::recruit_cost`).
    r.xp_cost = XpCostTables {
        land: db.experience_cost_rows().into_iter().map(|(rank, flat, mult)| (rank, XpCostRow { flat, mult })).collect(),
        naval: db.naval_experience_cost_rows().into_iter().map(|(rank, flat, mult)| (rank, XpCostRow { flat, mult })).collect(),
    };
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
        })
        .collect();
    // `building_chains` #2 parsed as the exe does (`0x004F3720`: an optional '-', digits; empty = 0).
    let chain_class = |chain: &str| -> i32 {
        c.building_chains.iter().find(|x| x.key == chain).and_then(|x| x.class_number.as_deref()).and_then(|s| s.parse().ok()).unwrap_or(0)
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
        r.technologies.insert(t.key.clone(), TechRules { cost: t.research_cost, building_level: t.building_level.clone(), requires: Vec::new() });
    }
    for u in c.unit_techs.iter() {
        r.unit_techs.entry(u.unit.clone()).or_default().push(u.technology.clone());
    }
    for b in c.building_techs.iter() {
        r.building_techs.entry(b.building.clone()).or_default().push(b.technology.clone());
    }
    for g in c.government_relations.iter() {
        r.government_relations.insert((g.government.clone(), g.other.clone()), (g.limit, g.value));
    }
    for t in c.attitude_thresholds.iter() {
        r.attitude_thresholds.insert(t.key.clone(), t.value);
    }
    for f in db.factions.iter() {
        r.faction_subcultures.insert(f.key.clone(), f.subculture.clone());
        // The faction's culture: `cultures_subcultures` of its subculture (sc_european_west →
        // european, sc_egy_european → egy_european), the keys `building_culture_variants` uses.
        if let Some(s) = c.characters.subcultures.get(&f.subculture) {
            r.faction_cultures.insert(f.key.clone(), s.culture.clone());
        }
    }
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
        let category = unit.map_or("", |u| u.category.as_str());
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
                range: match category {
                    "naval_line_of_battle" => 3,
                    "naval_frigate" | "naval_galley" => 2,
                    _ => 1,
                },
                bombard: matches!(class, "naval_bomb_ketch" | "naval_rocket_ship"),
                merchant: category == "naval_merchant",
            },
        );
    }
    for ch in c.chains.iter() {
        r.chain_kinds.insert(ch.key.clone(), ch.kind.as_deref().and_then(|s| s.trim().parse().ok()).unwrap_or(0));
    }
    for q in c.tech_requirements.iter() {
        if let Some(t) = r.technologies.get_mut(&q.technology) {
            t.requires.push(q.required.clone());
        }
    }
    for g in c.government_types.iter() {
        r.government_classes.insert(g.key.clone(), (g.upper_class.clone(), g.lower_class.clone()));
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
        if let Some(rel) = a.unknown_50.as_ref().filter(|s| !s.is_empty()) {
            r.agent_religions.insert(a.key.clone(), rel.clone());
        }
        r.agent_sight.insert(a.key.clone(), a.unknown_1c);
    }
    for p in c.unit_factions.iter() {
        let list = r.unit_factions.entry(p.unit.clone()).or_default();
        if p.allowed {
            list.push(p.faction.clone());
        }
    }
    for f in &db.factions {
        r.faction_categories.insert(f.key.clone(), f.category.clone());
    }
    r.effects = effect_rules_from_db(db);
    r.characters = character_rules_from_db(db);
    r
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
    for m in &t.bonus_agent {
        map(&m.effect, BonusKind::Agent, &m.bonus, &m.agent);
    }
    for m in &t.bonus_religion {
        map(&m.effect, BonusKind::Religion, &m.bonus, &m.religion);
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
        u.is_cavalry = record.category == "cavalry" || mounted;
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
        category: match record.category.as_str() {
            "cavalry" => 0,
            "artillery" => 1,
            "infantry" => 2,
            "dragoons" => 3,
            "elephants" => 4,
            "cavalry_camels" => 5,
            _ => 6,
        },
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
