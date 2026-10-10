//! Writes the save-compatibility test saves for the original game (analysis/campaign/SAVE_COMPAT.md
//! §8): a new eur_napoleon campaign as france, played the way the game plays it (the original
//! campaign scripts through `ntw_script`, the campaign AI in the turn loop, the save written with
//! the scripts' `save_value` slots, exactly as `F5` in the game does), then checked with every
//! invariant we know (`save_check`, reload, model rules) before anything is written.
//!
//! ```text
//! cargo run --release -p ntw_ai --example nr_test_saves -- <out dir> [round2 | turns [n]]
//! ```
//! `round2` (default; the second user test, SAVE_COMPAT.md §9):
//! * `NR-4 France turn 1.save`: the new campaign, turn 1, nothing done (human fixes).
//! * `NR-B1 France turn 1 AI rebuilt.save`: NR-4 with only the AI block's version set to 12 (the
//!   original then skips it and rebuilds its AI): tests that rebuild path alone.
//! * `NR-B2 France turn 1 new units.save`: NR-4 plus raised units (a new colonel-led army, a new
//!   captain-led navy, a unit joining a garrison) and the AI block kept as loaded (version 13):
//!   tests our new character / army / navy / unit records and objects the AI block does not know.
//! * `NR-B3 France turn 1 garrison unit.save`: NR-4 plus one unit joining a french garrison (no new
//!   character or force, no pathfinder change), AI block kept: the smallest addition.
//!
//! `turns [n]`: `NR-5` after n AI End Turns and `NR-6` after recruiting / merging / building and 3
//! End Turns (the first round's NR-2 / NR-3, round 3 of the user tests): the writer keeps their AI
//! block at version 13 and in step with the world (`cai_world::sync`, SAVE_COMPAT.md §10).
//!
//! The tool only writes the `NR-` files named above (replacing older copies of them, never any
//! other file). It exits with an error (and writes nothing) if a save breaks a rule.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ntw_ai::campaign::CampaignAiData;
use ntw_ai::campaign::driver;
use ntw_campaign::script_values::ScriptSaveValue;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_script::{ScriptContext, ScriptHost, ScriptSource, ScriptValue};
use ntw_sim::campaign::{CampaignCommand, CampaignModel, ForceId, RegionId, Terrain};

const CAMPAIGN: &str = "eur_napoleon";
const HUMAN: &str = "france";

struct Install {
    dir: PathBuf,
    db: GameDatabase,
    data: Arc<CampaignAiData>,
    startpos: Vec<u8>,
    terrain: Terrain,
    names: Option<ntw_campaign::names::NameData>,
    pictures: Vec<ntw_campaign::header_map::TheatrePictures>,
}

impl Install {
    fn open() -> Self {
        let dir = std::env::var_os("NTW_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
        let db = GameDatabase::from_install(&dir).expect("db");
        for w in &db.load_warnings {
            eprintln!("WARN game data: {w}");
        }
        let vfs = Vfs::open_install(&dir).expect("packs");
        let data = Arc::new(CampaignAiData::load(&vfs, &db).expect("AI tables"));
        let startpos = GameFiles { vfs: &vfs }.read(&format!("campaigns/{CAMPAIGN}/startpos.esf")).expect("startpos");
        let loaded = ntw_campaign::read(&startpos, &db).expect("startpos");
        let map = CampaignMap::load(&GameFiles { vfs: &vfs }, &loaded.info.map_key).expect("map");
        let terrain = Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map)));
        let names = ntw_campaign::names::NameData::load(&vfs, &db);
        let files = GameFiles { vfs: &vfs };
        let pictures = loaded.info.header.maps.iter().filter_map(|m| ntw_campaign::header_map::TheatrePictures::load(&files, &db, &loaded.info.map_key, &m.theatre)).collect();
        Install { dir, db, data, startpos, terrain, names, pictures }
    }

    /// A new campaign as the game starts one (`napoleon::campaign::scene`): model with the human
    /// set and the movement grid, script host with the campaign's scripts, the AI hook, then
    /// `NewSession`, `NewCampaignStarted` and turn 1.
    fn new_campaign(&self) -> ScriptHost {
        let mut loaded = ntw_campaign::read(&self.startpos, &self.db).expect("startpos");
        assert!(loaded.set_human(HUMAN));
        loaded.model.terrain = Some(self.terrain.clone());
        if let Some(nd) = &self.names {
            ntw_campaign::names::attach_data(&mut loaded.model, nd);
        }
        let source = ScriptSource::from_install(&self.dir).expect("scripts");
        let mut host = ScriptHost::new(loaded.model, HUMAN, source).expect("script host");
        if let Err(e) = host.load_campaign(CAMPAIGN) {
            eprintln!("warning: campaign scripts: {e}");
        }
        driver::install(&mut host, self.data.clone());
        for name in ["NewSession", "NewCampaignStarted"] {
            let r = host.fire(name, ScriptContext::for_faction(HUMAN));
            for e in r.errors {
                eprintln!("warning: script {name}: {e}");
            }
        }
        host.start_campaign();
        host
    }
}

/// A command as the game applies it (`CampaignSim::command`): a battle it starts is autoresolved.
fn command(host: &mut ScriptHost, cmd: CampaignCommand) -> bool {
    let label = format!("{cmd:?}");
    match host.apply(cmd) {
        Ok(_) => {
            if host.model().pending_battle.is_some() {
                let _ = host.apply(CampaignCommand::Autoresolve);
            }
            println!("  ok: {}", short(&label));
            true
        }
        Err(e) => {
            println!("  rejected: {} ({e:?})", short(&label));
            false
        }
    }
}

