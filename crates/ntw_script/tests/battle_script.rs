//! Runs the ORIGINAL Waterloo battle script (and `data/scripting_library.lua`) from the player's
//! install (read-only) against a battle snapshot built from the battle file. Skipped without an
//! install. See the script log with `-- --nocapture`.

use std::path::PathBuf;

use ntw_formats::battle_spec::BattleSpec;
use ntw_formats::pack::Vfs;
use ntw_script::ScriptSource;
use ntw_script::battle_script::{BattleScriptFacts, BattleScriptHost, BattleScriptRequest, ScriptUnit};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

/// Facts from the battle file alone: every unit at its file position with its file men.
fn facts(spec: &BattleSpec) -> BattleScriptFacts {
    let mut f = BattleScriptFacts::default();
    for alliance in &spec.alliances {
        let mut armies = Vec::new();
        let reinforcement_from = alliance.armies.len();
        for (ri, army) in alliance.armies.iter().chain(&alliance.reinforcements).enumerate() {
            let mut list = Vec::new();
            for su in &army.units {
                let men = su.num_soldiers.unwrap_or(100);
                list.push(f.units.len());
                f.units.push(ScriptUnit {
                    id: f.units.len() as u32 + 1,
                    name: su.unit_type.clone(),
                    script_name: su.script_name.clone().unwrap_or_default(),
                    men,
                    initial_men: men,
                    position: [su.position.0, 0.0, su.position.1],
                    infantry: su.category == "infantry",
                    cavalry: su.category == "cavalry" || su.category == "dragoons",
                    artillery: su.category == "artillery",
                    off_field: ri >= reinforcement_from,
                    ..Default::default()
                });
            }
            armies.push(list);
        }
        f.alliances.push(armies);
    }
    f
}

#[test]
fn waterloo_script_deploys_the_prussians() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let vfs = Vfs::open_install(&dir).unwrap();
    let xml = "napoleon_historical_battles/waterloo/waterloo_battle.xml";
    let spec = BattleSpec::parse(&vfs.read(xml).unwrap()).unwrap();
    let script = vfs.read("napoleon_historical_battles/waterloo/waterloo_battle.battle_script").unwrap();
    let mut f = facts(&spec);
    let host = BattleScriptHost::new(ScriptSource::from_install(&dir).unwrap(), f.clone(), 1).unwrap();
    host.load("@waterloo_battle.battle_script", &script).expect("script runs");
    host.phase("Deployment");
    host.phase("Deployed");
    let mut requests = Vec::new();
    let mut deployed_at = None;
    // 25 minutes of battle time with nobody hurt: the script deploys the Prussians after its
    // 40th 30-second check (20 minutes), CONFIRMED in Can_I_Spawn_Reinforcements.
    for step in 1..=3_000u64 {
        f.time_ms = step * 500;
        host.set_facts(f.clone());
        host.advance();
        let r = host.take_requests();
        if deployed_at.is_none() && r.iter().any(|r| matches!(r, BattleScriptRequest::DeployReinforcement { deploy: true, .. })) {
            deployed_at = Some(f.time_ms);
        }
        requests.extend(r);
    }
    let log = host.take_log();
    for l in log.iter().filter(|l| l.contains("ERROR") || l.contains("error")).take(40) {
        println!("{l}");
    }
    let stubs: std::collections::BTreeSet<&String> = log.iter().filter(|l| l.starts_with("UNKNOWN stub")).collect();
    println!("stubs used: {stubs:?}");
    println!("first deploy_reinforcement(true) at {deployed_at:?} ms, {} requests", requests.len());
    assert!(log.iter().all(|l| !l.starts_with("ERROR")), "script errors (see above)");
    let at = deployed_at.expect("the Prussians were deployed");
    // The 40th check of a 30 s repeating timer registered at the end of the 63 s intro cutscene.
    assert!((1_200_000..1_300_000).contains(&at), "deployed at {at} ms");
}

