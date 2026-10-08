//! Integration tests against a real install (read-only). Run with:
//! `cargo test -p ntw_data -- --ignored --nocapture`.
//! Override the location with the `NTW_DATA_DIR` environment variable.

use std::path::PathBuf;

use ntw_data::kv::{KvRules, KvTable, exe_truncate};
use ntw_data::{GameDatabase, Table};
use ntw_formats::pack::Vfs;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

/// Every implemented table loads with no leftover bytes (`DbTable::read` rejects any),
/// and the row counts and versions match Worker 2's `db_list.tsv`.
#[test]
#[ignore]
fn loads_every_table_exactly() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    fn check<T: ntw_data::DbRecord>(t: &Table<T>, version: u32, rows: usize) {
        println!("{:<26} v{} {:>4} rows", T::TABLE, t.version(), t.len());
        assert_eq!((t.version(), t.len()), (version, rows), "{}", T::TABLE);
    }
    check(&db.units, 4, 442);
    check(&db.unit_stats_land, 5, 328);
    check(&db.projectiles, 1, 144);
    check(&db.gun_type_to_projectiles, 0, 153);
    check(&db.factions, 3, 77);
    check(&db.regions, 1, 159);
    check(&db.building_levels, 0, 137);
    check(&db.technologies, 1, 68);
    check(&db.battle_climate_weather, 0, 660);
    check(&db.unit_stats_land_experience_bonuses, 0, 10);
    check(&db.unit_stats_naval_experience_bonuses, 0, 10);
    assert_eq!(db.kv_rules.table.entries().len(), 97);
    assert_eq!(db.kv_morale_raw.entries().len(), 69);
    assert_eq!(db.kv_fatigue_raw.entries().len(), 36);
}

/// Spot checks against Worker 2's sample values, following the foreign keys.
#[test]
#[ignore]
fn austrian_fusiliers_and_foreign_keys() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    let u = db.land_unit("Inf_Line_Austrian_German_Fusiliers").unwrap();
    println!("{:#?}", (u.unit.dev_name.as_str(), u.unit.recruitment_cost, u.unit.upkeep));
    println!(
        "men {} accuracy {} reload {} melee {}/{}/{} morale {} spacing {}x{} projectile {:?}",
        u.stats.num_men,
        u.stats.accuracy,
        u.stats.reload_skill,
        u.stats.melee_attack,
        u.stats.charge_bonus,
        u.stats.melee_defence,
        u.stats.morale,
        u.stats.spacing_file_close,
        u.stats.spacing_rank_close,
        u.stats.projectile
    );
    assert_eq!((u.stats.num_men, u.stats.accuracy, u.stats.melee_attack), (160, 40, 6));
    assert_eq!((u.stats.charge_bonus, u.stats.melee_defence, u.stats.morale), (10, 6, 6));
    assert_eq!(u.unit.recruitment_cost, 700);
    let p = db.primary_projectile(u.stats).expect("fusiliers fire a projectile");
    assert_eq!(p.key, "musket_flintlock");
    assert_eq!(p.effective_range, 80);
    assert_eq!(p.weapon_family.as_deref(), Some("musket_flintlock"));

    let art = db.land_unit("Art_Foot_French_12_lber").unwrap();
    let shots: Vec<&str> = db.gun_projectiles(art.stats).iter().map(|p| p.key.as_str()).collect();
    println!("12-lber fires {shots:?}");
    assert!(shots.contains(&"cannon_12_pounder_shot"));

    // Every stats row points at a unit, and every named projectile exists.
    for s in &db.unit_stats_land {
        assert!(db.unit(&s.key).is_some(), "{} has no units row", s.key);
        if let Some(p) = &s.projectile {
            assert!(db.projectile(p).is_some(), "{}: projectile {p} missing", s.key);
        }
    }
}

