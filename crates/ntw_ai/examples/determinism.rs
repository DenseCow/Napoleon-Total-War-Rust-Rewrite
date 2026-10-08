//! Determinism harness (BACKLOG §0; notes `analysis/fidelity/MIDDLEWARE_VERIFY.md` §4): runs the
//! battle sim + battle AI, or the campaign turn loop + campaign AI, and prints a state hash trace.
//! `twice` runs the same trace in two separate processes and compares them line by line, so
//! anything that differs between runs (hash-map order, addresses, time, thread scheduling) shows
//! up as the first diverging tick / turn. Reads the install only.
//!
//! ```text
//! cargo run -p ntw_ai --release --example determinism -- battle [ticks] [seed]
//! cargo run -p ntw_ai --release --example determinism -- campaign [turns]
//! cargo run -p ntw_ai --release --example determinism -- twice battle [ticks] [seed]
//! cargo run -p ntw_ai --release --example determinism -- twice campaign [turns]
//! ```
//! Set `NTW_DATA_DIR` for another install location.

use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::sync::Arc;

use ntw_ai::battle::BattleAi;
use ntw_ai::campaign::driver;
use ntw_ai::campaign::keys::read_ai_keys;
use ntw_ai::campaign::CampaignAiData;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::pack::Vfs;
use ntw_sim::battle::model::{Battle, BattleResult, LandUnit};
use ntw_sim::battle::shooting::MissileWeapon;
use ntw_sim::campaign::Terrain;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// One unit from its DB rows (as `tests/real_battle.rs`; speeds PLACEHOLDER as there).
fn make(db: &GameDatabase, key: &str, id: u32, side: u8, pos: (f32, f32), facing: f32) -> Option<LandUnit> {
    let rec = db.unit(key)?;
    let s = db.unit_stats(key)?;
    let cav = rec.category == "cavalry";
    let mut u = LandUnit::new(id, side, s.num_men.max(0) as u32, pos);
    u.facing = facing;
    u.melee_attack = s.melee_attack;
    u.charge_bonus = s.charge_bonus;
    u.melee_defence = s.melee_defence;
    u.armour = s.armour;
    u.shield = s.unknown_188;
    u.morale_stat = s.morale;
    u.is_cavalry = cav;
    if s.ammunition > 0
        && let Some(p) = db.primary_projectile(s)
    {
        u.missile = Some(MissileWeapon {
            range: p.effective_range,
            accuracy: s.accuracy as f32,
            reload_skill: s.reload_skill,
            reload_time_s: p.reload_time,
            damage: p.damage,
            projectiles_per_shot: p.projectiles_per_shot.max(1) as u32,
            is_artillery: s.is_artillery,
            guns: s.num_guns.max(0) as u32,
            ballistics: Default::default(),
        });
        u.ammunition = s.ammunition as u32;
    }
    (u.walk_speed, u.run_speed) = if cav { (3.0, 9.0) } else { (1.5, 4.0) };
    Some(u)
}

/// France v Austria, both sides played by the AI; prints `tick hash` every 50 ticks and at the end.
fn battle(ticks: u32, seed: u32) -> Option<()> {
    const FRANCE: [&str; 4] = ["Inf_Line_French_Fusiliers", "Inf_Line_French_18th_Ligne", "Inf_Gren_French_Grenadiers", "Cav_Heavy_French_Cuirassiers"];
    const AUSTRIA: [&str; 4] = [
        "Inf_Line_Austrian_German_Fusiliers",
        "Inf_Line_Austrian_Hungarian_Fusiliers",
        "Inf_Gren_Austrian_German_Grenadiers",
        "Cav_Light_Austrian_1st_Hussars",
    ];
    let db = GameDatabase::from_install(data_dir()).ok()?;
    let mut b = Battle::with_rules(seed, db.kv_morale, db.kv_fatigue, db.kv_rules_sim);
    let up = std::f32::consts::FRAC_PI_2;
    let xs = [-75.0, -10.0, 55.0, 130.0];
    for k in 0..4 {
        b.add_unit(make(&db, FRANCE[k], 1 + k as u32, 0, (xs[k], -150.0), up)?);
        b.add_unit(make(&db, AUSTRIA[k], 11 + k as u32, 1, (xs[k], 150.0), -up)?);
    }
    let mut ai = BattleAi::new(&[0, 1]);
    let mut n = 0;
    while n < ticks && b.battle_result() == BattleResult::Ongoing {
        ai.update(&mut b);
        b.step();
        n += 1;
        if n % 50 == 0 {
            println!("tick {n} {:016x}", b.state_hash());
        }
    }
    println!("end tick {n} {:016x} {:?}", b.state_hash(), b.battle_result());
    Some(())
}