/// The land tutorial's script (TUT_Land) with its markers, and the player's actions it waits for:
/// it asks for the cannon to be selected (unit selection handler), then for an attack order on a
/// militia unit (command handler), then reacts with its scripted counter-attack.
#[test]
fn tutorial_script_follows_the_players_actions() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let vfs = Vfs::open_install(&dir).unwrap();
    let source = ScriptSource::from_install(&dir).unwrap();
    let rec = ntw_script::ui::frontend::read_battles(&source).into_iter().find(|r| r.key == "TUT_Land").expect("TUT_Land");
    let lower = rec.spec.to_ascii_lowercase();
    let stem = lower.strip_suffix(".xml").unwrap();
    let spec = BattleSpec::parse(&vfs.read(&lower).unwrap()).unwrap();
    let script = vfs.read(&format!("{stem}.battle_script")).unwrap();
    let mut f = facts(&spec);
    let id_of = |f: &BattleScriptFacts, name: &str| f.units.iter().find(|u| u.script_name == name).map(|u| u.id).unwrap();
    let cannon = id_of(&f, "British_Cannon_1");
    let militia = id_of(&f, "Spanish_Militia_C1_W1");
    let host = BattleScriptHost::new(ScriptSource::from_install(&dir).unwrap(), f.clone(), 1).unwrap();
    host.load("@battle_tutorial.battle_script", &script).expect("script runs");
    let markers: Vec<_> = host.take_requests().into_iter().filter(|r| matches!(r, BattleScriptRequest::Marker { .. })).collect();
    assert!(markers.len() >= 4, "two markers made and placed: {markers:?}");
    host.phase("Deployment");
    host.phase("Deployed");
    let mut t = 0u64;
    let mut run = |host: &BattleScriptHost, f: &mut BattleScriptFacts, ms: u64| {
        let end = t + ms;
        while t < end {
            t += 100;
            f.time_ms = t;
            host.set_facts(f.clone());
            host.advance();
        }
    };
    run(&host, &mut f, 5_000);
    let log = host.take_log();
    assert!(log.iter().any(|l| l == "Call Message 1"), "{log:?}");
    host.selection(Some(cannon), true);
    run(&host, &mut f, 1_000);
    let log = host.take_log();
    assert!(log.iter().any(|l| l == "Cannon Selected"), "{log:?}");
    host.take_requests();
    host.command("Attack Unit", Some(militia), false, "");
    let log = host.take_log();
    assert!(log.iter().any(|l| l == "Militia 1 Attacked"), "{log:?}");
    let attacks = host.take_requests().into_iter().filter(|r| matches!(r, BattleScriptRequest::AttackUnit { target, .. } if *target == cannon)).count();
    assert_eq!(attacks, 2, "both militia units counter-attack the cannon");
    run(&host, &mut f, 60_000);
    let log = host.take_log();
    for l in log.iter().filter(|l| l.starts_with("ERROR") || l.starts_with("UNKNOWN")) {
        println!("{l}");
    }
    assert!(log.iter().all(|l| !l.starts_with("ERROR")), "script errors (see above)");
}

/// Survey (debug aid): runs every shipped land battle's script for 40 minutes of battle time with
/// nobody hurt and counts the UNKNOWN stub calls and the orders the model does not carry out.
/// `cargo test -p ntw_script --test battle_script survey -- --ignored --nocapture`.
#[test]
#[ignore]
fn survey_every_battle_script() {
    use std::collections::BTreeMap;
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        return;
    }
    let vfs = Vfs::open_install(&dir).unwrap();
    let source = ScriptSource::from_install(&dir).unwrap();
    let mut stubs: BTreeMap<String, usize> = BTreeMap::new();
    let mut others: BTreeMap<String, usize> = BTreeMap::new();
    let mut handled: BTreeMap<String, usize> = BTreeMap::new();
    let mut errors: BTreeMap<String, usize> = BTreeMap::new();
    for rec in ntw_script::ui::frontend::read_battles(&source) {
        let lower = rec.spec.to_ascii_lowercase();
        let Some(stem) = lower.strip_suffix(".xml") else { continue };
        let (Ok(xml), Ok(script)) = (vfs.read(&lower), vfs.read(&format!("{stem}.battle_script"))) else { continue };
        let Ok(spec) = BattleSpec::parse(&xml) else { continue };
        if spec.is_naval() {
            continue;
        }
        let mut f = facts(&spec);
        if let Some(m) = spec.map_definition.as_deref().and_then(|d| d.trim_end_matches('/').rsplit('/').next())
            && let Ok(map) = ntw_formats::battle_terrain::BattleMap::load(&vfs, &m.to_ascii_lowercase())
        {
            f.buildings = map.buildings_near.iter().map(|b| (b.key.clone(), [b.position.0, 0.0, b.position.1])).collect();
        }
        let host = BattleScriptHost::new(ScriptSource::from_install(&dir).unwrap(), f.clone(), 1).unwrap();
        if let Err(e) = host.load("@battle_script", &script) {
            *errors.entry(format!("{}: load: {e}", rec.key)).or_default() += 1;
            continue;
        }
        host.phase("Deployment");
        host.phase("Deployed");
        for step in 1..=4_800u64 {
            f.time_ms = step * 500;
            host.set_facts(f.clone());
            host.advance();
            for r in host.take_requests() {
                let new = match &r {
                    BattleScriptRequest::Skirmish { .. } => "skirmish",
                    BattleScriptRequest::SelectDeployable { .. } => "select_deployable_object",
                    BattleScriptRequest::SpecialAbility { .. } => "perform_special_ability",
                    BattleScriptRequest::ShotType { .. } => "change_shot_type",
                    BattleScriptRequest::ScriptMorale { .. } => "morale_behavior_*",
                    BattleScriptRequest::DefendBuilding { .. } => "defend_building",
                    _ => "",
                };
                if !new.is_empty() {
                    *handled.entry(new.to_owned()).or_default() += 1;
                }
                if let BattleScriptRequest::Other { order, .. } = r {
                    *others.entry(order).or_default() += 1;
                }
            }
        }
        for l in host.take_log() {
            if let Some(s) = l.strip_prefix("UNKNOWN stub ") {
                *stubs.entry(s.to_owned()).or_default() += 1;
            } else if l.starts_with("ERROR") {
                let short: String = l.chars().take(160).collect();
                *errors.entry(format!("{}: {short}", rec.key)).or_default() += 1;
            }
        }
    }
    let mut v: Vec<_> = stubs.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("STUBS {v:?}");
    let mut o: Vec<_> = others.into_iter().collect();
    o.sort_by_key(|x| std::cmp::Reverse(x.1));
    println!("ORDERS {o:?}");
    println!("HANDLED {handled:?}");
    for (e, n) in errors.iter().take(60) {
        println!("ERR x{n} {e}");
    }
}