/// Campaign tables decode to plausible values (W3 examples).
#[test]
#[ignore]
fn campaign_tables() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    let fr = db.faction("france").unwrap();
    println!("france: {:?} {:?} {:?}", fr.primary_colour(), fr.secondary_colour(), fr.tertiary_colour());
    assert_eq!(fr.primary_colour(), [9, 79, 150]);
    assert_eq!(fr.secondary_colour(), [228, 180, 9]);
    assert_eq!(fr.tertiary_colour(), [49, 65, 100]);
    assert_eq!(fr.primary_r, fr.primary_copy_r);
    assert_eq!(fr.language_code, "Fr");

    let r = db.region("eur_france").unwrap();
    assert_eq!((r.continent.as_str(), r.colour(), r.battle_name.as_str()), ("cont_europe", [77, 109, 139], "France"));

    let b = db.building_level("fFort2_western_artillery_fort").unwrap();
    assert_eq!((b.chain.as_str(), b.level, b.construction_turns, b.cost), ("fFort", 1, 6, 8000));
    assert_eq!(b.prestige_military, 10);

    let t = db.technology("admin1_classical_economics").unwrap();
    println!("{t:?}");
    assert_eq!(t.key, t.text_key);
}

/// The kv truncation: integer keys are cut toward zero, the float keys stay floats.
#[test]
#[ignore]
fn kv_truncation() {
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let db = GameDatabase::from_vfs(&vfs).unwrap();

    let mut fractional = 0;
    for (key, raw) in db.kv_morale_raw.entries() {
        if raw.fract() != 0.0 {
            fractional += 1;
            println!("_kv_morale {key} = {raw} -> {}", exe_truncate(*raw));
        }
    }
    println!("_kv_morale: {fractional} fractional values truncated");
    assert_eq!(db.kv_morale.morale_base, exe_truncate(db.kv_morale_raw.raw("morale_base").unwrap()));
    assert_eq!(db.kv_fatigue.threshold_max, exe_truncate(db.kv_fatigue_raw.raw("threshold_max").unwrap()));

    for key in ["misfire_musket_flintlock", "melee_height_delta_min", "attackpower_long_range_multiplier"] {
        let f = db.kv_rules.float(key).unwrap();
        println!("_kv_rules {key} (float) = {f}");
        assert_eq!(Some(f), db.kv_rules.table.raw(key));
    }
    let raw_rules = KvTable::from_bytes("_kv_rules", &vfs.read("db/_kv_rules_tables/_kv_rules").unwrap()).unwrap();
    let rules = KvRules { table: raw_rules };
    let mut changed = 0;
    for (key, raw) in rules.table.entries() {
        if let Some(i) = rules.int(key) {
            assert_eq!(i, exe_truncate(*raw));
            if i as f32 != *raw {
                changed += 1;
                println!("_kv_rules {key} = {raw} -> {i}");
            }
        }
    }
    println!("_kv_rules: {changed} integer keys changed by truncation");
    let unread: Vec<&str> = rules
        .table
        .entries()
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| KvRules::key_type(k).is_none())
        .collect();
    println!("_kv_rules keys the exe never reads: {unread:?}");
}

/// `_kv_rules` converts into the simulation's `KvRules`, with non-zero divisors.
#[test]
#[ignore]
fn kv_rules_for_the_simulation() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    let r = &db.kv_rules_sim;
    println!(
        "armour melee/missile divisors {}/{}, defense melee {}, misfire flintlock {}, long-range x{}",
        r.armour_melee_piercing_divisor,
        r.armour_missile_piercing_divisor,
        r.defense_melee_piercing_divisor,
        r.misfire_musket_flintlock,
        r.attackpower_long_range_multiplier
    );
    assert_ne!(r.armour_melee_piercing_divisor, 0);
    assert_ne!(r.armour_missile_piercing_divisor, 0);
    assert_ne!(r.defense_melee_piercing_divisor, 0);
    assert_eq!(r.hnbonus_bayonet, db.kv_rules.int("hnbonus_bayonet").unwrap());
    assert_eq!(r.melee_height_delta_min, db.kv_rules.float("melee_height_delta_min").unwrap());
    // W1 to confirm: truncated to 0 by the exe's int rule.
    assert_eq!((r.ship_bonus_range, r.ship_penalty_range), (0, 0));
}