fn short(s: &str) -> String {
    s.chars().take(110).collect()
}

fn end_turn(host: &mut ScriptHost) {
    if host.model().pending_battle.is_some() {
        let _ = host.apply(CampaignCommand::Autoresolve);
    }
    host.apply(CampaignCommand::EndTurn).expect("end turn");
    let m = host.model();
    println!("  end turn -> turn {}, {} forces, {} characters", m.calendar.turn_number(), m.world.forces.len(), m.world.characters.len());
}

/// The scripts' saved values as the game stores them (`napoleon::campaign::play::script_values_out`).
fn script_values_out(values: &[ScriptValue]) -> Vec<ScriptSaveValue> {
    values
        .iter()
        .map(|v| match v {
            ScriptValue::Bool(b) => ScriptSaveValue::Bool(*b),
            ScriptValue::Number(n) => ScriptSaveValue::Int(*n as i32),
            ScriptValue::String(s) => ScriptSaveValue::Int(s.trim().parse::<f64>().map_or(0, |n| n as i32)),
            ScriptValue::Nil => ScriptSaveValue::Int(0),
        })
        .collect()
}

/// Model rules the original keeps (SAVE_COMPAT.md §4).
fn model_problems(m: &CampaignModel) -> Vec<String> {
    let mut out = Vec::new();
    for f in m.world.forces.values() {
        match f.commander.and_then(|c| m.world.characters.get(&c)) {
            None => out.push(format!("force {} has no commander", f.id.raw())),
            Some(c) if c.faction != f.faction => out.push(format!("force {}: commander of another faction", f.id.raw())),
            Some(_) => {}
        }
        if f.units.is_empty() {
            out.push(format!("force {} has no units", f.id.raw()));
        }
    }
    for r in m.world.regions.values() {
        if let Some(g) = r.garrison {
            let ok = m.world.forces.get(&g).and_then(|f| f.commander).and_then(|c| m.world.characters.get(&c)).is_some_and(|c| c.garrisoned_in == Some(r.id));
            if !ok {
                out.push(format!("garrison of {} is not commanded from inside", r.key));
            }
        }
    }
    out
}

/// What to do with the `CAI_INTERFACE` block after the writer (bisect saves only).
#[derive(Clone, Copy, PartialEq, Eq)]
enum AiBlock {
    /// As the writer leaves it.
    AsWritten,
    /// Force this record version (12: the original skips the block and rebuilds its AI; 13: the
    /// original loads the stored block).
    Version(u8),
}

/// Writes the save as the game does, checks it, and returns its bytes.
fn save(inst: &Install, host: &mut ScriptHost, name: &str) -> Vec<u8> {
    save_with(inst, host, name, AiBlock::AsWritten)
}

/// [`save`] with an AI block override (bisect saves).
fn save_with(inst: &Install, host: &mut ScriptHost, name: &str, ai: AiBlock) -> Vec<u8> {
    save_values_from(inst, host, name, ai, None)
}

/// [`save_with`], with the scripts' saved values taken from `values_host` when given (bisect).
fn save_values_from(inst: &Install, host: &mut ScriptHost, name: &str, ai: AiBlock, values_host: Option<&mut ScriptHost>) -> Vec<u8> {
    save_full(inst, host, name, ai, values_host, false)
}

/// [`save_values_from`]; `old_slot_owners` writes captured regions as the writer did before
/// round 5 (slot residences left with the old owner; NR-C8b, a deliberate rule break).
fn save_full(inst: &Install, host: &mut ScriptHost, name: &str, ai: AiBlock, values_host: Option<&mut ScriptHost>, old_slot_owners: bool) -> Vec<u8> {
    let (values, report) = match values_host {
        Some(v) => v.save_values(),
        None => host.save_values(),
    };
    for e in report.errors {
        eprintln!("warning: SavingGame: {e}");
    }
    let values = script_values_out(&values);
    let source = EsfFile::from_bytes(&inst.startpos).expect("esf");
    let m = host.model();
    let mut tree = ntw_campaign::save::write_save_with(&source, &m, HUMAN, now(), Some(&values)).expect("write");
    ntw_campaign::header_map::update_maps(&mut tree, &m, HUMAN, &inst.pictures);
    if let AiBlock::Version(v) = ai {
        let cai = tree.root.children.iter_mut().find_map(|c| match c {
            ntw_formats::esf::EsfNode::Record(r) if r.name == "CAMPAIGN_ENV" => Some(r),
            _ => None,
        });
        let cai = cai
            .and_then(|e| e.children.iter_mut().find_map(|c| match c {
                ntw_formats::esf::EsfNode::Record(r) if r.name == "CAMPAIGN_MODEL" => Some(r),
                _ => None,
            }))
            .and_then(|m| m.children.iter_mut().find_map(|c| match c {
                ntw_formats::esf::EsfNode::Record(r) if r.name == "CAI_INTERFACE" => Some(r),
                _ => None,
            }))
            .expect("CAI_INTERFACE");
        cai.version = v;
    }
    if old_slot_owners {
        undo_handover(&source, &mut tree);
    }
    let bytes = tree.to_bytes().expect("bytes");
    let mut problems = model_problems(&m);
    // Every rule of the checker, on the tree as written and as read back from the bytes.
    let back = EsfFile::from_bytes(&bytes).expect("written bytes parse");
    let r = ntw_campaign::save_check::check(&back);
    // NR-B1 sets another AI block version on purpose (the crash test of the rebuild path).
    let deliberate = |v: &String| matches!(ai, AiBlock::Version(x) if x != ntw_campaign::save_check::LOADABLE_CAI_VERSION) && v.starts_with("AI block version");
    let deliberate_regions = |v: &String| old_slot_owners && v.starts_with("region ");
    problems.extend(r.violations.iter().filter(|v| !deliberate(v) && !deliberate_regions(v)).cloned());
    if back.to_bytes().expect("bytes") != bytes {
        problems.push("the written bytes do not re-serialise identically".into());
    }
    match ntw_campaign::read(&bytes, &inst.db) {
        Ok(l) => {
            if l.model.world.forces.len() != m.world.forces.len() || l.model.world.characters.len() != m.world.characters.len() {
                problems.push("reloading gives a different world".into());
            }
            problems.extend(model_problems(&l.model).into_iter().map(|p| format!("reloaded: {p}")));
        }
        Err(e) => problems.push(format!("does not reload: {e}")),
    }
    println!(
        "{name}: turn {}, {} forces, {} characters, {} bytes; {} problems; AI block rebuilt by the original: {}; {} commanders without a pathfinder obstacle",
        m.calendar.turn_number(),
        m.world.forces.len(),
        m.world.characters.len(),
        bytes.len(),
        problems.len(),
        r.ai_block_rebuilt,
        r.commanders_without_obstacle.len()
    );
    if !problems.is_empty() {
        for p in problems.iter().take(30) {
            eprintln!("  {p}");
        }
        eprintln!("{name}: NOT written (rule violations)");
        std::process::exit(1);
    }
    bytes
}

