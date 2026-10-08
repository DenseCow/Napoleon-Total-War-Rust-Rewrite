//! Research helper for the effects system (slot 0-F, `analysis/fidelity/EFFECTS_FIDELITY.md`): for
//! each faction of a startpos or save, the effect sums the model computes, and the unit upkeep with
//! and without the upkeep effects next to the upkeep the original stored (`ECONOMICS_DATA` #5[1] land,
//! #5[2] naval of the newest history record). Read-only.
//!
//! ```text
//! cargo run -p ntw_campaign --release --example effects_check -- <data dir> <file>... [--human key] [--bonus a,b,...]
//! ```

use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode};
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
            Some(p) => ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(data)) }.read(p).expect("vfs file"),
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
        let file_units = units_in_file(&esf);
        let (mut exact_with, mut exact_without, mut n) = (0, 0, 0);
        for f in m.world.factions.values() {
            // Every unit the file holds for the faction (armies, navies and garrisons alike).
            let units: Vec<&str> = file_units.get(&f.key).map(|v| v.iter().map(String::as_str).collect()).unwrap_or_default();
            if units.is_empty() {
                continue;
            }
            if std::env::var("FX_UNITS").is_ok_and(|k| k == f.key) {
                println!("{} units: {:?}", f.key, units);
            }
            let mut with = (0i64, 0i64);
            let mut without = (0i64, 0i64);
            for k in &units {
                let Some(rec) = db.unit(k) else { continue };
                let naval = rec.category.starts_with("naval");
                let fmod = fx.faction(f.id, if naval { "upkeep_cost_mod_naval_all" } else { "upkeep_cost_mod_land_all" });
                let cat = fx.faction_qualified(f.id, BonusKind::UnitCategory, "upkeep_mod", &rec.category);
                let class = fx.faction_qualified(f.id, BonusKind::UnitClass, "upkeep_mod", &rec.unit_class);
                let mult = (100.0 + fmod + cat + class).max(0.0);
                let cost = (f64::from(rec.upkeep) * f64::from(mult) * 0.01).round_ties_even() as i64;
                let slot = if naval { &mut with.1 } else { &mut with.0 };
                *slot += cost;
                let slot = if naval { &mut without.1 } else { &mut without.0 };
                *slot += i64::from(rec.upkeep);
            }
            let s = stored.get(&f.key).copied();
            n += 1;
            if s == Some(with) {
                exact_with += 1;
            }
            if s == Some(without) {
                exact_without += 1;
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
        println!("upkeep exact: with effects {exact_with}/{n}, without {exact_without}/{n}");
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

/// The newest `ECONOMICS_DATA` #5[1] (land upkeep) and #5[2] (naval upkeep) per faction key.
fn stored_upkeep(esf: &EsfFile) -> std::collections::BTreeMap<String, (i64, i64)> {
    let mut out = std::collections::BTreeMap::new();
    let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") else { return out };
    for item in &arr.items {
        let Some(f) = item.first().and_then(EsfNode::as_record) else { continue };
        let Some(key) = f.values().find_map(EsfNode::as_str) else { continue };
        let Some(e) = f.child("FACTION_ECONOMICS") else { continue };
        let Some(h) = e.get(0).and_then(EsfNode::as_record_array) else { continue };
        let Some(d) = h.items.last().and_then(|it| it.first()).and_then(EsfNode::as_record) else { continue };
        let Some(a) = d.get(5).and_then(EsfNode::as_i32_array) else { continue };
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

/// Every `UNIT_RECORD_KEY` under each faction's `ARMY_ARRAY`, by faction key.
fn units_in_file(esf: &EsfFile) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut out: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") else { return out };
    for item in &arr.items {
        let Some(f) = item.first().and_then(EsfNode::as_record) else { continue };
        let Some(key) = f.values().find_map(EsfNode::as_str) else { continue };
        let Some(armies) = f.record_array("ARMY_ARRAY") else { continue };
        let list = out.entry(key.to_string()).or_default();
        for a in armies.records() {
            a.walk(&mut |r: &ntw_formats::esf::EsfRecord| {
                if r.name == "UNIT_RECORD_KEY"
                    && let Some(k) = r.get_str(0)
                {
                    list.push(k.to_string());
                }
            });
        }
    }
    out
}

/// Faction taxes with the effect bonuses (tax efficiency with `admin_cost_mod`; per region: the
/// governor/minister bonus (here `tax_bonus_minister`: every region is taken as home theatre,
/// PROVISIONAL), `tax_bonus_building` of the region, `tax_bonus_technology` of the faction) next to the
/// stored taxes (`ECONOMICS_DATA` #1[0], newest record) and the model without effects.
fn taxes_check(m: &ntw_sim::campaign::CampaignModel, fx: &Effects, esf: &EsfFile) {
    use ntw_sim::campaign::economy;
    let mut stored = std::collections::BTreeMap::new();
    if let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") {
        for item in &arr.items {
            let Some(f) = item.first().and_then(EsfNode::as_record) else { continue };
            let Some(key) = f.values().find_map(EsfNode::as_str) else { continue };
            let v = f
                .child("FACTION_ECONOMICS")
                .and_then(|e| e.get(0))
                .and_then(EsfNode::as_record_array)
                .and_then(|h| h.items.last())
                .and_then(|it| it.first())
                .and_then(EsfNode::as_record)
                .and_then(|d| d.get(1))
                .and_then(EsfNode::as_i32_array)
                .and_then(|a| a.first().copied());
            stored.insert(key.to_string(), v);
        }
    }
    let (mut exact_fx, mut exact_plain, mut n) = (0, 0, 0);
    for f in m.world.factions.values() {
        let regions: Vec<&ntw_sim::campaign::Region> = m.world.regions.values().filter(|r| r.owner == f.id).collect();
        if regions.is_empty() {
            continue;
        }
        let admin = fx.faction.get(&f.id).map_or(0, |s| s.get_int("admin_cost_mod"));
        let eff = economy::tax_efficiency(&m.rules, regions.len() as i32, admin);
        let mut taxes = 0;
        for r in &regions {
            if r.tax_exempt {
                continue;
            }
            let bonuses = economy::TaxBonuses {
                character: fx.faction(f.id, "tax_bonus_minister"),
                building: fx.region_local(r.id, "tax_bonus_building"),
                technology: fx.faction(f.id, "tax_bonus_technology"),
            };
            taxes += [&f.tax_lower, &f.tax_upper]
                .into_iter()
                .map(|level| economy::class_taxes(economy::effective_tax_rate(m.rules.tax_rate(level), eff, bonuses), r.gdp, r.town_wealth))
                .sum::<i32>();
        }
        let plain: i32 = regions.iter().map(|r| economy::region_taxes(m, r)).sum();
        let s = stored.get(&f.key).copied().flatten();
        n += 1;
        exact_fx += usize::from(s == Some(taxes));
        exact_plain += usize::from(s == Some(plain));
        println!("  taxes {:<26} stored {:?} with effects {taxes} plain {plain} (admin {admin})", f.key, s);
    }
    println!("taxes exact: with effects {exact_fx}/{n}, plain {exact_plain}/{n}");
}

/// Prints every faction's saved `CAMPAIGN_BONUS_VALUES` blocks (`FACTION` #54 base, #55 base + difficulty
/// handicap) as (type, bonus id, value, qualifier).
fn bonus_blocks(esf: &EsfFile) {
    let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") else { return };
    for item in &arr.items {
        let Some(f) = item.first().and_then(EsfNode::as_record) else { continue };
        let Some(key) = f.values().find_map(EsfNode::as_str) else { continue };
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
/// (difficulty, human).
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
    let (mut ok, mut n) = (0, 0);
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
        n += 1;
        let same = model == saved;
        ok += usize::from(same);
        let show = |s: &ntw_sim::campaign::effects::EffectSet| {
            s.values.iter().map(|(k, v)| if k.kind == BonusKind::Basic { format!("{}={v}", k.bonus) } else { format!("{:?}:{}:{}={v}", k.kind, k.bonus, k.qualifier) }).collect::<Vec<_>>().join(" ")
        };
        if !same || std::env::var("FX_ALL").is_ok() {
            println!("  difficulty {:<24} diff {diff} human {human} {}\n    saved #55: {}\n    model    : {}", f.key, if same { "ok" } else { "DIFFERS" }, show(&saved), show(&model));
        }
    }
    println!("difficulty (#55 = #54 + handicap): {ok}/{n}");
}
