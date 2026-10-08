//! Cavalry and clip-selection checks against a real install (read-only). Run with:
//! `cargo test -p ntw_data --test cavalry_install -- --ignored --nocapture`.
//! Override the location with the `NTW_DATA_DIR` environment variable.
//!
//! These walk every unit of `unit_stats_land` through the DB route in
//! `ntw_formats::unit_animation` and `ntw_formats::mount` (`analysis/units/CAVALRY.md`).

use std::collections::HashMap;
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_data::schemas::UnitStatsLand;
use ntw_formats::anim::Anim;
use ntw_formats::battle_animation::AnimationTables;
use ntw_formats::mount::{MountIndex, MountModel};
use ntw_formats::pack::Vfs;
use ntw_formats::unit_animation::{Gait, UnitAnimationKeys, choose_clips, gait_speeds, plan_figure};
use ntw_formats::unit_model::{
    BattleTables, EquipmentLibrary, EquipmentThemes, UnitModelIndex, VariantRole, unit_variant_path,
};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

fn keys(s: &UnitStatsLand) -> UnitAnimationKeys {
    UnitAnimationKeys {
        man_animation_type: s.man_animation_type.clone(),
        man_entity: s.man_entity.clone(),
        weapon_theme: Some(s.weapon_anim_group.clone()),
        officer: Some(s.officer.clone()),
        musician: s.musician.clone(),
        standard_bearer: s.standard_bearer.clone(),
        num_mounts: s.num_mounts,
        mount: s.mount.clone(),
        mount_entity: s.mount_entity.clone(),
        mount_type: s.mount_type.clone(),
    }
}

struct Ctx {
    vfs: Vfs,
    db: GameDatabase,
    tables: AnimationTables,
    battle: BattleTables,
    speeds: HashMap<String, Option<(usize, f32)>>,
}

impl Ctx {
    fn open() -> Self {
        let vfs = Vfs::open_install(data_dir()).unwrap();
        let db = GameDatabase::from_vfs(&vfs).unwrap();
        let tables = AnimationTables::from_vfs(&vfs).unwrap();
        let battle = BattleTables::from_vfs(&vfs).unwrap();
        Self { vfs, db, tables, battle, speeds: HashMap::new() }
    }

    /// (frame count, root speed) of a clip, cached; `None` if it is missing or bad.
    fn clip(vfs: &Vfs, speeds: &mut HashMap<String, Option<(usize, f32)>>, path: &str) -> Option<(usize, f32)> {
        *speeds.entry(path.to_ascii_lowercase()).or_insert_with(|| {
            let a = Anim::read(&vfs.read(path).ok()?).ok()?;
            Some((a.frames.len(), a.root_speed()))
        })
    }
}

/// Every unit's figures resolve their clips for stand, walk and run from the DB tables;
/// mounted figures get a paired mount clip with the same frame count and root speed.
#[test]
#[ignore]
fn every_unit_figure_resolves_clips() {
    let mut ctx = Ctx::open();
    let stats: Vec<UnitStatsLand> = ctx.db.unit_stats_land.iter().cloned().collect();
    let (mut figures, mut mounted, mut problems) = (0, 0, Vec::new());
    for s in &stats {
        let k = keys(s);
        for role in [VariantRole::Soldier, VariantRole::Officer, VariantRole::Musician, VariantRole::StandardBearer] {
            if role != VariantRole::Soldier && k.personality(role).is_none() {
                continue;
            }
            let Some(plan) = plan_figure(&k, role, &ctx.battle, &ctx.tables) else {
                problems.push(format!("{} {role:?}: no plan", s.key));
                continue;
            };
            figures += 1;
            if plan.mount.is_some() {
                mounted += 1;
            }
            let speeds = gait_speeds(&plan, &ctx.battle);
            for gait in Gait::ALL {
                let (vfs, cache) = (&ctx.vfs, &mut ctx.speeds);
                let mut speed_of = |p: &str| Ctx::clip(vfs, cache, p).map(|c| c.1);
                let Some(clips) = choose_clips(&ctx.tables, &plan, gait, speeds, 0, &mut speed_of) else {
                    problems.push(format!("{} {role:?} {gait:?}: no clip ({})", s.key, plan.animation_table));
                    continue;
                };
                let man = Ctx::clip(&ctx.vfs, &mut ctx.speeds, &clips.man.path);
                if man.is_none() {
                    problems.push(format!("{} {role:?} {gait:?}: {} unreadable", s.key, clips.man.path));
                }
                if let Some(m) = &clips.mount {
                    let horse = Ctx::clip(&ctx.vfs, &mut ctx.speeds, &m.path);
                    let same = matches!((man, horse), (Some(a), Some(b)) if a.0 == b.0 && (a.1 - b.1).abs() < 1e-3);
                    if !same {
                        problems.push(format!("{} {role:?} {gait:?}: {} / {} not paired", s.key, clips.man.path, m.path));
                    }
                }
            }
        }
    }
    println!("{figures} figures ({mounted} mounted), {} problems", problems.len());
    for p in problems.iter().take(40) {
        println!("  {p}");
    }
    assert!(mounted > 0);
    assert!(problems.is_empty());
}

