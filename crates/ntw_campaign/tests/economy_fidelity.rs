//! The economy model against figures the original stored (read-only; skips without the install).
//!
//! Every shipped start position holds, per faction, an `ECONOMICS_DATA` record whose category 11
//! (`#2[3]`) is the faction's other income as the original computed it (`faction_gdp_other` for a
//! major power, `faction_gdp_other_minor` otherwise; 0x00BBC710, CAMPAIGN_FIDELITY.md). The model
//! must give the same value for every faction of every campaign.

use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_sim::campaign::economy;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

#[test]
fn other_income_matches_the_startpos_records() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let mut checked = 0;
    let mut exact_trade = 0;
    // Not mp_eur_napoleon: its startpos stores 1500 (the DB base value of faction_gdp_other_minor) for
    // the minors although that campaign overrides it to 1700; the record is stale build-time data
    // (the exe recomputes the category while loading a faction, 0x0087A190 -> 0x00BBC710).
    for campaign in [
        "eur_napoleon",
        "egy_napoleon",
        "mp_egy_napoleon",
        "ita_napoleon",
        "mp_ita_napoleon",
        "spa_napoleon",
        "tut_napoleon",
    ] {
        let path = dir.join(format!(r"campaigns\{campaign}\startpos.esf"));
        if !path.is_file() {
            continue;
        }
        let loaded = ntw_campaign::read_file(&path, &db).expect("startpos loads");
        let esf = EsfFile::open(&path).expect("startpos");
        let arr = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY").expect("factions");
        for item in &arr.items {
            let Some(f) = item.first().and_then(EsfNode::as_record) else { continue };
            let key = f.values().find_map(EsfNode::as_str).unwrap_or_default();
            let Some(fac) = loaded.model.world.factions.values().find(|x| x.key == key) else { continue };
            let record = f
                .child("FACTION_ECONOMICS")
                .and_then(|e| e.get(0))
                .and_then(EsfNode::as_record_array)
                .and_then(|h| h.items.last())
                .and_then(|it| it.first())
                .and_then(EsfNode::as_record);
            let category = |group: usize, i: usize| {
                record.and_then(|d| d.get(group)).and_then(EsfNode::as_i32_array).and_then(|a| a.get(i).copied())
            };
            let Some(stored) = category(2, 3) else { continue };
            let income = economy::faction_income(&loaded.model, fac.id);
            assert_eq!(income.other, stored, "{campaign} {key}: other income");
            // Trade (category 7 = #1[2]): routes (GDP + commodity parts) plus commodities sold at home.
            let stored_trade = category(1, 2).unwrap_or(0);
            if income.trade == stored_trade {
                exact_trade += 1;
            } else {
                println!("{campaign} {key}: trade {} stored {stored_trade}", income.trade);
            }
            checked += 1;
        }
    }
    println!("checked {checked} factions, trade computed exactly for {exact_trade}");
    assert!(checked > 20);
    assert_eq!(exact_trade, checked, "trade must match for every faction");
}

/// Every shipped start position stores each region's GDP (`REGION` #10) and town wealth growth
/// (#15) as the original computed them (0x00A6AFC0). The recomputation must give the same values
/// (the growth includes both classes' `tw_growth_taxes_*` effects: -0.3 and -3 for the upper,
/// -0.25 and -2 for the lower classes at `tax_normal`).
#[test]
fn gdp_and_growth_match_the_startpos_records() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let mut checked = 0;
    for campaign in ["eur_napoleon", "mp_eur_napoleon", "egy_napoleon", "ita_napoleon", "spa_napoleon", "tut_napoleon"] {
        let path = dir.join(format!(r"campaigns\{campaign}\startpos.esf"));
        if !path.is_file() {
            continue;
        }
        let loaded = ntw_campaign::read_file(&path, &db).expect("startpos loads");
        let m = &loaded.model;
        for r in m.world.regions.values().filter(|r| m.world.factions.contains_key(&r.owner)) {
            let (gdp, growth) = economy::recompute_region(m, r);
            assert_eq!(gdp, r.gdp, "{campaign} {}: GDP", r.key);
            assert_eq!(
                growth,
                r.town_wealth_growth,
                "{campaign} {}: town wealth growth (offset {}, discontent {}, buildings {:?})",
                r.key,
                r.wealth_growth_offset,
                r.discontent_growth,
                r.buildings().map(|b| (b.level_key.clone(), b.health)).collect::<Vec<_>>()
            );
            checked += 1;
        }
    }
    println!("checked {checked} regions");
    assert!(checked > 200);
}

