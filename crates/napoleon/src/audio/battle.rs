//! Battle → sound bridge. Reads the battle's display-only volley log (`battle::VolleyFx`)
//! and sends [`ProjectileFired`] for each new volley, so the battle code needs no audio calls.
//!
//! Gun and shot type come from the shooter's projectile row: `projectiles.weapon_family`
//! (e.g. `musket_flintlock`, `cannon`) and `projectiles.shot_type` (e.g. `bullet`,
//! `round_shot`), the same names the projectile-fire bank uses; the sound kind from its category
//! and missile type ([`ProjectileKind::of`]). Artillery has no direct projectile; the first of
//! its gun type's shots in the exe's order (by shot type, `GameDatabase::primary_projectile_row`)
//! is used (PROVISIONAL: the original uses the ammunition actually loaded).

use std::collections::HashSet;
use std::sync::Arc;

use bevy::prelude::*;
use ntw_data::{Projectile, Table};

use super::{ProjectileFired, ProjectileKind, ProjectileSound};
use crate::battle::{BattleSim, VolleyFx};
use crate::data::GameData;

/// Each projectile row's [`ProjectileSound`], built on its first volley, so later volleys clone
/// an `Arc` and allocate nothing. Indexed by the row's number in `projectiles` (no key hashed per
/// volley), for the table whose [`Table::id`] is `table`: another table (replaced game data)
/// empties it. `noted`: the category and missile type pairs of no kind already logged.
#[derive(Default)]
pub(super) struct ProjectileSounds {
    table: u64,
    by_row: Vec<Option<Arc<ProjectileSound>>>,
    noted: HashSet<(String, String)>,
}

impl ProjectileSounds {
    /// The sound of row `row` of `projectiles`, built and kept on first use (`None` past the
    /// table's end); a row of no kind (a documented PROVISIONAL gap, see [`ProjectileKind::of`])
    /// is noted at info level once per category and missile type.
    fn of(&mut self, projectiles: &Table<Projectile>, row: usize) -> Option<Arc<ProjectileSound>> {
        if self.table != projectiles.id() {
            self.table = projectiles.id();
            self.by_row.clear();
            self.by_row.resize(projectiles.len(), None);
        }
        let p = projectiles.rows().get(row)?;
        let slot = &mut self.by_row[row];
        if let Some(s) = slot {
            return Some(s.clone());
        }
        let s = Arc::new(projectile_sound(p));
        if s.kind.is_none() && self.noted.insert((p.category.clone(), p.missile_type.clone())) {
            info!(
                "sound: projectile {} (category {}, missile type {}) has no fire sound kind; artillery distance bands used",
                p.key, p.category, p.missile_type
            );
        }
        *slot = Some(s.clone());
        Some(s)
    }
}

/// What projectile row `p`'s fire sound depends on.
fn projectile_sound(p: &Projectile) -> ProjectileSound {
    ProjectileSound {
        gun_type: p.weapon_family.clone().unwrap_or_else(|| "none".into()),
        shot_type: p.shot_type.clone(),
        kind: ProjectileKind::of(&p.category, &p.missile_type),
    }
}

/// Sends [`ProjectileFired`] for volleys not seen yet (tracked by tick).
pub(super) fn bridge_volleys(
    sim: Option<Res<BattleSim>>,
    fx: Option<Res<VolleyFx>>,
    data: Option<Res<GameData>>,
    mut last: Local<Option<(u32, usize)>>,
    mut sounds: Local<ProjectileSounds>,
    mut out: MessageWriter<ProjectileFired>,
) {
    let (Some(sim), Some(fx), Some(data)) = (sim, fx, data) else { return };
    // (tick, count at that tick) of the newest volley already handled. `recent` is in firing
    // order, so the volleys of one tick are adjacent: one pass numbers them.
    let newest = fx.recent.last().map(|(_, v)| v.tick);
    let mut run = (0u32, 0usize); // (tick, volleys of it seen so far in this pass)
    for (j, (_, v)) in fx.recent.iter().enumerate() {
        let at_tick = if j > 0 && run.0 == v.tick { run.1 } else { 0 };
        run = (v.tick, at_tick + 1);
        if let Some((t, n)) = *last
            && (v.tick < t || (v.tick == t && at_tick < n))
        {
            continue;
        }
        let Some((unit, info)) = sim.unit_with_info(v.shooter) else { continue };
        let Some(view) = data.db.land_unit(&info.key) else { continue };
        let Some((row, _)) = data.db.primary_projectile_row(view.stats) else { continue };
        let Some(sound) = sounds.of(&data.db.projectiles, row) else { continue };
        let position = crate::terrain::ground_point(Vec2::new(unit.position.0, unit.position.1)) + Vec3::Y * 1.5;
        out.write(ProjectileFired { sound, position, shots: v.shots });
    }
    if let Some(t) = newest {
        let n = fx.recent.iter().rev().take_while(|(_, w)| w.tick == t).count();
        *last = Some((t, n));
    } else if sim.battle.time_seconds() < 0.5 {
        // A new battle (restart): forget the old ticks.
        *last = None;
    }
}

/// Copies the battle speed into [`super::GameSpeed`] for the sound manager's game-speed rule.
pub(super) fn track_speed(sim: Option<Res<BattleSim>>, mut speed: ResMut<super::GameSpeed>) {
    let s = sim.map_or(1.0, |s| s.speed.multiplier());
    if speed.0 != s {
        speed.0 = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rifle (a `missile` row firing a `bullet`, its weapon family no musket) is small arms;
    /// each row's sound is built once and shared by its later volleys; a row of no kind is
    /// noted once per category and missile type; a replaced table is not read through the old
    /// rows' sounds (regression: the cache was keyed by the key `String`, hashed per volley).
    #[test]
    fn a_rifle_is_small_arms_and_each_row_is_resolved_once() {
        let row = |key: &str, category: &str, missile: &str, family: &str| Projectile {
            key: key.into(),
            category: category.into(),
            missile_type: missile.into(),
            shot_type: "bullet".into(),
            weapon_family: Some(family.into()),
            ..Default::default()
        };
        let table = Table::from_rows(
            1,
            vec![
                row("baker_rifle", "Missile", "Bullet", "rifle"),
                row("grenade_a", "missile", "grenade", "grenade"),
                row("grenade_b", "missile", "grenade", "grenade"),
            ],
        );
        let mut sounds = ProjectileSounds::default();
        let first = sounds.of(&table, 0).unwrap();
        assert_eq!(*first, ProjectileSound { gun_type: "rifle".into(), shot_type: "bullet".into(), kind: Some(ProjectileKind::SmallArms) });
        assert!(Arc::ptr_eq(&first, &sounds.of(&table, 0).unwrap()), "a later volley shares the first one's sound");
        for r in [1, 2] {
            assert_eq!(sounds.of(&table, r).unwrap().kind, None);
        }
        assert_eq!(sounds.noted.len(), 1, "one note per category and missile type");
        assert!(sounds.of(&table, 3).is_none(), "past the table's end");
        // Row 0 of another table is that table's row, not the old rifle's sound.
        let other = Table::from_rows(1, vec![row("musket", "missile", "bullet", "musket_flintlock")]);
        assert_eq!(sounds.of(&other, 0).unwrap().gun_type, "musket_flintlock");
    }
}