fn write(dir: &Path, name: &str, bytes: &[u8]) {
    assert!(name.starts_with("NR-"), "only NR- files are written");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write file");
    println!("  written {}", path.display());
}

/// The player's turn for NR-3: recruit, build, merge, move.
fn player_actions(host: &mut ScriptHost) {
    let human = host.model().faction_by_key(HUMAN).expect("france").id;
    // Recruit: one land unit in each of up to three regions (garrisoned ones join the garrison,
    // others raise a colonel-led army).
    let regions: Vec<(RegionId, String, bool)> = {
        let m = host.model();
        let mut v: Vec<_> = m.world.regions.values().filter(|r| r.owner == human).map(|r| (r.id, r.key.clone(), r.garrison.is_some())).collect();
        v.sort_by_key(|r| (!r.2, r.1.clone()));
        v
    };
    let mut recruited = 0;
    let mut with_garrison = false;
    let mut without = false;
    for (id, key, garrisoned) in &regions {
        if recruited >= 4 {
            break;
        }
        if (*garrisoned && with_garrison) || (!*garrisoned && without) {
            continue;
        }
        let units = host.model().recruitable_units(*id);
        let land: Vec<String> = {
            let m = host.model();
            units.into_iter().filter(|e| e.flags == 0).map(|e| e.unit_key).filter(|u| m.rules.units.get(u).is_some_and(|x| !x.is_naval)).collect()
        };
        for u in land.iter().take(2) {
            println!(" recruit {u} in {key} (garrison: {garrisoned})");
            if command(host, CampaignCommand::Recruit { region: *id, unit_key: u.clone() }) {
                recruited += 1;
                if *garrisoned {
                    with_garrison = true;
                } else {
                    without = true;
                }
            }
        }
    }
    // A ship, if a port can build one.
    'ships: for (id, key, _) in &regions {
        let units = host.model().recruitable_units(*id);
        let naval: Vec<String> = {
            let m = host.model();
            units.into_iter().filter(|e| e.flags == 0).map(|e| e.unit_key).filter(|u| m.rules.units.get(u).is_some_and(|x| x.is_naval)).collect()
        };
        for u in naval.iter().take(1) {
            println!(" recruit ship {u} in {key}");
            if command(host, CampaignCommand::Recruit { region: *id, unit_key: u.clone() }) {
                break 'ships;
            }
        }
    }
    // Build: the first upgrade a region allows, in two regions.
    let mut built = 0;
    for (id, key, _) in &regions {
        if built >= 2 {
            break;
        }
        let options: Vec<(ntw_sim::campaign::SlotRef, String)> = {
            let m = host.model();
            let r = &m.world.regions[id];
            let mut o = Vec::new();
            for (i, s) in r.slots.iter().enumerate() {
                if let Some(b) = &s.building {
                    for u in m.rules.buildings.get(&b.level_key).map(|b| b.upgrades_to.clone()).unwrap_or_default() {
                        if m.can_build(*id, ntw_sim::campaign::SlotRef::Slot(i), &u).is_ok() {
                            o.push((ntw_sim::campaign::SlotRef::Slot(i), u));
                        }
                    }
                }
            }
            o
        };
        if let Some((slot, level)) = options.into_iter().next() {
            println!(" build {level} in {key} slot {slot:?}");
            if command(host, CampaignCommand::ConstructBuilding { region: *id, slot, level_key: level }) {
                built += 1;
            }
        }
    }
    // Merge: the two closest french land armies in the field.
    let pair: Option<(ForceId, ForceId)> = {
        let m = host.model();
        let armies: Vec<(ForceId, f64, f64)> = m
            .world
            .forces
            .values()
            .filter(|f| f.faction == human && !f.is_navy)
            .filter_map(|f| {
                let c = m.world.characters.get(&f.commander?)?;
                (c.garrisoned_in.is_none()).then(|| (f.id, c.position.0.to_f32() as f64, c.position.1.to_f32() as f64))
            })
            .collect();
        let mut best: Option<(f64, ForceId, ForceId)> = None;
        for (i, a) in armies.iter().enumerate() {
            for b in &armies[i + 1..] {
                let d = (a.1 - b.1).hypot(a.2 - b.2);
                if best.is_none_or(|x| d < x.0) {
                    best = Some((d, a.0, b.0));
                }
            }
        }
        best.map(|(_, a, b)| (a, b))
    };
    if let Some((a, b)) = pair {
        println!(" merge army {} into {}", a.raw(), b.raw());
        command(host, CampaignCommand::MergeForces { force: a, into: b });
    }
    // Move: another field army a short way (towards its nearest own settlement).
    let mv = {
        let m = host.model();
        m.world.forces.values().filter(|f| f.faction == human && !f.is_navy && Some(f.id) != pair.map(|p| p.1)).find_map(|f| {
            let c = m.world.characters.get(&f.commander?)?;
            if c.garrisoned_in.is_some() {
                return None;
            }
            let (x, z) = (c.position.0.to_f32(), c.position.1.to_f32());
            let target = m
                .world
                .regions
                .values()
                .filter(|r| r.owner == human)
                .map(|r| r.settlement.position)
                .min_by(|a, b| ((a.0.to_f32() - x).hypot(a.1.to_f32() - z)).total_cmp(&(b.0.to_f32() - x).hypot(b.1.to_f32() - z)))?;
            Some((f.id, (target.0, ntw_sim::fixed::Fixed20::from_f64(target.1.to_f32() as f64 - 3.0))))
        })
    };
    if let Some((f, to)) = mv {
        println!(" move army {}", f.raw());
        command(host, CampaignCommand::MoveForce { force: f, to });
    }
}