/// `diplomatic_relations_government_type` is keyed on the **pair** of governments (0-B round 14,
/// item 2: the row key round 13 left open). `record_index` takes one string, but it is the composite
/// `own + SEP + target` the exe splices before the call, and the shipped rows say the same: the
/// table is 4 x 4 and both #2 and #3 take several different values inside a single column, so
/// neither is a function of one government.
///
/// Locks three things: the 4 x 4 shape, that #2 (the shocked value, `0x00B1B5A0`) and #3 (the
/// steady value / limit, `0x00B45A40`) are not functions of one column, and that the values the
/// model uses are the shipped ones (absolute monarchy towards a republic: #2 -100, #3 -30).
#[test]
fn government_relations_are_keyed_on_the_pair() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).expect("vfs");
    let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs };
    let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let m = ntw_campaign::read_esf(&esf, &db).expect("loads").model;
    let rows = &m.rules.government_relations;
    let govs: Vec<&String> = rows.keys().map(|(a, _)| a).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    assert_eq!(rows.len(), 16, "4 government types x 4");
    assert_eq!(govs.len(), 4);
    // Every ordered pair is present.
    for a in &govs {
        for b in &govs {
            assert!(rows.contains_key(&((*a).clone(), (*b).clone())), "missing row {a} -> {b}");
        }
    }
    // Not a function of the owner, not of the target: every column must take several #2 / #3 values.
    for (label, idx) in [("own", 0), ("target", 1)] {
        let mut per_col: std::collections::BTreeMap<&String, (std::collections::BTreeSet<i32>, std::collections::BTreeSet<i32>)> = Default::default();
        for ((a, b), (v2, v3)) in rows {
            let e = per_col.entry(if idx == 0 { a } else { b }).or_default();
            e.0.insert(*v2);
            e.1.insert(*v3);
        }
        for (g, (v2, v3)) in per_col {
            // gov_empire is the catch-all row (all zeros); every other government discriminates.
            assert!(g == "gov_empire" || (v2.len() > 1 && v3.len() > 1), "f({label} = {g}) is constant: #2 {v2:?} #3 {v3:?}");
        }
    }
    assert_eq!(rows[&("gov_absolute_monarchy".into(), "gov_republic".into())], (-100, -30));
    assert_eq!(rows[&("gov_republic".into(), "gov_absolute_monarchy".into())], (-140, -25));
}

