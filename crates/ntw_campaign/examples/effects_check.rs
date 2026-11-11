//! Research helper for the effects system (slot 0-F, `analysis/fidelity/EFFECTS_FIDELITY.md`): for
//! each faction of a startpos or save, the effect sums the model computes, and the unit upkeep with
//! and without the upkeep effects next to the upkeep the original stored (`ECONOMICS_DATA` #5[1] land,
//! #5[2] naval of the newest history record). Read-only.
//!
//! ```text
//! cargo run -p ntw_campaign --release --example effects_check -- <data dir> <file>... [--human key] [--bonus a,b,...]
//! ```

use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::effects::{BonusKind, Effects};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((data, rest)) = args.split_first() else {
        eprintln!("usage: effects_check <data dir> <file>... [--bonus a,b]");
        std::process::exit(2);
    };
    let mut files = Vec::new();
    let mut bonuses: Vec<String> = ["tax_bonus_technology", "tax_bonus_minister", "upkeep_cost_mod_land_all", "upkeep_cost_mod_naval_all", "admin_cost_mod", "gdp_mod_all"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut human_arg: Option<String> = None;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--human" {
            human_arg = it.next().cloned();
        } else if a == "--bonus" {
            bonuses = it.next().map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default();
        } else {
            files.push(a.clone());
        }
    }
    let db = GameDatabase::from_install(data).expect("game database");
    if std::env::var("FX_DIFF").is_ok() {
        for r in db.campaign.effects.difficulty.iter() {
            println!("  row {} {} {} {}", r.difficulty, r.human, r.effect, r.value);
        }
    }
    let vfs = ntw_formats::pack::Vfs::open_install(std::path::Path::new(data)).expect("vfs");
    for file in &files {
        // `vfs:campaigns/.../startpos.esf` reads from the packs.
        let bytes = match file.strip_prefix("vfs:") {
            Some(p) => ntw_formats::pack::GameFiles { vfs: &vfs }.read(p).expect("vfs file"),
            None => std::fs::read(file).expect("read"),
        };
        let esf = EsfFile::from_bytes(&bytes).expect("esf");
        let mut l = ntw_campaign::read_esf(&esf, &db).expect("load");
        // The human: the faction whose setup flag says so (keys reader), else none.
        if let Some(h) = human_arg.clone().or_else(|| human_of(&esf)) {
            l.set_human(&h);
        }
        // A start position: the campaign start (difficulty handicap) as the game would run it.
        if !l.model.turn.started {
            ntw_sim::campaign::effects::apply_start_handicaps(&mut l.model);
        }
        let m = &l.model;
        let fx = Effects::compute(m);
        println!("== {file}");
        let stored = stored_upkeep(&esf);
        let (mut exact_with, mut exact_without, mut n, mut no_figure) = (0, 0, 0, 0);
        for f in m.world.factions.values() {
            // The forces both totals sum: the model's costed forces (a force with no commander counts 0 in
            // both), and `FX_UNITS` prints the same units. A faction is skipped only when it has no unit
            // in them and the file stores no upkeep for it either (none, or 0 / 0): a stored figure with
            // no costed unit is a row to see.
            let forces: Vec<_> = m.world.forces.values().filter(|x| x.faction == f.id && ntw_sim::campaign::economy::costed_force(x).is_some()).collect();
            let s = stored.get(&f.key).copied();
            if forces.iter().all(|x| x.units.is_empty()) && matches!(s, None | Some((0, 0))) {
                continue;
            }
            if std::env::var("FX_UNITS").is_ok_and(|k| k == f.key) {
                let units: Vec<&str> = forces.iter().flat_map(|x| x.units.iter().map(|u| u.unit_key.as_str())).collect();
                println!("{} units: {:?}", f.key, units);
            }
            // The model's per-force rule (each force costed with its commander's set).
            let (land, naval) = ntw_sim::campaign::economy::faction_upkeep_split_with(m, &fx, f.id);
            let with = (i64::from(land), i64::from(naval));
            // The same units without the effects, from the unit table the rule reads (a key it lacks
            // counts 0 in both totals).
            let mut without = (0i64, 0i64);
            for force in &forces {
                for u in &force.units {
                    let Some(rec) = m.rules.units.get(&u.unit_key) else { continue };
                    let slot = if force.is_navy { &mut without.1 } else { &mut without.0 };
                    *slot += i64::from(rec.upkeep);
                }
            }
            // Only a faction with a stored figure can be exact or not.
            if s.is_some() {
                n += 1;
                exact_with += usize::from(s == Some(with));
                exact_without += usize::from(s == Some(without));
            } else {
                no_figure += 1;
            }
            let sums: Vec<String> = bonuses.iter().map(|b| format!("{b}={}", fx.faction(f.id, b))).filter(|x| !x.ends_with("=0")).collect();
            println!(
                "{:<28} human {} diff {} upkeep stored {:?} model+effects {:?} plain {:?} | {}",
                f.key,
                m.turn.humans.contains(&f.id),
                m.world.faction_details.get(&f.id).map_or(0, |d| d.difficulty),
                s,
                with,
                without,
                sums.join(" ")
            );
        }
        println!("upkeep exact: with effects {exact_with}/{n}, without {exact_without}/{n} ({no_figure} more with units and no stored figure)");
        taxes_check(m, &fx, &esf);
        difficulty_check(m);
        if let Ok(k) = std::env::var("FX_TECH")
            && let Some(d) = m.faction_by_key(&k).and_then(|f| m.world.faction_details.get(&f.id))
        {
            let mut by_state = std::collections::BTreeMap::new();
            for (_, s) in &d.technologies {
                *by_state.entry(*s).or_insert(0) += 1;
            }
            println!("techs {k} by state: {by_state:?}");
        }
        if std::env::var("FX_BLOCKS").is_ok() {
            bonus_blocks(&esf);
        }
    }
}