/// NR-B2's additions at turn 1, as recruitment completes them (`spawn_recruited_unit`): a unit
/// joining a french garrison, a new colonel-led army in a french settlement without a garrison,
/// and a ship forming a new captain-led navy. Nothing is removed and no turn passes.
fn add_units(host: &mut ScriptHost) {
    let mut st = host.state_mut();
    let m = &mut st.model;
    let human = m.faction_by_key(HUMAN).expect("france").id;
    let mut regions: Vec<_> = m.world.regions.values().filter(|r| r.owner == human).map(|r| (r.key.clone(), r.id, r.garrison.is_some())).collect();
    regions.sort();
    let land = |m: &CampaignModel, r: RegionId| m.recruitable_units(r).into_iter().filter(|e| e.flags == 0).map(|e| e.unit_key).find(|u| m.rules.units.get(u).is_some_and(|x| !x.is_naval));
    let ship = |m: &CampaignModel, r: RegionId| m.recruitable_units(r).into_iter().filter(|e| e.flags == 0).map(|e| e.unit_key).find(|u| m.rules.units.get(u).is_some_and(|x| x.is_naval));
    let picks = [
        regions.iter().find(|r| r.2).and_then(|r| Some((r.0.clone(), r.1, land(m, r.1)?))),
        regions.iter().find(|r| !r.2).and_then(|r| Some((r.0.clone(), r.1, land(m, r.1)?))),
        regions.iter().find_map(|r| Some((r.0.clone(), r.1, ship(m, r.1)?))),
    ];
    for (key, id, unit) in picks.into_iter().flatten() {
        println!(" raise {unit} in {key}");
        m.spawn_recruited_unit(id, unit);
    }
}

