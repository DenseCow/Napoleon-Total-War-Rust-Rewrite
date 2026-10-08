//! Runs the battle's ORIGINAL `.battle_script` during the battle (`ntw_script::battle_script`).
//!
//! - Loaded on entering the battle when the battle file has a script next to it
//!   (`<battle>.xml` → `<battle>.battle_script`, CONFIRMED layout of the shipped battles).
//! - Every model tick (after `tick_battle`): the snapshot of the units is handed to the scripts,
//!   phase changes become phase events ("Deployment", "Deployed"; the result goes to the command
//!   handlers), due timers fire, and the scripts' requests are applied to the model.
//! - `NAPOLEON_BATTLE_SCRIPTS=off` turns them off (test harness).

use bevy::prelude::*;
use ntw_script::ScriptSource;
use ntw_script::battle_script::{BattleScriptFacts, BattleScriptHost, BattleScriptRequest, ScriptUnit};
use ntw_sim::battle::model::{Battle, Reinforcement};
use ntw_sim::battle::attributes::ABILITY_NAMES;
use ntw_sim::battle::morale::ScriptMorale;
use ntw_sim::battle::orders::MoveSpeed;

use super::setup::BattleStart;
use super::{BattlePhase, BattleSim, UnitInfo};

/// The running battle script (not `Send`: the Lua state stays on the main thread).
pub struct BattleScripts {
    host: BattleScriptHost,
    shown: Option<BattlePhase>,
    last_tick: Option<u32>,
    /// The selection the scripts last heard about (`None` = not told yet).
    selected: Option<Option<u32>>,
}

/// Loads the battle script, if the battle has one. Exclusive system (the host is a non-send resource).
pub fn enter(world: &mut World) {
    world.remove_non_send::<BattleScripts>();
    if std::env::var("NAPOLEON_BATTLE_SCRIPTS").is_ok_and(|v| v == "off") {
        return;
    }
    let Some(spec) = world.get_resource::<BattleStart>().and_then(|s| s.spec.clone()) else { return };
    let Some(sim) = world.get_resource::<BattleSim>() else { return };
    let path = match spec.to_ascii_lowercase().strip_suffix(".xml") {
        Some(stem) => format!("{stem}.battle_script"),
        None => return,
    };
    let dir = crate::config::game_data_dir();
    let Ok(source) = ScriptSource::from_install(&dir) else { return };
    let Some(file) = source.find(&path) else {
        info!("Battle script: none for {spec}");
        return;
    };
    // The map's near buildings in file order (INFERRED: the scripts' battle:buildings() list).
    let buildings: Vec<(String, [f32; 3])> = crate::terrain::current_map()
        .map(|m| {
            m.buildings_near
                .iter()
                .map(|b| (b.key.clone(), [b.position.0, sim.battle.ground.height(b.position.0, b.position.1), b.position.1]))
                .collect()
        })
        .unwrap_or_default();
    let mut facts = facts_of(&sim.battle, &sim.info, &buildings);
    // The battle file's start view, (x, height, map y) = engine coordinates.
    facts.camera = sim.camera_start.unwrap_or(([0.0; 3], [0.0; 3]));
    let host = match BattleScriptHost::new(source, facts, sim.seed) {
        Ok(h) => h,
        Err(e) => {
            warn!("Battle script: host failed: {e}");
            return;
        }
    };
    if let Err(e) = host.load(&file.chunk_name, &file.bytes) {
        warn!("Battle script {path}: {e}");
    }
    flush_log(&host);
    info!("Battle script {path} loaded");
    world.insert_non_send(BattleScripts { host, shown: None, last_tick: None, selected: None });
}

/// One script step per model tick. Exclusive system.
pub fn tick(world: &mut World) {
    let Some(mut scripts) = world.remove_non_send::<BattleScripts>() else { return };
    let mut deferred = Vec::new();
    if let Some(mut sim) = world.get_resource_mut::<BattleSim>() {
        deferred = step(&mut scripts, &mut sim);
    }
    world.insert_non_send(scripts);
    for r in deferred {
        match r {
            BattleScriptRequest::Camera { target, position, .. } => {
                // The harness camera (NAPOLEON_BATTLE_CAMERA) wins over script cutscenes.
                if std::env::var_os("NAPOLEON_BATTLE_CAMERA").is_none() {
                    world.insert_resource(super::view::ScriptCamera(Some((position, target))));
                }
            }
            BattleScriptRequest::UiVisible { name, visible } => super::hud::set_component_visible(world, &name, visible),
            BattleScriptRequest::Marker { id, name, position, rotation_deg, scale, visible } => {
                let mut markers = world.get_resource_or_insert_with(super::markers::ScriptMarkers::default);
                markers.set(id, super::markers::MarkerState { name, position, rotation_deg, scale, visible });
            }
            _ => {}
        }
    }
}