/// The unit card's capability block for DB-built units (BATTLE_FIDELITY.md §22): the French line
/// fusiliers get square formation and fire-and-advance from `unit_to_unit_abilities_junctions` and
/// the class's `fire_volley` drill.
#[test]
#[ignore]
fn unit_capabilities_from_the_ability_junctions() {
    use ntw_sim::battle::attributes::ability;
    let db = GameDatabase::from_install(data_dir()).unwrap();
    assert_eq!(db.unit_abilities.len(), 838);
    let c = db.unit_capabilities("Inf_Line_French_Fusiliers", "infantry_line");
    assert!(c.has_ability(ability::SQUARE_FORMATION) && c.has_ability(ability::FIRE_AND_ADVANCE));
    assert!(!c.has_ability(ability::PLUG_BAYONETS));
    assert_eq!(c.firing_drill, 0);
    // Technologies: fire and advance needs the technology whose effect enables it.
    assert_eq!((db.technology_effects.len(), db.effect_abilities.len(), db.effect_shot_types.len()), (106, 16, 9));
    let none = db.unit_capabilities_with("Inf_Line_French_Fusiliers", "infantry_line", Some(&[]));
    let gated: Vec<(&str, &str)> = db.effect_abilities.iter().map(|r| (r.effect.as_str(), r.ability.as_str())).collect();
    // No technology has `enable_square_formation`, so square formation is not gated.
    assert!(none.has_ability(ability::SQUARE_FORMATION) && !none.has_ability(ability::FIRE_AND_ADVANCE), "{gated:?}");
    let tech: Vec<String> =
        db.technology_effects.iter().filter(|r| r.effect == "enable_fire_and_advance").map(|r| r.technology.clone()).collect();
    assert!(!tech.is_empty());
    let with = db.unit_capabilities_with("Inf_Line_French_Fusiliers", "infantry_line", Some(&tech));
    assert!(with.has_ability(ability::FIRE_AND_ADVANCE));
    assert!(db.shot_type_needs_technology("carcass") && db.shot_type_needs_technology("quicklime"));
    assert!(!db.shot_type_needs_technology("canister") && !db.shot_type_needs_technology("round_shot"));
    let enablers: Vec<(&str, &str)> = db
        .technology_effects
        .iter()
        .filter(|r| r.effect.starts_with("enable_"))
        .map(|r| (r.technology.as_str(), r.effect.as_str()))
        .collect();
    println!("technology enablers: {enablers:?}");
}

#[test]
#[ignore = "needs the game installed"]
fn character_tables() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    let c = &db.campaign.characters;
    let n = [
        c.traits.len(),
        c.trait_info.len(),
        c.antitraits.len(),
        c.trait_agents.len(),
        c.ancillaries.len(),
        c.ancillary_agents.len(),
        c.ancillary_excluded.len(),
        c.ancillary_subcultures.len(),
        c.subcultures.len(),
    ];
    assert_eq!(n, [163, 163, 93, 179, 275, 333, 238, 1235, 16]);
    let t = c.traits.get("C_Admiral_Attacker_Bad").unwrap();
    assert_eq!((t.no_going_back_level, t.priority, t.category.as_str()), (3, 2, "Naval"));
    let a = c.ancillaries.get("Anc_Banker").unwrap();
    assert_eq!((a.kind.as_str(), a.faction_unique, a.priority, a.start_year, a.end_year), ("character", true, 5, 1796, 1900));
    assert_eq!(c.subcultures.get("sc_european_east").unwrap().culture, "european");
}