/// The region GDP / town wealth growth recompute (0x00A6AFC0) against **every** region of every
/// vanilla save the original wrote (read-only evidence copies, `NTW_EVIDENCE_DIR`), with the
/// faction-wide effects the round end reads.
///
/// Three regions were the last mismatches (0-B round 14, CONFIRMED from the stored values alone):
/// - `eur_moravia` (Austria) and `eur_wallachia` (the Ottomans) are **tax-exempt** (`REGION` #19),
///   so they take no `taxes_effects_jct` bundle at all: their stored growth is the raw sum, while the
///   `tax_normal` bundles (−0.55 and −5 over both classes) would have subtracted.
/// - `eur_bavaria` is owned by Austria but governed by the landless Bavaria, so it reads
///   **Bavaria's** faction-wide sum (no technologies, `gdp_mod_all` 0) and not Austria's
///   (`tw_growth_technologies` 5, `gdp_mod_all` 3), while every building in its slots counts.
#[test]
fn gdp_and_growth_match_the_vanilla_saves() {
    let dir = data_dir();
    let ev = std::env::var_os("NTW_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(r"Documents\ntw-evidence\saves"));
    if !dir.is_dir() || !ev.is_dir() {
        println!("SKIP: no install at {} or no evidence saves at {}", dir.display(), ev.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let fx_kind = ntw_sim::campaign::effects::Effects::compute;
    let mut checked = 0;
    let mut seen = std::collections::BTreeSet::new();
    for name in [
        "auto_nr4_t4",
        "auto_after_c8",
        "orig_over_nr4_0252",
        "auto_b2b3_0211",
        "auto_nr1",
        "orig_fr_t1",
        "orig_fr_t1_b",
        "orig_fr_may1811",
        "auto_orig_spa_0245",
    ] {
        let path = ev.join(format!("{name}.save"));
        if !path.is_file() {
            continue;
        }
        let m = ntw_campaign::read_file(&path, &db).expect("save loads").model;
        let fx = fx_kind(&m);
        for r in m.world.regions.values().filter(|r| m.world.factions.get(&r.owner).is_some_and(|f| !f.key.is_empty())) {
            let (gdp, growth) = economy::recompute_region_with(&m, Some(&fx), r);
            assert_eq!(gdp, r.gdp, "{name} {}: GDP", r.key);
            assert_eq!(growth, r.town_wealth_growth, "{name} {}: town wealth growth", r.key);
            checked += 1;
            for k in ["eur_moravia", "eur_wallachia", "eur_bavaria"] {
                if r.key == k {
                    seen.insert(k.to_string());
                }
            }
        }
    }
    println!("checked {checked} region records, the three former misses seen: {}", seen.len());
    assert!(checked > 400, "{checked}");
    // eur_bavaria only exists as a governed region of austria in the later saves; the other two are
    // in every eur save. Require all three so this test cannot pass vacuously.
    assert_eq!(seen.len(), 3, "the three region-growth misses must all be covered");
}

/// An independent re-implementation of the region recompute's two contested inputs, so the model
/// can be checked on data shapes the nine vanilla saves never contain.
///
/// Round 14 fixed two things in `economy::recompute_region_with`, both found only because eur has a
/// tax-exempt region (`eur_moravia`, `eur_wallachia`) and one region whose owner is not its governor
/// (`eur_bavaria`). The evidence that would have caught a **third** shape is absent from every
/// shipped file, and `ECON_TAXEX=1` confirms how thin it is: across the six start positions and the
/// nine vanilla saves there are **2** tax-exempt regions and **1** governed-by-another region, and
/// the four spa saves and four of the five eur saves have neither. So instead of three more one-off
/// regions this test states the two rules as properties over *every* region, and checks them on
/// **synthetically re-shaped** models: it clones a loaded model, then for each region
///
/// - forces `tax_exempt` on and off (`REGION` #19), and
/// - rewrites the governorship listings so the region's **governor is a chosen faction other than
///   its owner** (the `eur_bavaria` shape, applied to every region and every faction pair),
///
/// and requires the model's GDP and growth to equal this reference. The reference takes the
/// governing faction and the exempt flag as *parameters* rather than reading them off the region,
/// so an implementation that looked at `region.owner`, or that ignored `tax_exempt` for a region
/// other than the two eur ones, cannot pass.
///
/// The reference is deliberately written from the doc comment of `recompute_region_with`
/// (`CAMPAIGN_FIDELITY.md` §GDP) rather than by calling into `economy`, and it reuses the two public
/// building-effect readers (`economy::building_effect`, `economy::SLOT_GDP_EFFECTS` /
/// `SLOT_TW_EFFECTS`) plus the effects store, which are the parts both implementations share.
fn reference_region(
    model: &ntw_sim::campaign::CampaignModel,
    fx: &ntw_sim::campaign::effects::Effects,
    region: &ntw_sim::campaign::world::Region,
    // The faction whose government, tax levels and faction-wide sum the region reads. Passed in,
    // not looked up: that is the whole point of the property.
    governor: ntw_sim::campaign::FactionId,
    // `REGION` #19.
    exempt: bool,
) -> (u32, i32) {
    use ntw_sim::campaign::effects::{BonusKind, EffectSet};
    use ntw_sim::campaign::economy::{SLOT_GDP_EFFECTS, SLOT_TW_EFFECTS, building_effect};
    use ntw_sim::campaign::TaxClass;
    let rules = &model.rules;
    let gov = model.world.factions.get(&governor);
    let list_sum = |l: Option<&Vec<(String, f32)>>, key: &str| {
        l.map_or(0.0, |l| l.iter().filter(|(k, _)| k == key).map(|(_, v)| *v).sum::<f32>())
    };
    // region_effect: the region's own buildings + the governing faction's government + its tax
    // levels (both classes) unless the region is exempt.
    let region_effect = |key: &str| -> f32 {
        let mut v: f32 = region.buildings().map(|b| building_effect(rules, &b.level_key, key)).sum();
        if let Some(f) = gov {
            v += list_sum(rules.government_effects.get(&f.government_key), key);
            if !exempt {
                for (class, level) in [(TaxClass::Lower, &f.tax_lower), (TaxClass::Upper, &f.tax_upper)] {
                    v += list_sum(rules.effects.tax_rows(class.key(), level), key);
                }
            }
        }
        v
    };
    // faction_part: the governing faction's own sum, minus the government part region_effect has
    // already counted.
    let faction_part = |key: &str| -> f32 {
        let g = gov.and_then(|f| rules.effects.government().get(&f.government_key)).map_or(0.0, |s: &EffectSet| s.get(key));
        fx.faction(governor, key) - g
    };
    let gdp_factor = (region_effect("gdp_mod_all") + faction_part("gdp_mod_all")) * 0.01 + 1.0;
    let tw_factor = (region_effect("tw_growth_mod_all") + faction_part("tw_growth_mod_all")) * 0.01 + 1.0;
    let chain_mod = |id: &str, chain: &str| -> f32 {
        let key = |s: &EffectSet| s.get_qualified(BonusKind::Saved(2), id, chain);
        let local: f32 = region.effect_buildings().filter_map(|b| rules.effects.building_local().get(&b.level_key)).map(key).sum();
        local + fx.faction_qualified(governor, BonusKind::Saved(2), id, chain)
    };
    let mut gdp = i64::from(region.base_gdp);
    let mut growth: i32 = (region_effect("tw_growth_industry_global") + faction_part("tw_growth_factionwide")).round_ties_even() as i32;
    growth = growth.saturating_add(region.discontent_growth);
    for b in region.buildings().filter(|b| b.health >= 100) {
        let chain = rules.buildings.get(&b.level_key).map_or("", |x| x.chain.as_str());
        let gdp_f = gdp_factor + chain_mod("1", chain) * 0.01;
        let tw_f = tw_factor + chain_mod("2", chain) * 0.01;
        for key in SLOT_GDP_EFFECTS {
            gdp += (building_effect(rules, &b.level_key, key).round_ties_even() as i32 as f32 * gdp_f) as i64;
        }
        for key in SLOT_TW_EFFECTS {
            growth = growth.saturating_add((building_effect(rules, &b.level_key, key).round_ties_even() as i32 as f32 * tw_f) as i32);
        }
    }
    growth = (growth as f32 + region_effect("tw_growth_technologies_fixed") + faction_part("tw_growth_technologies")) as i32;
    growth = (growth as f32 + faction_part("tw_growth_home_region")) as i32;
    let modifier = if growth >= 0 { region_effect("tw_growth_taxes_modifier") + faction_part("tw_growth_tax_modifier") } else { 0.0 };
    let fixed = region_effect("tw_growth_taxes_fixed") + faction_part("tw_growth_tax_modifier_fixed");
    let scaled = ((modifier - region.wealth_growth_offset as f32) * growth as f32) as i32;
    growth = (scaled.saturating_add(growth) as f32 + fixed) as i32;
    (gdp.clamp(0, i64::from(u32::MAX)) as u32, growth)
}

/// Rewrites every governorship listing so that `region` is governed by `governor` instead of its
/// owner: the `eur_bavaria` shape (owned by Austria, in Bavaria's governorship), applied to any
/// region and any faction pair.
///
/// Every other listing is removed first, because `World::governing_faction` gives the owner
/// priority when the owner lists the region itself, and `governing_factions` otherwise takes the
/// first faction in id order that lists it — so a leftover listing would make the case prove
/// nothing. A governorship is created for `governor` when it has none (`World::governing_faction`
/// only considers factions that hold the region in some post's `GOVERNORSHIP`). The synthetic post
/// carries no taxes: the tax levels the recompute reads live on the `Faction` record
/// (`tax_lower` / `tax_upper`), not on the governorship, so this changes only *who governs*.
fn set_governor(model: &mut ntw_sim::campaign::CampaignModel, region: ntw_sim::campaign::RegionId, governor: ntw_sim::campaign::FactionId) {
    use ntw_sim::campaign::{Governorship, GovernorshipTaxes, GovernmentPost};
    let ids: Vec<_> = model.world.faction_details.keys().copied().collect();
    for f in ids {
        let Some(d) = model.world.faction_details.get_mut(&f) else { continue };
        let mut held = false;
        for p in d.posts.iter_mut() {
            let Some(g) = p.governorship.as_mut() else { continue };
            g.regions.retain(|r| *r != region);
            if f == governor {
                g.regions.push(region);
                held = true;
            }
        }
        if f == governor && !held {
            d.posts.push(GovernmentPost {
                id: 0,
                key: "governor_europe".to_string(),
                holder: None,
                governorship: Some(Governorship {
                    taxes: GovernorshipTaxes { lower: 2, upper: 2, lower_rate: 15, upper_rate: 15 },
                    theatre_id: 0,
                    regions: vec![region],
                    faction: governor,
                    flags: (false, false),
                }),
            });
        }
    }
}

/// The size a recruited unit is created at (BACKLOG 0-B open item 7, the `num_men` sub-item that
/// `rules::men` carried as PROVISIONAL).
///
/// `spawn_recruited_unit` sizes a finished unit with `world::recruited_unit_size` (it used to size
/// a ship at whatever `max_men` another unit of the same key carried, a stand-in). The rule is
/// checked here against the units the original itself stored: **every** unit in all nine vanilla
/// saves carries `max_men` equal to
/// - `unit_stats_land.num_men` for a land unit, or
/// - the **sum of the `unit_stats_naval` crew triple** (c17/c18/c19, `rules::ships.crews`) for a
///   ship — `3_Decker_British_1st_Rate` 50+50+204 = 304, `Trade_Ship_Indiaman` 0+28+112 = 140,
///   `Small_Ottoman_Galley` 10+14+4 = 28, and so on for every class,
///
/// with no exception in 3174 units. `men` equals `max_men` on every unit except the ones that have
/// taken battle casualties (9 of 460 in `auto_nr4_t4`, 5 of 484 in `orig_over_nr4_0252`, 0 in the
/// four spa saves), which is the expected damage reading, so a fresh unit starts at its full size.
///
/// So the land rule and the crew sum for ships are CONFIRMED from data, not from the exe.
#[test]
fn recruited_unit_size_is_num_men_for_land_and_the_crew_sum_for_ships() {
    let dir = data_dir();
    let ev = std::env::var_os("NTW_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(r"Documents\ntw-evidence\saves"));
    if !dir.is_dir() || !ev.is_dir() {
        println!("SKIP: no install at {} or no evidence saves at {}", dir.display(), ev.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let mut checked = 0;
    let mut naval = 0;
    let mut damaged = 0;
    for name in [
        "auto_nr4_t4",
        "auto_after_c8",
        "orig_over_nr4_0252",
        "auto_b2b3_0211",
        "auto_nr1",
        "orig_fr_t1",
        "orig_fr_t1_b",
        "orig_fr_may1811",
        "auto_orig_spa_0245",
    ] {
        let path = ev.join(format!("{name}.save"));
        if !path.is_file() {
            continue;
        }
        let m = ntw_campaign::read_file(&path, &db).expect("save loads").model;
        for u in m.world.forces.values().flat_map(|f| f.units.iter()) {
            let is_naval = m.rules.units.get(&u.unit_key).is_some_and(|r| r.is_naval);
            if is_naval {
                naval += 1;
            }
            // The shipped rule, exercised rather than restated: `world::recruited_unit_size` is what
            // `spawn_recruited_unit` should size a finished unit at.
            let want = ntw_sim::campaign::world::recruited_unit_size(&m.rules, &u.unit_key);
            assert_ne!(want, 0, "{name} {}: the rule gives a size of 0, so it cannot be checked", u.unit_key);
            assert_eq!(u.max_men, want, "{name} {}: max_men (naval {is_naval})", u.unit_key);
            if u.men != u.max_men {
                damaged += 1;
                assert!(u.men < u.max_men, "{name} {}: men {} above max_men {}", u.unit_key, u.men, u.max_men);
            }
            checked += 1;
        }
    }
    println!("checked {checked} units ({naval} ships); {damaged} carry battle casualties below their full size");
    assert!(checked > 3000, "{checked} units");
    assert!(naval > 300, "{naval} ships, so the crew-sum rule is exercised across the classes");
}

/// The two round-14 rules as properties, checked on re-shaped models rather than on the two eur
/// regions that happened to expose them.
///
/// **Properties.**
/// 1. *Tax exemption is a per-region flag, not a place.* For every region of every shipped start
///    position and vanilla save, forcing `REGION` #19 on and off must give exactly the reference's
///    growth, and the two must differ whenever the governing faction's tax bundle is non-zero. A
///    rule keyed on the region's key (or one that only ever fired for `eur_moravia` /
///    `eur_wallachia`) fails here on a region that is neither.
/// 2. *The faction part is the governing faction's, not the owner's.* With the governorship
///    rewritten so the governor is an arbitrary faction `G != owner`, the model must equal the
///    reference parameterised by `G`. An implementation that read `region.owner` would instead
///    return the owner's value and fail every such pair.
/// 3. *The two compose.* A tax-exempt region governed by a faction other than its owner reads
///    `G`'s government and `G`'s faction-wide sum with **no** tax bundle — the shape no shipped
///    file has.
#[test]
fn tax_exemption_and_governorship_hold_for_every_region_and_faction_pair() {
    let dir = data_dir();
    let ev = std::env::var_os("NTW_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(r"Documents\ntw-evidence\saves"));
    if !dir.is_dir() || !ev.is_dir() {
        println!("SKIP: no install at {} or no evidence saves at {}", dir.display(), ev.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let mut files: Vec<PathBuf> = ["eur_napoleon", "egy_napoleon", "ita_napoleon", "spa_napoleon"]
        .iter()
        .map(|c| dir.join("campaigns").join(c).join("startpos.esf"))
        .filter(|p| p.is_file())
        .collect();
    for name in ["auto_nr4_t4", "auto_after_c8", "orig_over_nr4_0252", "orig_fr_t1", "auto_orig_spa_0245"] {
        let p = ev.join(format!("{name}.save"));
        if p.is_file() {
            files.push(p);
        }
    }
    let mut regions_seen = 0;
    let mut pairs = 0;
    let mut exempt_flips_that_mattered = 0;
    let mut governors_that_mattered = 0;
    let mut composed = 0;
    for path in &files {
        let base = ntw_campaign::read_file(path, &db).expect("file loads").model;
        let ids: Vec<_> = base.world.regions.keys().copied().collect();
        let gov_factions: Vec<_> = base.world.factions.keys().copied().filter(|f| !base.world.factions[f].key.is_empty()).collect();
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        // One effects store per file. Rewriting a governorship does not change any faction's set
        // (`faction_set` sums regions by `r.owner`); it only decides **which** faction the recompute
        // queries, which is the axis under test. That is asserted rather than assumed, so the reuse
        // below is checked and the sweep stays affordable.
        let fx0 = ntw_sim::campaign::effects::Effects::compute(&base);
        if let Some(rid) = ids.first().copied() {
            let owner = base.world.regions.get(&rid).expect("region").owner;
            let other = gov_factions.iter().copied().find(|g| *g != owner).unwrap_or(owner);
            let mut probe = base.clone();
            set_governor(&mut probe, rid, other);
            let fx1 = ntw_sim::campaign::effects::Effects::compute(&probe);
            for f in &gov_factions {
                for key in ["tw_growth_technologies", "gdp_mod_all", "tw_growth_mod_all"] {
                    assert_eq!(
                        fx1.faction(*f, key),
                        fx0.faction(*f, key),
                        "{name}: a governorship rewrite changed faction {f:?} {key}, so the store is not reusable"
                    );
                }
            }
        }
        for rid in ids {
            let owned = {
                let r = base.world.regions.get(&rid).expect("region");
                base.world.factions.get(&r.owner).is_some_and(|f| !f.key.is_empty())
            };
            if !owned {
                continue;
            }
            let owner = base.world.regions.get(&rid).expect("region").owner;
            let original_exempt = base.world.regions.get(&rid).expect("region").tax_exempt;
            // Every live faction as the governor, the owner first: the shipped shape plus the
            // `eur_bavaria` shape applied to every region. `set_governor` clears every other listing
            // first, so the cases are independent of each other.
            let mut taxed_growth: std::collections::BTreeMap<ntw_sim::campaign::FactionId, i32> = Default::default();
            let mut owner_shape: std::collections::BTreeMap<bool, (u32, i32)> = Default::default();
            // One clone per region, re-shaped in place: a `CampaignModel` clone is the dominant cost
            // of the sweep and each case overwrites the last one's edits anyway.
            let mut m = base.clone();
            for governor in [owner].into_iter().chain(gov_factions.iter().copied().filter(|g| *g != owner)) {
                set_governor(&mut m, rid, governor);
                // Checked, not assumed: if the rewrite did not take, the case proves nothing.
                assert_eq!(m.world.governing_faction(rid), Some(governor), "{name} region {rid:?}: the governorship rewrite did not take");
                for exempt in [false, true] {
                    // Property 1 and 2 together: build the shape, ask the model, ask the reference.
                    if let Some(r) = m.world.regions.get_mut(&rid) {
                        r.tax_exempt = exempt;
                    }
                    let r = m.world.regions.get(&rid).expect("region");
                    let got = economy::recompute_region_with(&m, Some(&fx0), r);
                    let want = reference_region(&m, &fx0, r, governor, exempt);
                    assert_eq!(got, want, "{name} {}: governor {governor:?} exempt {exempt}", r.key);
                    pairs += 1;
                    if exempt {
                        composed += usize::from(governor != owner);
                        // The flag must be load-bearing: the same shape with the flag off (computed
                        // above for this governor, `taxed_growth`) must give a different growth
                        // wherever the governing faction's bundle is non-zero.
                        if taxed_growth.get(&governor).is_some_and(|g| *g != got.1) {
                            exempt_flips_that_mattered += 1;
                        }
                    } else {
                        taxed_growth.insert(governor, got.1);
                    }
                    if governor != owner {
                        // ... and the governor must be load-bearing too: the owner's own shape (also
                        // computed above, `owner_shape`) must differ wherever the two factions do.
                        if owner_shape.get(&exempt).is_some_and(|s| *s != got) {
                            governors_that_mattered += 1;
                        }
                    } else {
                        owner_shape.insert(exempt, got);
                    }
                }
            }
            // Put the region back the way the file had it before the next one is swept.
            if let Some(r) = m.world.regions.get_mut(&rid) {
                r.tax_exempt = original_exempt;
            }
            regions_seen += 1;
        }
    }
    println!(
        "checked {pairs} (region, governor, exempt) shapes over {regions_seen} owned regions in {} files: \
         {exempt_flips_that_mattered} exemption flips changed the growth, {governors_that_mattered} foreign governors changed the answer, \
         {composed} tax-exempt-and-foreign-governor shapes",
        files.len()
    );
    // Guard against a vacuous pass: the properties must have been exercised where they bite.
    assert!(regions_seen > 200, "{regions_seen} owned regions");
    assert!(pairs > 1500, "{pairs} shapes");
    assert!(exempt_flips_that_mattered > regions_seen / 2, "{exempt_flips_that_mattered} flips mattered");
    assert!(governors_that_mattered > 0, "a foreign governor never changed the answer");
    assert!(composed > regions_seen, "{composed} composed shapes");
}

/// The commodities a faction's navies gather at the trade nodes, computed from its trade ships with
/// `trade_nodes` (0x00BC9930), equal the volumes its saved domestic routes bring home, and the
/// trade income stays exact with the computed supply.
#[test]
fn trade_node_supply_matches_the_domestic_routes() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).expect("vfs");
    let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs };
    let (mut checked, mut equal) = (0, 0);
    for campaign in ["eur_napoleon", "egy_napoleon", "ita_napoleon", "spa_napoleon"] {
        let Ok(bytes) = files.read(&format!("campaigns/{campaign}/startpos.esf")) else { continue };
        let esf = EsfFile::from_bytes(&bytes).expect("esf");
        let mut loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
        let map = ntw_formats::campaign_map::CampaignMap::load(&files, &loaded.info.map_key).expect("map");
        ntw_campaign::trade::attach_map(&mut loaded.model, &map.regions);
        let m = &loaded.model;
        let attached = m.world.trade_nodes.iter().filter(|n| n.info.is_some()).count();
        println!("{campaign}: {} nodes, {attached} with a DB row", m.world.trade_nodes.len());
        for f in m.world.factions.values() {
            // The start positions store figures computed before any effect applies (their upkeep and
            // taxes are the effect-free values). In `spa_napoleon` the node volume is scaled by the
            // faction's `trade_node_supply_mod`, which the first saved turn shows (spa_britain 37 → 39),
            // so the loaded supply is scaled here and the stale stored trade income is not compared.
            let spa_mod = if campaign == "spa_napoleon" {
                ntw_sim::campaign::effects::Effects::faction_sum(m, f.id).get("trade_node_supply_mod")
            } else {
                0.0
            };
            let loaded_supply: Vec<u32> = (0..m.world.commodity_prices.len())
                .map(|c| m.world.domestic_trade.get(&f.id).into_iter().flatten().map(|p| p.volumes.get(c).copied().unwrap_or(0)).sum())
                .map(|v: u32| ((spa_mod * 0.01 + 1.0) * v as f32) as u32)
                .collect();
            let Some(computed) = m.trade_supply(f.id) else { continue };
            if loaded_supply.iter().all(|v| *v == 0) && computed.iter().all(|v| *v == 0) {
                continue;
            }
            checked += 1;
            if computed == loaded_supply {
                equal += 1;
            } else {
                println!("  {} computed {computed:?} loaded {loaded_supply:?}", f.key);
            }
            let stored = m.world.faction_details.get(&f.id).and_then(|d| d.stored_trade_income);
            if spa_mod == 0.0 {
                assert_eq!(Some(ntw_sim::campaign::economy::trade_routes_value(m, f.id)), stored, "{campaign} {}: trade", f.key);
            }
        }
    }
    println!("node supply equals the domestic routes for {equal} of {checked} factions");
    assert!(checked > 3);
    assert_eq!(equal, checked);
}

/// Region commodity demand (0x00AB49F0) equals the stored `REGION` #32 for every region, and the
/// price update (0x00BCB020) gives back the stored prices of the start position.
#[test]
fn commodity_demand_and_prices_match_the_startpos() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).expect("vfs");
    let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs };
    let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let mut loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let map = ntw_formats::campaign_map::CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    ntw_campaign::trade::attach_map(&mut loaded.model, &map.regions);
    let m = &mut loaded.model;
    let regions = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER/REGIONS_ARRAY").expect("regions");
    let mut checked = 0;
    for it in &regions.items {
        let r = it.first().and_then(EsfNode::as_record).expect("region");
        let key = r.get_str(0).expect("key");
        let stored = r.get(32).and_then(EsfNode::as_u32_array).expect("#32").to_vec();
        let region = m.world.regions.values().find(|x| x.key == key).expect("model region");
        assert_eq!(m.commodity_demand(region), stored, "{key}: demand");
        checked += 1;
    }
    assert_eq!(checked, 72);
    let stored = m.world.commodity_prices.clone();
    m.update_commodity_prices();
    assert_eq!(m.world.commodity_prices, stored, "prices");
}

#[test]
fn region_neighbours_come_from_the_map_outlines() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).expect("vfs");
    let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs };
    let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let mut loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let map = ntw_formats::campaign_map::CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    ntw_campaign::trade::attach_map(&mut loaded.model, &map.regions);
    let w = &loaded.model.world;
    let id = |k: &str| w.regions.values().find(|r| r.key == k).expect(k).id;
    let keys = |k: &str| -> Vec<String> {
        let mut v: Vec<String> = w.region_neighbours[&id(k)].iter().map(|r| w.regions[r].key.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(w.region_neighbours.len(), 72);
    assert_eq!(
        keys("eur_france"),
        ["eur_alsace_lorraine", "eur_aquitaine", "eur_bretagne", "eur_normandie", "eur_pays_d_oc", "eur_picardie_champagne", "eur_piemont_liguria", "eur_provence", "eur_switzerland"]
    );
    // Islands touch only seas; neighbours are symmetric.
    assert!(keys("eur_sicily").is_empty() && keys("eur_corsica").is_empty());
    for (r, list) in &w.region_neighbours {
        for n in list {
            assert!(w.region_neighbours[n].contains(r), "{} - {}", w.regions[r].key, w.regions[n].key);
        }
    }
}

#[test]
fn tech_availability_rule_keeps_every_start_position_state() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).expect("vfs");
    let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs };
    let mut checked = 0;
    for campaign in ["eur_napoleon", "egy_napoleon", "ita_napoleon", "spa_napoleon"] {
        let Ok(bytes) = files.read(&format!("campaigns/{campaign}/startpos.esf")) else { continue };
        let esf = EsfFile::from_bytes(&bytes).expect("esf");
        let mut m = ntw_campaign::read_esf(&esf, &db).expect("load").model;
        let factions: Vec<_> = m.world.faction_details.keys().copied().collect();
        for f in factions {
            let before = m.world.faction_details[&f].technologies.clone();
            m.update_tech_availability(f);
            assert_eq!(m.world.faction_details[&f].technologies, before, "{campaign} faction {f:?}");
            checked += before.len();
        }
    }
    assert!(checked > 1000, "{checked}");
}