fn step(s: &mut BattleScripts, sim: &mut BattleSim) -> Vec<BattleScriptRequest> {
    // The buildings were given at load time and do not change.
    s.host.set_facts(facts_of(&sim.battle, &sim.info, &[]));
    if s.shown != Some(sim.phase) {
        match sim.phase {
            BattlePhase::Deployment => s.host.phase("Deployment"),
            BattlePhase::Conflict => {
                if s.shown == Some(BattlePhase::Deployment) || s.shown.is_none() {
                    if s.shown.is_none() {
                        s.host.phase("Deployment");
                    }
                    s.host.phase("Deployed");
                }
            }
            BattlePhase::Finished => {
                let won = sim.outcome.winner() == Some(0);
                s.host.results(won);
            }
        }
        s.shown = Some(sim.phase);
    }
    if s.last_tick != Some(sim.battle.tick) {
        s.host.advance();
        s.last_tick = Some(sim.battle.tick);
    }
    // The player's actions: selection changes (INFERRED order: the old unit deselected, then the
    // new one selected), commands and camera inputs.
    if s.selected != Some(sim.selected) {
        let old = s.selected.flatten();
        s.selected = Some(sim.selected);
        if old.is_some() {
            s.host.selection(old, false);
        }
        if let Some(id) = sim.selected {
            s.host.selection(Some(id), true);
        }
    }
    for e in std::mem::take(&mut sim.script_events) {
        match e {
            super::ScriptEvent::Command { name, unit, bool1 } => s.host.command(name, unit, bool1, ""),
            super::ScriptEvent::Input(name) => s.host.input(name),
        }
    }
    let mut deferred = Vec::new();
    for r in s.host.take_requests() {
        match r {
            BattleScriptRequest::Camera { .. } | BattleScriptRequest::UiVisible { .. } | BattleScriptRequest::Marker { .. } => deferred.push(r),
            r => apply(&mut sim.battle, r),
        }
    }
    flush_log(&s.host);
    deferred
}

/// The snapshot the scripts read. Engine position = (x, height, map y): INFERRED from Waterloo,
/// where the script's Planchenoit point (z = −390) lies on the French (south, map y < 0) side.
pub fn facts_of(battle: &Battle, info: &[UnitInfo], buildings: &[(String, [f32; 3])]) -> BattleScriptFacts {
    let mut f = BattleScriptFacts { time_ms: u64::from(battle.tick) * 100, buildings: buildings.to_vec(), ..Default::default() };
    for (i, u) in battle.units.iter().enumerate() {
        let army = info.get(i).map_or((u.side as usize, u.army_index as usize), |x| x.army);
        while f.alliances.len() <= army.0 {
            f.alliances.push(Vec::new());
        }
        let armies = &mut f.alliances[army.0];
        while armies.len() <= army.1 {
            armies.push(Vec::new());
        }
        armies[army.1].push(i);
        let missile_range = u.missile.map_or(0.0, |w| w.range as f32);
        f.units.push(ScriptUnit {
            id: u.id,
            name: info.get(i).map_or_else(String::new, |x| x.name.clone()),
            script_name: info.get(i).and_then(|x| x.script_name.clone()).unwrap_or_default(),
            men: u.men,
            initial_men: u.max_men,
            position: [u.position.0, battle.ground.height(u.position.0, u.position.1), u.position.1],
            // INFERRED: degrees clockwise from +z (map +y); model facing is radians from +x.
            bearing: (90.0 - u.facing.to_degrees()).rem_euclid(360.0),
            moving: u.moved,
            routing: u.morale.is_routing_or_shattered(),
            leaving: u.left_field,
            cavalry: u.is_cavalry,
            infantry: !u.is_cavalry && !u.missile.is_some_and(|w| w.is_artillery),
            artillery: u.missile.is_some_and(|w| w.is_artillery),
            missile_range,
            ammo: u.ammunition,
            // Line units: the rounds per man the cartridge pool started with; artillery: one per volley.
            starting_ammo: u.ammo_pool.map_or(u.volleys_fired + u.ammunition, |p| p.per_man),
            off_field: matches!(u.reinforcement, Reinforcement::Held | Reinforcement::Waiting),
            garrison: u.garrison.map(|g| g as u32 + 1),
        });
    }
    f
}