/// Every agent attribute's picture (`CharacterTables::attribute_icon`) is either the shipped
/// `PLACEHOLDER` or a file the install has (the `Pips` pictures are loose files, not in a pack).
#[test]
#[ignore = "needs the game installed"]
fn agent_attribute_icons_exist() {
    let dir = data_dir();
    let db = GameDatabase::from_install(&dir).unwrap();
    let vfs = Vfs::open_install(&dir).unwrap();
    let c = &db.campaign.characters;
    assert_eq!(c.attributes.len(), 14);
    assert_eq!(c.attribute_icon("subterfuge"), "data/ui/campaign ui/pips/skill_spying.tga");
    assert_eq!(c.attribute_icon("no_such_attribute"), "");
    let mut pictures = 0;
    for r in &c.attributes {
        let icon = c.attribute_icon(&r.key);
        if icon == "PLACEHOLDER" {
            continue;
        }
        let rel = icon.strip_prefix("data/").unwrap_or(icon);
        let found = vfs.contains(rel) || dir.join(rel.replace('\\', "/")).is_file();
        assert!(found, "{} -> {icon} is not in the install", r.key);
        pictures += 1;
    }
    assert_eq!(pictures, 8);
    // The attributes the campaign model reads all have a row.
    for key in ntw_sim::campaign::agents::ATTRIBUTES {
        assert!(c.attributes.get(key).is_some(), "{key}");
    }
}

#[test]
#[ignore]
fn experience_bonus_table() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    // CONFIRMED reader 0x00670F40: the exe's fatigue path indexes column 6 (+0x20) by row position,
    // so the file order must be rank 0..9 and these are the values it adds per tick.
    let rows: Vec<_> = db.unit_stats_land_experience_bonuses.iter().map(|r| r.rank.clone()).collect();
    assert_eq!(rows, (0..10).map(|i| i.to_string()).collect::<Vec<_>>());
    assert_eq!(db.experience_fatigue_bonuses(), vec![0, 0, 0, 0, 0, -1, -1, -2, -2, -3]);
    // Rank 9 in full: the two other CONFIRMED columns (+0x24 flat, +0x28 multiplier).
    let r9 = db.experience_bonuses(9).expect("rank 9 exists");
    assert_eq!((r9.unknown_0c, r9.unknown_10, r9.unknown_14, r9.unknown_18, r9.unknown_1c), (7, 7, 18, 6, 18));
    assert_eq!((r9.unknown_24, r9.unknown_28), (360, 1.9));
    let r0 = db.experience_bonuses(0).expect("rank 0 exists");
    assert_eq!((r0.unknown_0c, r0.unknown_24, r0.unknown_28), (0, 0, 1.0));
}