/// The population factors (`population::growth_factors`, `0x00AA9C10`) against those the original
/// stored in the vanilla saves (`REGION_FACTORS` #0, #3, #7; refreshed at the round start of the saved
/// turn), with the campaign map loaded for the military factor's area test. The saves whose armies
/// moved after the round start (`orig_fr_t1_b`: rebel bands spawned in eight regions, `orig_fr_may1811`)
/// are left out; every region of the others must match. Then one growth step (`population::grow`,
/// `0x00AB4070`) of `auto_nr4_t4` against `orig_over_nr4_0252`, one round later: every region whose
/// factors the next save also stores unchanged (no army moved, no capture) must reach its population.
#[test]
fn population_factors_and_growth_match_the_vanilla_saves() {
    use ntw_sim::campaign::population;
    let dir = data_dir();
    let ev = std::env::var_os("NTW_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(r"Documents\ntw-evidence\saves"));
    if !dir.is_dir() || !ev.is_dir() {
        println!("SKIP: no install at {} or no evidence saves at {}", dir.display(), ev.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).expect("vfs");
    let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs };
    let mut maps = std::collections::BTreeMap::new();
    let load = |name: &str, maps: &mut std::collections::BTreeMap<String, ntw_sim::campaign::Terrain>| {
        let path = ev.join(format!("{name}.save"));
        if !path.is_file() {
            return None;
        }
        let mut l = ntw_campaign::read_file(&path, &db).expect("save loads");
        let terrain = maps.entry(l.info.map_key.clone()).or_insert_with(|| {
            let map = ntw_formats::campaign_map::CampaignMap::load(&files, &l.info.map_key).expect("map");
            ntw_sim::campaign::Terrain(std::sync::Arc::new(ntw_campaign::pathing::build_grid(&map)))
        });
        l.model.terrain = Some(terrain.clone());
        Some(l.model)
    };
    let mut checked = 0;
    let mut military = 0;
    for name in ["auto_nr4_t4", "auto_after_c8", "orig_over_nr4_0252", "auto_b2b3_0211", "auto_nr1", "orig_fr_t1", "auto_orig_spa_0245"] {
        let Some(m) = load(name, &mut maps) else { continue };
        for r in m.world.regions.values() {
            let set = economy::region_effect_set(&m, r);
            let input = population::FactorInputs { set: &set, hostile_units: population::hostile_units(&m, r) };
            let s = population::growth_factors(&m, r, r.population, &r.population_state, &input);
            let p = &r.population_state;
            assert_eq!((s.factors, s.capacity, s.overcrowded), (p.factors, p.capacity, p.overcrowded), "{name} {}", r.key);
            assert_eq!(s.growth.to_bits(), p.growth.to_bits(), "{name} {}: growth", r.key);
            military += usize::from(p.factors[3] != 0.0);
            checked += 1;
        }
    }
    println!("population factors: {checked} regions, {military} with a military factor");
    assert!(checked > 400 && military >= 4, "{checked} {military}");
    let (Some(a), Some(b)) = (load("auto_nr4_t4", &mut maps), load("orig_over_nr4_0252", &mut maps)) else { return };
    let mut grown = 0;
    for r in a.world.regions.values() {
        let Some(r2) = b.world.regions.values().find(|x| x.key == r.key) else { continue };
        let owner = |m: &ntw_sim::campaign::CampaignModel, f| m.world.factions.get(&f).map(|x| x.key.clone());
        if r2.population_state.factors != r.population_state.factors || owner(&b, r2.owner) != owner(&a, r.owner) {
            continue;
        }
        let (pop, state) = population::grow(&a, r, r.population, &r.population_state);
        assert_eq!((pop, state.trend), (r2.population, r2.population_state.trend), "{}", r.key);
        grown += 1;
    }
    assert!(grown >= 68, "{grown}");
}