/// NR-B3's one addition: a unit joining the first (by key) french garrison with room.
fn add_garrison_unit(host: &mut ScriptHost) {
    let mut st = host.state_mut();
    let m = &mut st.model;
    let human = m.faction_by_key(HUMAN).expect("france").id;
    let mut regions: Vec<_> = m
        .world
        .regions
        .values()
        .filter(|r| r.owner == human && r.garrison.and_then(|g| m.world.forces.get(&g)).is_some_and(|f| f.units.len() < 20))
        .map(|r| (r.key.clone(), r.id))
        .collect();
    regions.sort();
    let pick = regions.iter().find_map(|(k, id)| {
        let u = m.recruitable_units(*id).into_iter().filter(|e| e.flags == 0).map(|e| e.unit_key).find(|u| m.rules.units.get(u).is_some_and(|x| !x.is_naval))?;
        Some((k.clone(), *id, u))
    });
    let (key, id, unit) = pick.expect("a french garrison that can recruit");
    let forces = m.world.forces.len();
    println!(" raise {unit} in {key} (joins the garrison)");
    m.spawn_recruited_unit(id, unit);
    assert_eq!(m.world.forces.len(), forces, "the unit joins the garrison");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first().map(PathBuf::from) else {
        eprintln!("usage: nr_test_saves <out dir> [round2 | turns [n]]");
        std::process::exit(2);
    };
    let mode = args.get(1).map_or("round2", String::as_str);
    let inst = Install::open();
    let mut outputs: Vec<(&str, Vec<u8>)> = Vec::new();
    match mode {
        "round2" => {
            println!("== NR-4: new campaign, turn 1 (human flag in the faction record, HUMAN AI manager)");
            let mut h = inst.new_campaign();
            outputs.push(("NR-4 France turn 1.save", save(&inst, &mut h, "NR-4")));

            println!("== NR-B1: NR-4 with the AI block marked for rebuilding (version 12), nothing else");
            let mut h = inst.new_campaign();
            outputs.push(("NR-B1 France turn 1 AI rebuilt.save", save_with(&inst, &mut h, "NR-B1", AiBlock::Version(12))));

            println!("== NR-B2: NR-4 plus raised units (new colonel army, new navy, a unit in a garrison), AI block kept");
            let mut h = inst.new_campaign();
            let before: Vec<i32> = h.model().world.characters.keys().map(|c| c.raw()).collect();
            add_units(&mut h);
            assert!(before.iter().all(|c| h.model().world.characters.contains_key(&ntw_sim::campaign::CharacterId(*c))), "nothing removed");
            outputs.push(("NR-B2 France turn 1 new units.save", save_with(&inst, &mut h, "NR-B2", AiBlock::Version(13))));

            println!("== NR-B3: NR-4 plus one unit joining a french garrison (no new character or force), AI block kept");
            let mut h = inst.new_campaign();
            add_garrison_unit(&mut h);
            outputs.push(("NR-B3 France turn 1 garrison unit.save", save_with(&inst, &mut h, "NR-B3", AiBlock::Version(13))));
        }
        "years" => {
            // A long run across year ends (traits, ancillaries, natural deaths; 0-G characters):
            // every rule must hold on the save after it.
            let turns: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(26);
            println!("== NR-Y: {turns} End Turns");
            let mut h = inst.new_campaign();
            let start: std::collections::BTreeSet<i32> = h.model().world.characters.keys().map(|c| c.raw()).collect();
            for _ in 0..turns {
                end_turn(&mut h);
            }
            let m = h.model();
            let now: std::collections::BTreeSet<i32> = m.world.characters.keys().map(|c| c.raw()).collect();
            let traits: usize = m.world.character_details.values().map(|d| d.traits.len()).sum();
            let anc: usize = m.world.character_details.values().map(|d| d.ancillaries.len()).sum();
            println!("  {} of {} start characters gone, {} new; {traits} traits, {anc} ancillaries", start.difference(&now).count(), start.len(), now.difference(&start).count());
            drop(m);
            let bytes = save(&inst, &mut h, "NR-Y");
            // The written traits, points and ancillaries are the model's, for every character.
            let back = ntw_campaign::read(&bytes, &inst.db).expect("reload");
            let m = h.model();
            let mut differ = 0;
            let stale = m.world.character_details.keys().filter(|id| !m.world.characters.contains_key(id)).count();
            println!("  details of characters no longer in the world (left by other removal paths; not saved): {stale}");
            for (id, d) in m.world.character_details.iter().filter(|(id, _)| m.world.characters.contains_key(id)) {
                let Some(b) = back.model.world.character_details.get(id) else {
                    differ += 1;
                    println!("    {}: not read back (in the model: {})", id.raw(), m.world.characters.contains_key(id));
                    continue;
                };
                if b.traits != d.traits || b.ancillaries != d.ancillaries {
                    differ += 1;
                    println!("    {}: {:?} {:?} read back {:?} {:?}", id.raw(), d.traits, d.ancillaries, b.traits, b.ancillaries);
                }
            }
            println!("  characters whose traits or ancillaries read back differently: {differ}");
            outputs.push(("NR-Y France 26 turns.save", bytes));
        }
        "round5" => {
            // Every save gets the human's victory option (SAVE_COMPAT.md §17); NR-7 / NR-8 are
            // NR-5 / NR-6 again; NR-C1.. take one kind of NR-7's changes onto turn 1 each.
            let turns: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            println!("== NR-4: new campaign, turn 1 (victory conditions fixed)");
            let mut h = inst.new_campaign();
            outputs.push(("NR-4 France turn 1.save", save(&inst, &mut h, "NR-4")));
            println!("== NR-B3: NR-4 plus one unit joining a french garrison");
            let mut h = inst.new_campaign();
            add_garrison_unit(&mut h);
            outputs.push(("NR-B3 France turn 1 garrison unit.save", save(&inst, &mut h, "NR-B3")));
            println!("== NR-7: {turns} End Turns");
            let mut after = inst.new_campaign();
            for _ in 0..turns {
                end_turn(&mut after);
            }
            outputs.push(("NR-7 France AI turns.save", save(&inst, &mut after, "NR-7")));
            println!("== NR-8: recruit, merge, build, move, then 3 End Turns");
            let mut h = inst.new_campaign();
            player_actions(&mut h);
            for _ in 0..3 {
                end_turn(&mut h);
            }
            outputs.push(("NR-8 France recruit merge build.save", save(&inst, &mut h, "NR-8")));
            let m1 = after.model().clone();
            type Cat = fn(&mut CampaignModel, &CampaignModel) -> String;
            let cats: [(&str, &str, Cat); 8] = [
                ("NR-C1 removals.save", "NR-C1", bisect::removals),
                ("NR-C2 moves.save", "NR-C2", bisect::moves),
                ("NR-C3 buildings and queues.save", "NR-C3", bisect::regions),
                ("NR-C4 factions and diplomacy.save", "NR-C4", bisect::factions),
                ("NR-C5 new forces and units.save", "NR-C5", bisect::additions),
                ("NR-C6 unit strengths.save", "NR-C6", bisect::strengths),
                ("NR-C7 calendar.save", "NR-C7", bisect::calendar),
                ("NR-C8 captured regions.save", "NR-C8", bisect::captures),
            ];
            for (file, name, f) in cats {
                let mut h = inst.new_campaign();
                let what = {
                    let mut st = h.state_mut();
                    f(&mut st.model, &m1)
                };
                println!("== {name}: turn 1 plus NR-7's {what}");
                outputs.push((file, save(&inst, &mut h, name)));
            }
            println!("== NR-C8b: NR-C8 with the captured region's slots left with the old owner");
            let mut h = inst.new_campaign();
            {
                let mut st = h.state_mut();
                bisect::captures(&mut st.model, &m1);
            }
            outputs.push(("NR-C8b captured regions old slots.save", save_full(&inst, &mut h, "NR-C8b", AiBlock::AsWritten, None, true)));
            // (NR-7's script values equal turn 1's: no save for them.)
            let (v1, _) = after.save_values();
            let mut h = inst.new_campaign();
            let (v0, _) = h.save_values();
            assert_eq!(script_values_out(&v0), script_values_out(&v1), "script values changed: add a bisect save for them");
        }
        "turns" => {
            // After AI turns: the AI block stays version 13, kept in step with the world
            // (`cai_world::sync`, SAVE_COMPAT.md §10).
            let turns: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            println!("== NR-5: {turns} End Turns");
            let mut h = inst.new_campaign();
            for _ in 0..turns {
                end_turn(&mut h);
            }
            outputs.push(("NR-5 France AI turns.save", save(&inst, &mut h, "NR-5")));

            println!("== NR-6: recruit, merge, build, move, then End Turns");
            let mut h = inst.new_campaign();
            player_actions(&mut h);
            for _ in 0..3 {
                end_turn(&mut h);
            }
            outputs.push(("NR-6 France recruit merge build.save", save(&inst, &mut h, "NR-6")));
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    }

    std::fs::create_dir_all(&out).expect("out dir");
    for (name, bytes) in &outputs {
        write(&out, name, bytes);
    }
}

/// Unix seconds now (the ESF header's timestamp, as `F5` in the game writes it).
fn now() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32)
}