/// Every unit with mounts resolves to a horse (variant -> model -> parsed mesh with a
/// diffuse texture) and a rider (uniform -> soldier variant file), and its mount_type is
/// the rider table's mount_table.
#[test]
#[ignore]
fn every_mounted_unit_resolves_to_horse_and_rider() {
    let ctx = Ctx::open();
    let index = MountIndex::from_vfs(&ctx.vfs).unwrap();
    let models = UnitModelIndex::from_vfs(&ctx.vfs).unwrap();
    let (mut units, mut problems, mut no_uniform) = (0, Vec::new(), Vec::new());
    for s in ctx.db.unit_stats_land.iter() {
        let k = keys(s);
        if !k.is_mounted() {
            continue;
        }
        units += 1;
        let mount = k.mount.clone().unwrap();
        // Every colour variant, not just one pick.
        for (model_key, _) in index.variants(&mount) {
            match index.model(model_key).and_then(|m| m.best_lod()) {
                Some(p) if ctx.vfs.contains(p) => {}
                _ => problems.push(format!("{}: model {model_key} has no mesh", s.key)),
            }
        }
        match MountModel::assemble(&ctx.vfs, &index, &mount, 0) {
            Ok(m) => {
                if m.pieces.is_empty() || m.textures.diffuse.is_none() {
                    problems.push(format!("{}: {} has no pieces or diffuse", s.key, m.mesh_path));
                }
            }
            Err(e) => problems.push(format!("{}: {e}", s.key)),
        }
        let uniforms = models.uniforms_for_unit(&s.key, None);
        match uniforms.first() {
            // The shipped data has a few units with no `uniforms` row at all
            // (Cav_Light_Brunswick_Hussars); those cannot be drawn by any route.
            None => no_uniform.push(s.key.clone()),
            Some(u) if ctx.vfs.contains(&unit_variant_path(&u.variant, VariantRole::Soldier)) => {}
            Some(u) => problems.push(format!("{}: no rider variant {}", s.key, u.variant)),
        }
        let own = ctx.tables.table(&k.man_animation_type).and_then(|t| t.mount_table.clone());
        if own.as_deref().map(str::to_ascii_lowercase) != k.mount_type.as_deref().map(str::to_ascii_lowercase) {
            problems.push(format!("{}: mount_type {:?} vs table mount_table {own:?}", s.key, k.mount_type));
        }
    }
    println!("{units} mounted units, {} problems, without uniforms: {no_uniform:?}", problems.len());
    for p in problems.iter().take(40) {
        println!("  {p}");
    }
    assert!(units > 100);
    assert!(problems.is_empty());
}

/// Command figures: every personality used by a unit names an animation table and an
/// equipment theme; drummers, buglers and standard bearers show their instrument set
/// (`personal`), and every item of those sets has equipment pieces.
#[test]
#[ignore]
fn command_figures_resolve_equipment() {
    let ctx = Ctx::open();
    let themes = EquipmentThemes::from_vfs(&ctx.vfs).unwrap();
    let equipment = EquipmentLibrary::from_vfs(&ctx.vfs).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let mut problems = Vec::new();
    for s in ctx.db.unit_stats_land.iter() {
        let k = keys(s);
        for role in [VariantRole::Officer, VariantRole::Musician, VariantRole::StandardBearer] {
            let Some(key) = k.personality(role) else { continue };
            if !seen.insert(key.to_ascii_lowercase()) {
                continue;
            }
            let Some(plan) = plan_figure(&k, role, &ctx.battle, &ctx.tables) else {
                problems.push(format!("{key}: no plan"));
                continue;
            };
            let Some(theme) = plan.equipment_theme.as_deref().and_then(|t| themes.theme(t)) else {
                problems.push(format!("{key}: theme {:?} missing", plan.equipment_theme));
                continue;
            };
            let display: Vec<String> = ctx
                .tables
                .resolve_first(&plan.animation_table, &["STAND_TRAINED", "STAND"])
                .first()
                .map(|c| c.equipment_display.clone())
                .unwrap_or_default();
            let sets = EquipmentThemes::displayed_sets(theme, &display);
            println!("{key:<28} {:<30} {display:?} -> {sets:?}", plan.animation_table);
            if role != VariantRole::Officer && theme.instrument.is_some() && !sets.iter().any(|(c, _)| *c == "equipment_personal") {
                problems.push(format!("{key}: instrument not displayed"));
            }
            for (_, set) in sets {
                for item in themes.items(set) {
                    if equipment.pieces_for_item(item).is_empty() {
                        problems.push(format!("{key}: item {item} has no pieces"));
                    }
                }
            }
        }
    }
    for p in &problems {
        println!("  {p}");
    }
    assert!(seen.len() > 5);
    assert!(problems.is_empty());
}