/// The eur_napoleon startpos, france human, `turns` End Turns with the campaign AI; prints
/// `turn hash` after each turn.
fn campaign(turns: u32) -> Option<()> {
    let dir = data_dir();
    let sp = dir.join(r"campaigns\eur_napoleon\startpos.esf");
    let db = GameDatabase::from_install(&dir).ok()?;
    let data = Arc::new(CampaignAiData::from_install(&dir, &db).ok()?);
    let mut loaded = ntw_campaign::read_file(&sp, &db).ok()?;
    loaded.set_human("france");
    let vfs = Vfs::open_install(&dir).ok()?;
    let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    let map = CampaignMap::load(&files, &loaded.info.map_key).ok()?;
    loaded.model.terrain = Some(Terrain(Arc::new(ntw_campaign::pathing::build_grid(&map))));
    let keys = read_ai_keys(&ntw_formats::esf::EsfFile::open(&sp).ok()?.root);
    let mut m = loaded.model;
    let mut ctx = driver::context_for(&m, "eur_napoleon");
    ctx.ai_keys = keys;
    println!("turn 0 {:016x}", m.state_hash());
    for t in 1..=turns {
        let (_, reports) = driver::end_turn(&mut m, &data, &ctx);
        let orders: usize = reports.iter().map(|r| r.accepted).sum();
        println!("turn {t} {:016x} orders {orders}", m.state_hash());
        // NTW_TREASURY=1: every faction's treasury after the turn (effects slot 0-F before/after runs).
        if std::env::var_os("NTW_TREASURY").is_some() {
            let t: Vec<String> = m.world.factions.values().map(|f| format!("{}={}", f.key, f.treasury)).collect();
            println!("  treasury {}", t.join(" "));
        }
    }
    Some(())
}

/// Runs this program twice with `args` and compares the traces.
fn twice(args: &[String]) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cannot find this program: {e}");
            return ExitCode::FAILURE;
        }
    };
    let run = || Command::new(&exe).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    let (a, b) = match (run(), run()) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("run failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    if la.is_empty() {
        eprintln!("no trace (is the game installed?)");
        return ExitCode::FAILURE;
    }
    match la.iter().zip(&lb).position(|(x, y)| x != y) {
        Some(i) => {
            println!("DIVERGED at line {}: \n  run 1: {}\n  run 2: {}", i + 1, la[i], lb[i]);
            ExitCode::FAILURE
        }
        None if la.len() != lb.len() => {
            println!("DIVERGED: {} vs {} lines", la.len(), lb.len());
            ExitCode::FAILURE
        }
        None => {
            println!("IDENTICAL: {} lines, last: {}", la.len(), la.last().copied().unwrap_or_default());
            ExitCode::SUCCESS
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize, d: u32| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let ok = match args.first().map(String::as_str) {
        Some("twice") => return twice(&args[1..]),
        Some("battle") => battle(num(1, 3000), num(2, 7)),
        Some("campaign") => campaign(num(1, 3)),
        _ => {
            eprintln!("usage: determinism battle [ticks] [seed] | campaign [turns] | twice <battle|campaign> ...");
            return ExitCode::FAILURE;
        }
    };
    if ok.is_some() {
        ExitCode::SUCCESS
    } else {
        eprintln!("setup failed (is the game installed?)");
        ExitCode::FAILURE
    }
}