/// Bisect categories (`bisect` mode): the turn-1 model `m` with one kind of change taken from the
/// model `after` some End Turns. Each returns a one-line summary of what it changed.
mod bisect {
    use std::collections::BTreeSet;

    use ntw_sim::campaign::{CampaignModel, CharacterId, ForceId};

    /// Characters, forces and units gone in `after` are removed (a surviving force keeps a
    /// commander who is gone, so no force is left without one).
    pub fn removals(m: &mut CampaignModel, after: &CampaignModel) -> String {
        let w = &mut m.world;
        let gone_forces: Vec<ForceId> = w.forces.keys().filter(|f| !after.world.forces.contains_key(f)).copied().collect();
        for f in &gone_forces {
            w.forces.remove(f);
        }
        for r in w.regions.values_mut() {
            if r.garrison.is_some_and(|g| gone_forces.contains(&g)) {
                r.garrison = None;
            }
            if r.fleet.is_some_and(|g| gone_forces.contains(&g)) {
                r.fleet = None;
            }
        }
        w.embarked.retain(|a, n| !gone_forces.contains(a) && !gone_forces.contains(n));
        let commanders: BTreeSet<CharacterId> = w.forces.values().filter_map(|f| f.commander).collect();
        let gone_chars: Vec<CharacterId> =
            w.characters.keys().filter(|c| !after.world.characters.contains_key(c) && !commanders.contains(c)).copied().collect();
        for c in &gone_chars {
            w.characters.remove(c);
            w.character_details.remove(c);
        }
        let mut units = 0;
        for f in w.forces.values_mut() {
            let keep: BTreeSet<_> = after.world.forces.get(&f.id).map(|a| a.units.iter().map(|u| u.id).collect()).unwrap_or_default();
            let before = f.units.clone();
            f.units.retain(|u| keep.contains(&u.id) || u.character.is_some_and(|c| commanders.contains(&c)));
            if f.units.is_empty() {
                // Keep the force valid (at least one unit).
                f.units = before;
                continue;
            }
            units += before.len() - f.units.len();
            for u in &mut f.units {
                if u.character.is_some_and(|c| gone_chars.contains(&c)) {
                    u.character = None;
                }
            }
        }
        format!("removed {} forces, {} characters, {} units", gone_forces.len(), gone_chars.len(), units)
    }

    /// Positions and action points of characters who stand in the field (no garrison) in both.
    pub fn moves(m: &mut CampaignModel, after: &CampaignModel) -> String {
        let garrisons = |mm: &CampaignModel| -> BTreeSet<ForceId> { mm.world.regions.values().filter_map(|r| r.garrison).collect() };
        let (g0, g1) = (garrisons(m), garrisons(after));
        let force_of = |mm: &CampaignModel, c: CharacterId| mm.world.forces.values().find(|f| f.commander == Some(c)).map(|f| f.id);
        let mut n = 0;
        let ids: Vec<CharacterId> = m.world.characters.keys().copied().collect();
        for c in ids {
            let Some(a) = after.world.characters.get(&c) else { continue };
            let (f0, f1) = (force_of(m, c), force_of(after, c));
            if f0 != f1 || f0.is_some_and(|f| g0.contains(&f) || g1.contains(&f)) {
                continue;
            }
            if m.world.embarked.contains_key(&f0.unwrap_or(ForceId(0))) || after.world.embarked.contains_key(&f0.unwrap_or(ForceId(0))) {
                continue;
            }
            let ch = m.world.characters.get_mut(&c).expect("character");
            if ch.garrisoned_in != a.garrisoned_in {
                continue;
            }
            if ch.position != a.position {
                n += 1;
            }
            ch.position = a.position;
            ch.movement_points = a.movement_points;
            ch.max_movement_points = a.max_movement_points;
        }
        format!("moved {n} characters")
    }

