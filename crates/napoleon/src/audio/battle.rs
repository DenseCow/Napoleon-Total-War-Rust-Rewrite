//! Battle → sound bridge. Reads the battle's display-only volley log (`battle::VolleyFx`)
//! and sends [`ProjectileFired`] for each new volley, so the battle code needs no audio calls.
//!
//! Gun and shot type come from the shooter's projectile row: `projectiles.weapon_family`
//! (e.g. `musket_flintlock`, `cannon`) and `projectiles.shot_type` (e.g. `bullet`,
//! `round_shot`), the same names the projectile-fire bank uses; the sound kind from its category
//! and missile type ([`ProjectileKind::of`]). Artillery has no direct projectile; its first
//! `gun_type_to_projectiles` row is used (PROVISIONAL: the original uses the ammunition actually
//! loaded).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use ntw_data::Projectile;

use super::{ProjectileFired, ProjectileKind, ProjectileSound};
use crate::battle::{BattleSim, VolleyFx};
use crate::data::GameData;

/// Each projectile row's [`ProjectileSound`], built on its first volley, so later volleys clone
/// an `Arc` and allocate nothing. Kept for the system's lifetime (the game data is inserted once);
/// `noted`: the category and missile type pairs of no kind already logged.
#[derive(Default)]
pub(super) struct ProjectileSounds {
    by_key: HashMap<String, Arc<ProjectileSound>>,
    noted: HashSet<(String, String)>,
}

impl ProjectileSounds {
    /// The sound of projectile row `p`, built and kept on first use; a row of no kind (a documented
    /// PROVISIONAL gap, see [`ProjectileKind::of`]) is noted at info level once per category and
    /// missile type.
    fn of(&mut self, p: &Projectile) -> Arc<ProjectileSound> {
        if let Some(s) = self.by_key.get(p.key.as_str()) {
            return s.clone();
        }
        let s = Arc::new(projectile_sound(p));
        if s.kind.is_none() && self.noted.insert((p.category.clone(), p.missile_type.clone())) {
            info!(
                "sound: projectile {} (category {}, missile type {}) has no fire sound kind; artillery distance bands used",
                p.key, p.category, p.missile_type
            );
        }
        self.by_key.insert(p.key.clone(), s.clone());
        s
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
    // (tick, count at that tick) of the newest volley already handled.
    let newest = fx.recent.last().map(|(_, v)| v.tick);
    for (_, v) in &fx.recent {
        let at_tick = fx.recent.iter().filter(|(_, w)| w.tick == v.tick).position(|(_, w)| std::ptr::eq(w, v)).unwrap_or(0);
        if let Some((t, n)) = *last
            && (v.tick < t || (v.tick == t && at_tick < n))
        {
            continue;
        }
        let Some((i, unit)) = sim.battle.units.iter().enumerate().find(|(_, u)| u.id == v.shooter) else { continue };
        let Some(info) = sim.info.get(i) else { continue };
        let Some(view) = data.db.land_unit(&info.key) else { continue };
        let projectile = data.db.primary_projectile(view.stats);
        let Some(p) = projectile else { continue };
        let position = crate::terrain::ground_point(Vec2::new(unit.position.0, unit.position.1)) + Vec3::Y * 1.5;
        out.write(ProjectileFired { sound: sounds.of(p), position, shots: v.shots });
    }
    if let Some(t) = newest {
        let n = fx.recent.iter().filter(|(_, w)| w.tick == t).count();
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
    /// noted once per category and missile type.
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
        let mut sounds = ProjectileSounds::default();
        let rifle = row("baker_rifle", "Missile", "Bullet", "rifle");
        let first = sounds.of(&rifle);
        assert_eq!(*first, ProjectileSound { gun_type: "rifle".into(), shot_type: "bullet".into(), kind: Some(ProjectileKind::SmallArms) });
        assert!(Arc::ptr_eq(&first, &sounds.of(&rifle)), "a later volley shares the first one's sound");
        for key in ["grenade_a", "grenade_b"] {
            assert_eq!(sounds.of(&row(key, "missile", "grenade", "grenade")).kind, None);
        }
        assert_eq!(sounds.noted.len(), 1, "one note per category and missile type");
    }
}