/// The naval twin of the table above, `unit_stats_naval_experience_bonuses` (getter `0x00E31710`).
/// The 7-column layout of DB_BUILDERS.md is CONFIRMED by decoding the real file with no leftover
/// bytes; `0x00ED49A0`'s naval branch reads `+0x1C` (flat) and `+0x20` (multiplier) of the row the
/// experience rank names.
#[test]
#[ignore]
fn naval_experience_bonus_table() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    let ranks: Vec<String> = db.unit_stats_naval_experience_bonuses.iter().map(|r| r.rank.clone()).collect();
    println!("naval ranks {ranks:?}");
    for r in db.unit_stats_naval_experience_bonuses.iter() {
        println!(
            "rank {}: +0x0C {} +0x10 {} +0x14 {} +0x18 {} | flat +0x1C {} x{:.2}",
            r.rank, r.unknown_0c, r.unknown_10, r.unknown_14, r.unknown_18, r.unknown_1c, r.unknown_20
        );
    }
    assert_eq!(db.unit_stats_naval_experience_bonuses.len(), 10);
    // A rank-0 recruit costs what it costs (the multiplier is 1.0 and the flat term 0); a veteran
    // costs far more, which is the whole point of the pair.
    let r0 = db.naval_experience_bonuses(0).expect("rank 0 exists");
    let r9 = db.naval_experience_bonuses(9).expect("rank 9 exists");
    assert_eq!(db.naval_experience_adjusted_cost(0, 100), 100);
    // rank 9: flat 255 + ROUND(100 * 1.45) = 255 + 145.
    assert_eq!(db.naval_experience_adjusted_cost(9, 100), 400);
    // An unknown rank (or no table at all) leaves the cost alone, as the exe does.
    assert_eq!(db.naval_experience_adjusted_cost(42, 100), 100);
    // The columns with no reader yet, which the decode confirms exist: +0x0C is the rank itself,
    // +0x14 is 2 × rank and +0x18 is rank / 2.
    assert_eq!(r9.unknown_0c, 9);
    assert_eq!(r9.unknown_14, 18);
    assert_eq!(r9.unknown_18, 5);
    assert_eq!((r0.unknown_0c, r0.unknown_14, r0.unknown_18), (0, 0, 0));
    // The land table's same formula (0x00ED49A0, land branch): rank 9 = 360 + ROUND(100 * 1.9).
    assert_eq!(db.experience_adjusted_cost(0, 100), 100);
    assert_eq!(db.experience_adjusted_cost(9, 100), 550);
    // Every rank 0..9 exists and the cost grows with it (the shipped rows are flat 0 / x1.0 at
    // rank 0 and rising), which is the head-to-head comparison rule the campaign spends against.
    let land_rows = db.experience_cost_rows();
    let naval_rows = db.naval_experience_cost_rows();
    assert_eq!(land_rows.len(), 10);
    assert_eq!(naval_rows.len(), 10);
    assert_eq!(land_rows.iter().map(|r| r.0).collect::<Vec<_>>(), (0..10).collect::<Vec<u8>>());
    assert_eq!(naval_rows.iter().map(|r| r.0).collect::<Vec<_>>(), (0..10).collect::<Vec<u8>>());
    let mut prev_land = 0;
    let mut prev_naval = 0;
    for rank in 0..=9u8 {
        let land = db.experience_adjusted_cost(rank, 100);
        let naval = db.naval_experience_adjusted_cost(rank, 100);
        assert!(land >= prev_land, "rank {rank}: {land} < {prev_land}");
        assert!(naval >= prev_naval, "rank {rank}: {naval} < {prev_naval}");
        if rank == 0 {
            // A recruit pays exactly its base cost (flat 0, x1.00 in both tables).
            assert_eq!((land, naval), (100, 100));
        } else {
            assert!(land > 100, "rank {rank}: a veteran costs more than a recruit ({land})");
            assert!(naval > 100, "rank {rank}: a veteran ship costs more than a recruit ({naval})");
        }
        prev_land = land;
        prev_naval = naval;
    }
    // Outside 0..9 there is no row, so the cost is untouched.
    assert_eq!(db.experience_adjusted_cost(10, 100), 100);
    assert_eq!(db.naval_experience_adjusted_cost(255, 100), 100);
    println!("rank 0 flat {} x{:.2}, rank 9 flat {} x{:.2}", r0.unknown_1c, r0.unknown_20, r9.unknown_1c, r9.unknown_20);
}

/// The campaign-battle weather pick (`0x00F5B4D0`) on the real table: a temperate summer draw.
#[test]
#[ignore]
fn climate_weather_pick_on_the_real_table() {
    let db = GameDatabase::from_install(data_dir()).unwrap();
    let rows: Vec<_> = db.battle_climate_weather.iter().cloned().collect();
    let climate = rows[0].climate.clone();
    // r = 0 picks the first weighted row of the climate's summer.
    let p = ntw_data::weather::pick_climate_weather(&rows, &climate, "season_summer", None, || 0.0).unwrap();
    assert_eq!(p.row.climate, climate);
    assert!(p.row.weight > 0);
    println!("{} → {} (flag {})", climate, p.row.weather, p.flag);
}