    /// Region economy (buildings, roads, construction, recruitment queues, figures) of regions
    /// with the same owner in both.
    pub fn regions(m: &mut CampaignModel, after: &CampaignModel) -> String {
        let mut n = 0;
        for (id, r) in m.world.regions.iter_mut() {
            let Some(a) = after.world.regions.get(id) else { continue };
            if a.owner != r.owner {
                continue;
            }
            let changed = r.slots != a.slots || r.road != a.road || r.construction != a.construction || r.recruitment_queue != a.recruitment_queue;
            r.slots = a.slots.clone();
            r.road = a.road.clone();
            r.construction = a.construction.clone();
            r.recruitment_queue = a.recruitment_queue.clone();
            r.population = a.population;
            r.gdp = a.gdp;
            r.town_wealth = a.town_wealth;
            r.town_wealth_growth = a.town_wealth_growth;
            r.wealth_growth_offset = a.wealth_growth_offset;
            r.discontent_growth = a.discontent_growth;
            r.tax_exempt = a.tax_exempt;
            n += usize::from(changed);
        }
        format!("{n} regions with changed buildings / queues")
    }

    /// Faction state: treasury, taxes, diplomacy, relationships, trade, bankruptcy.
    pub fn factions(m: &mut CampaignModel, after: &CampaignModel) -> String {
        let mut stances = 0;
        for (id, f) in m.world.factions.iter_mut() {
            let Some(a) = after.world.factions.get(id) else { continue };
            stances += a.diplomacy.iter().filter(|(k, v)| f.diplomacy.get(k) != Some(v)).count();
            f.treasury = a.treasury;
            f.tax_lower = a.tax_lower.clone();
            f.tax_upper = a.tax_upper.clone();
            f.diplomacy = a.diplomacy.clone();
        }
        m.world.relationships = after.world.relationships.clone();
        m.world.trade_accumulated = after.world.trade_accumulated.clone();
        m.world.bankrupt_turns = after.world.bankrupt_turns.clone();
        format!("{stances} stance changes")
    }

    /// New forces (with their new commanders) and new units in forces that exist at turn 1.
    pub fn additions(m: &mut CampaignModel, after: &CampaignModel) -> String {
        let w = &mut m.world;
        let existing_units: BTreeSet<_> = w.forces.values().flat_map(|f| f.units.iter().map(|u| u.id)).collect();
        let mut forces = 0;
        let mut chars = 0;
        let mut units = 0;
        let mut new_forces = Vec::new();
        for (fid, f) in &after.world.forces {
            if w.forces.contains_key(fid) {
                continue;
            }
            let Some(c) = f.commander else { continue };
            if w.characters.contains_key(&c) || f.units.iter().any(|u| existing_units.contains(&u.id)) {
                continue;
            }
            let mut force = f.clone();
            for u in &mut force.units {
                if u.character.is_some_and(|x| x != c) {
                    u.character = None;
                }
            }
            w.characters.insert(c, after.world.characters[&c].clone());
            if let Some(d) = after.world.character_details.get(&c) {
                w.character_details.insert(c, d.clone());
            }
            units += force.units.len();
            w.forces.insert(*fid, force);
            new_forces.push(*fid);
            forces += 1;
            chars += 1;
        }
        let all_units: BTreeSet<_> = w.forces.values().flat_map(|f| f.units.iter().map(|u| u.id)).collect();
        for (id, f) in w.forces.iter_mut() {
            if new_forces.contains(id) {
                continue;
            }
            let Some(a) = after.world.forces.get(id) else { continue };
            for u in &a.units {
                if !all_units.contains(&u.id) && f.units.len() < 20 && u.character.is_none() {
                    f.units.push(u.clone());
                    units += 1;
                }
            }
        }
        for (rid, r) in w.regions.iter_mut() {
            let a = &after.world.regions[rid];
            if r.garrison.is_none() && a.garrison.is_some_and(|g| new_forces.contains(&g)) && a.owner == r.owner {
                r.garrison = a.garrison;
            }
            if r.fleet.is_none() && a.fleet.is_some_and(|g| new_forces.contains(&g)) && a.owner == r.owner {
                r.fleet = a.fleet;
            }
        }
        // A new commander inside a settlement whose garrison this model does not give him stands
        // outside it.
        for f in &new_forces {
            let c = w.forces[f].commander.expect("commander");
            let garrison_of = w.regions.values().find(|r| r.garrison == Some(*f)).map(|r| r.id);
            if let Some(ch) = w.characters.get_mut(&c) {
                ch.garrisoned_in = garrison_of;
            }
        }
        for (a, n) in &after.world.embarked {
            if new_forces.contains(a) && w.forces.contains_key(n) {
                w.embarked.insert(*a, *n);
            }
        }
        w.next_id = w.next_id.max(after.world.next_id);
        format!("added {forces} forces, {chars} characters, {units} units")
    }

    /// Unit strengths of units in both.
    pub fn strengths(m: &mut CampaignModel, after: &CampaignModel) -> String {
        let mut n = 0;
        for (id, f) in m.world.forces.iter_mut() {
            let Some(a) = after.world.forces.get(id) else { continue };
            for u in &mut f.units {
                if let Some(x) = a.units.iter().find(|x| x.id == u.id)
                    && (x.men, x.max_men) != (u.men, u.max_men)
                {
                    u.men = x.men;
                    u.max_men = x.max_men;
                    n += 1;
                }
            }
        }
        format!("{n} unit strengths")
    }