/// Every faction record of the file with its key (its first string, as the loader reads it): the
/// `FACTION_ARRAY` items, then `WORLD/REBEL_FACTION/FACTION`, which the model lists too.
fn faction_records(esf: &EsfFile) -> Vec<(&str, &EsfRecord)> {
    let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return Vec::new() };
    let listed = world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.items.iter()).filter_map(|item| item.first().and_then(EsfNode::as_record));
    let rebels = world.child("REBEL_FACTION").and_then(|r| r.child("FACTION"));
    listed.chain(rebels).filter_map(|f| Some((f.values().find_map(EsfNode::as_str)?, f))).collect()
}

/// The newest `ECONOMICS_DATA` record of a faction (the last of its `FACTION_ECONOMICS` history).
fn newest_economics(faction: &EsfRecord) -> Option<&EsfRecord> {
    let history = faction.child("FACTION_ECONOMICS")?.get(0)?.as_record_array()?;
    history.items.last()?.first()?.as_record()
}

/// The newest `ECONOMICS_DATA` #5[1] (land upkeep) and #5[2] (naval upkeep) per faction key. A
/// faction with no history record has no entry.
fn stored_upkeep(esf: &EsfFile) -> std::collections::BTreeMap<String, (i64, i64)> {
    let mut out = std::collections::BTreeMap::new();
    for (key, f) in faction_records(esf) {
        let Some(a) = newest_economics(f).and_then(|d| d.get(5)).and_then(EsfNode::as_i32_array) else { continue };
        out.insert(key.to_string(), (i64::from(a.get(1).copied().unwrap_or(0)), i64::from(a.get(2).copied().unwrap_or(0))));
    }
    out
}

/// The faction whose `CAMPAIGN_PLAYER_SETUP` first flag is set (INFERRED is human, see
/// `ntw_ai::campaign::keys::FactionDifficulty`).
fn human_of(esf: &EsfFile) -> Option<String> {
    let arr = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY")?;
    for rec in arr.records().filter(|r| r.name == "FACTION") {
        let setup = rec.child("CAMPAIGN_PLAYER_SETUP")?;
        let vals: Vec<&EsfNode> = setup.values().collect();
        let key = vals.iter().find_map(|v| v.as_str())?;
        if vals.iter().filter_map(|v| v.as_bool()).next() == Some(true) {
            return Some(key.to_string());
        }
    }
    None
}