/// Applies one script request to the model.
pub fn apply(b: &mut Battle, r: BattleScriptRequest) {
    let set = |b: &mut Battle, units: &[u32], on: bool| {
        for &id in units {
            if let Some(i) = b.unit_index(id) {
                b.units[i].script_controlled = on;
            }
        }
    };
    match r {
        BattleScriptRequest::DeployReinforcement { unit, deploy } => {
            info!("Battle script: deploy_reinforcement({deploy}) for unit {unit} at {:.0} s", b.time_seconds());
            b.deploy_reinforcement(unit, deploy);
        }
        BattleScriptRequest::TakeControl { units } => set(b, &units, true),
        BattleScriptRequest::ReleaseControl { units } => set(b, &units, false),
        BattleScriptRequest::Halt { units } => {
            for id in units {
                b.order_halt(id);
            }
        }
        BattleScriptRequest::FireAtWill { units, on } => {
            for id in units {
                b.order_fire_at_will(id, on);
            }
        }
        BattleScriptRequest::AttackUnit { units, target, run } => {
            for id in units {
                // PROVISIONAL stand-off: close to contact.
                b.order_attack_unit(id, target, 2.0, run);
            }
        }
        BattleScriptRequest::Move { units, x, z, run, .. } => {
            let speed = if run { MoveSpeed::Run } else { MoveSpeed::Walk };
            for id in units {
                b.order_move_at(id, (x, z), speed);
            }
        }
        BattleScriptRequest::GuardMode { units, on } => {
            // PROVISIONAL: guard mode = hold the position (no pursuit) in our model.
            for id in units {
                b.order_hold_position(id, on);
            }
        }
            BattleScriptRequest::Skirmish { units, on } => {
            for id in units {
                b.order_skirmish(id, on);
            }
        }
        BattleScriptRequest::SelectDeployable { units, name } => {
            let Some(v) = ability_value(&name) else {
                warn!("Battle script: select_deployable_object: unknown ability {name}");
                return;
            };
            for id in units {
                if let Err(e) = b.order_select_deployable(id, v) {
                    debug!("Battle script: select_deployable_object({name}) for unit {id}: {e}");
                }
            }
        }
        BattleScriptRequest::SpecialAbility { units, name } => {
            let Some(v) = ability_value(&name) else {
                warn!("Battle script: perform_special_ability: unknown ability {name}");
                return;
            };
            for id in units {
                if let Err(e) = b.order_special_ability(id, v) {
                    debug!("Battle script: perform_special_ability({name}) for unit {id}: {e}");
                }
            }
        }
        BattleScriptRequest::ShotType { units, name } => {
            for id in units {
                if let Err(e) = b.order_change_shot_type(id, &name) {
                    debug!("Battle script: change_shot_type({name}) for unit {id}: {e}");
                }
            }
        }
        BattleScriptRequest::ScriptMorale { units, mode } => {
            let mode = match mode {
                0 => ScriptMorale::Fearless,
                2 => ScriptMorale::Rout,
                _ => ScriptMorale::Default,
            };
            for id in units {
                b.order_script_morale(id, mode);
            }
        }
        BattleScriptRequest::DefendBuilding { units, building, run } => {
            for id in units {
                match b.order_defend_building(id, building.wrapping_sub(1), run) {
                    Ok(()) => info!("Battle script: unit {id} ordered into building {building}"),
                    Err(e) => warn!("Battle script: defend_building({building}) for unit {id}: {e}"),
                }
            }
        }
        BattleScriptRequest::SetInvincible { units, on } => {
            for id in units {
                b.order_invincible(id, on);
            }
        }
        BattleScriptRequest::Camera { .. } | BattleScriptRequest::UiVisible { .. } | BattleScriptRequest::Marker { .. } => {}
        BattleScriptRequest::Other { order, .. } => debug!("Battle script order not modelled: {order}"),
    }
}

fn flush_log(host: &BattleScriptHost) {
    for l in host.take_log() {
        if l.starts_with("ERROR") {
            warn!("Battle script: {l}");
        } else {
            debug!("Battle script: {l}");
        }
    }
}

/// Leaving the battle: drop the script.
pub fn leave(world: &mut World) {
    world.remove_non_send::<BattleScripts>();
}

/// The ability enum value of a script's ability name (`0x0057C950`, CONFIRMED names).
fn ability_value(name: &str) -> Option<u8> {
    ABILITY_NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| *v)
}