    /// The calendar (turn and date) and the random state.
    pub fn calendar(m: &mut CampaignModel, after: &CampaignModel) -> String {
        m.calendar = after.calendar;
        m.rng = after.rng;
        format!("turn {}", m.calendar.turn_number())
    }

    /// Regions that changed owner: owner, slots, queues; the old garrison leaves (its commander
    /// stands outside), the new owner's garrison when that force exists here.
    pub fn captures(m: &mut CampaignModel, after: &CampaignModel) -> String {
        let mut n = 0;
        let ids: Vec<_> = m.world.regions.keys().copied().collect();
        for id in ids {
            let a = &after.world.regions[&id];
            let (owner, old_garrison) = {
                let r = &m.world.regions[&id];
                (r.owner, r.garrison)
            };
            if a.owner == owner {
                continue;
            }
            n += 1;
            let new_garrison = a.garrison.filter(|g| m.world.forces.get(g).is_some_and(|f| f.faction == a.owner));
            let new_fleet = a.fleet.filter(|g| m.world.forces.get(g).is_some_and(|f| f.faction == a.owner));
            // The new garrison may have marched out of another settlement it garrisoned.
            for r in m.world.regions.values_mut() {
                if new_garrison.is_some() && r.garrison == new_garrison {
                    r.garrison = None;
                }
            }
            {
                let r = m.world.regions.get_mut(&id).expect("region");
                r.owner = a.owner;
                r.slots = a.slots.clone();
                r.road = a.road.clone();
                r.construction = a.construction.clone();
                r.recruitment_queue = a.recruitment_queue.clone();
                r.garrison = new_garrison;
                r.fleet = new_fleet;
            }
            if let Some(g) = old_garrison.filter(|g| Some(*g) != new_garrison)
                && let Some(c) = m.world.forces.get(&g).and_then(|f| f.commander)
                && let Some(ch) = m.world.characters.get_mut(&c)
            {
                ch.garrisoned_in = None;
            }
            if let Some(g) = new_garrison
                && let Some(c) = m.world.forces.get(&g).and_then(|f| f.commander)
                && let Some(ch) = m.world.characters.get_mut(&c)
            {
                ch.garrisoned_in = Some(id);
                ch.position = m.world.regions[&id].settlement.position;
            }
            println!(
                "  {}: owner {} -> {}, garrison {:?} -> {:?} (commander {:?})",
                m.world.regions[&id].key,
                owner.raw(),
                a.owner.raw(),
                old_garrison.map(|g| g.raw()),
                new_garrison.map(|g| g.raw()),
                new_garrison.and_then(|g| m.world.forces.get(&g)).and_then(|f| f.commander).map(|c| (c.raw(), m.world.characters.get(&c).map(|x| x.garrisoned_in)))
            );
        }
        format!("{n} regions changed owner")
    }
}

/// Puts back the old owner on every slot residence (not the settlement's) of each region whose
/// owner differs from the start position's: how the writer left captured regions before round 5.
fn undo_handover(source: &EsfFile, tree: &mut EsfFile) {
    use ntw_formats::esf::{EsfNode, EsfRecord};
    fn regions_mut(root: &mut EsfRecord) -> Vec<&mut EsfRecord> {
        fn find<'a>(r: &'a mut EsfRecord, path: &[&str]) -> Option<&'a mut EsfRecord> {
            let Some((first, rest)) = path.split_first() else { return Some(r) };
            let next = r.children.iter_mut().find_map(|c| match c {
                EsfNode::Record(x) if x.name == *first => Some(&mut **x),
                _ => None,
            })?;
            find(next, rest)
        }
        let Some(m) = find(root, &["CAMPAIGN_ENV", "CAMPAIGN_MODEL", "WORLD", "REGION_MANAGER"]) else { return Vec::new() };
        let mut out = Vec::new();
        for c in &mut m.children {
            if let EsfNode::RecordArray(a) = c
                && a.name == "REGIONS_ARRAY"
            {
                for n in a.items.iter_mut().flat_map(|it| it.iter_mut()) {
                    if let EsfNode::Record(r) = n {
                        out.push(&mut **r);
                    }
                }
            }
        }
        out
    }
    fn set_owner(r: &mut EsfRecord, from: u32, to: u32, skip_settlement: bool) {
        if skip_settlement && r.name == "SETTLEMENT" {
            return;
        }
        if r.name == "GARRISON_RESIDENCE" && r.get_u32(0) == Some(to) {
            r.children[0] = EsfNode::U32(from);
        }
        for c in &mut r.children {
            match c {
                EsfNode::Record(x) => set_owner(x, from, to, skip_settlement),
                EsfNode::RecordArray(a) => {
                    for n in a.items.iter_mut().flat_map(|it| it.iter_mut()) {
                        if let EsfNode::Record(x) = n {
                            set_owner(x, from, to, skip_settlement);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut src = source.clone();
    let old: Vec<Option<u32>> = regions_mut(&mut src.root).iter().map(|r| r.get_u32(20)).collect();
    for (r, o) in regions_mut(&mut tree.root).into_iter().zip(old) {
        if let (Some(o), Some(n)) = (o, r.get_u32(20))
            && o != n
        {
            set_owner(r, o, n, true);
        }
    }
}