/// Faction taxes by the model's rule (`economy::region_taxes_with`: the tax efficiency with
/// `admin_cost_mod` and the region's `economy::tax_bonuses`) next to the stored taxes (`ECONOMICS_DATA`
/// #1[0], newest record) and to the same rule with no effect at all (an empty [`Effects`]: no bonus,
/// `admin_cost_mod` 0). Both columns read the same regions' wealth, so they differ only by the effects.
fn taxes_check(m: &ntw_sim::campaign::CampaignModel, fx: &Effects, esf: &EsfFile) {
    use ntw_sim::campaign::economy;
    let stored: std::collections::BTreeMap<&str, i32> = faction_records(esf)
        .into_iter()
        .filter_map(|(key, f)| Some((key, *newest_economics(f)?.get(1)?.as_i32_array()?.first()?)))
        .collect();
    let none = Effects::default();
    let (mut exact_fx, mut exact_plain, mut n) = (0, 0, 0);
    for f in m.world.factions.values() {
        let regions: Vec<&ntw_sim::campaign::Region> = m.world.regions.values().filter(|r| r.owner == f.id).collect();
        if regions.is_empty() {
            continue;
        }
        let admin = fx.faction.get(&f.id).map_or(0, |s| s.get_int("admin_cost_mod"));
        let taxes: i32 = regions.iter().map(|r| economy::region_taxes_with(m, fx, r)).sum();
        let plain: i32 = regions.iter().map(|r| economy::region_taxes_with(m, &none, r)).sum();
        let s = stored.get(f.key.as_str()).copied();
        // Only a faction with a stored figure can be exact or not.
        if s.is_some() {
            n += 1;
            exact_fx += usize::from(s == Some(taxes));
            exact_plain += usize::from(s == Some(plain));
        }
        println!("  taxes {:<26} stored {:?} with effects {taxes} plain {plain} (admin {admin})", f.key, s);
    }
    println!("taxes exact: with effects {exact_fx}/{n}, plain {exact_plain}/{n}");
}

/// Prints every faction's saved `CAMPAIGN_BONUS_VALUES` blocks (`FACTION` #54 base, #55 base + difficulty
/// handicap) as (type, bonus id, value, qualifier).
fn bonus_blocks(esf: &EsfFile) {
    for (key, f) in faction_records(esf) {
        for idx in [54, 55] {
            let Some(rec) = f.get(idx).and_then(EsfNode::as_record) else { continue };
            let mut blocks = Vec::new();
            for a in rec.children.iter().filter_map(EsfNode::as_record_array) {
                for item in &a.items {
                    let v: Vec<String> = item.iter().map(|n| match n {
                        EsfNode::Record(r) => format!("{}[{}]", r.name, r.values().map(|x| format!("{x:?}")).collect::<Vec<_>>().join(",")),
                        other => format!("{other:?}"),
                    }).collect();
                    blocks.push(format!("{}({})", a.name, v.join(",")));
                }
            }
            println!("  blocks {key} #{idx} {}: {}", rec.name, blocks.join(" "));
        }
    }
}

/// The saved `FACTION` #55 (base + difficulty handicap) against #54 + the model's difficulty set for
/// (difficulty, human). A faction that differs because its stored #55 is just its #54 (the file holds
/// no handicap entry at all: a campaign whose start did not build #55) is counted on its own.
fn difficulty_check(m: &ntw_sim::campaign::CampaignModel) {
    use ntw_sim::campaign::effects::{apply_start_handicaps, saved_set};
    let mut rebuilt = m.clone();
    apply_start_handicaps(&mut rebuilt);
    if std::env::var("FX_DIFF").is_ok() {
        for ((d, h), s) in m.rules.effects.difficulty() {
            let v: Vec<String> = s.values.iter().map(|(k, v)| format!("{}={v}", k.bonus)).collect();
            println!("  table ({d},{h}): {}", v.join(" "));
        }
    }
    let (mut ok, mut n, mut bare) = (0, 0, 0);
    for f in m.world.factions.values() {
        let Some(d) = m.world.faction_details.get(&f.id) else { continue };
        if d.bonus_with_difficulty.is_empty() && d.bonus_base.is_empty() {
            continue;
        }
        let human = m.turn.humans.contains(&f.id);
        let r = &rebuilt.world.faction_details[&f.id];
        let diff = r.difficulty;
        let mut model = saved_set(&r.bonus_with_difficulty);
        model.values.retain(|_, v| *v != 0.0);
        let mut saved = saved_set(&d.bonus_with_difficulty);
        saved.values.retain(|_, v| *v != 0.0);
        let mut base = saved_set(&d.bonus_base);
        base.values.retain(|_, v| *v != 0.0);
        n += 1;
        let same = model == saved;
        ok += usize::from(same);
        bare += usize::from(!same && saved == base);
        let show = |s: &ntw_sim::campaign::effects::EffectSet| {
            s.values.iter().map(|(k, v)| if k.kind == BonusKind::Basic { format!("{}={v}", k.bonus) } else { format!("{:?}:{}:{}={v}", k.kind, k.bonus, k.qualifier) }).collect::<Vec<_>>().join(" ")
        };
        if !same || std::env::var("FX_ALL").is_ok() {
            println!("  difficulty {:<24} diff {diff} human {human} {}\n    saved #55: {}\n    model    : {}", f.key, if same { "ok" } else { "DIFFERS" }, show(&saved), show(&model));
        }
    }
    println!("difficulty (#55 = #54 + handicap): {ok}/{n}; of the rest, {bare} store #55 = #54 (no handicap entry in the file)");
}
