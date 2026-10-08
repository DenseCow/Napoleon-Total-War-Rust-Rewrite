//! Battle → sound bridge. Reads the battle's display-only volley log (`battle::VolleyFx`)
//! and sends [`ProjectileFired`] for each new volley, so the battle code needs no audio calls.
//!
//! Gun and shot type come from the shooter's projectile row: `projectiles.weapon_family`
//! (e.g. `musket_flintlock`, `cannon`) and `projectiles.shot_type` (e.g. `bullet`,
//! `round_shot`), the same names the projectile-fire bank uses. Artillery has no direct
//! projectile; its first `gun_type_to_projectiles` row is used (PROVISIONAL: the original
//! uses the ammunition actually loaded).

use bevy::prelude::*;

use super::ProjectileFired;
use crate::battle::{BattleSim, VolleyFx};
use crate::data::GameData;

/// Sends [`ProjectileFired`] for volleys not seen yet (tracked by tick).
pub(super) fn bridge_volleys(
    sim: Option<Res<BattleSim>>,
    fx: Option<Res<VolleyFx>>,
    data: Option<Res<GameData>>,
    mut last: Local<Option<(u32, usize)>>,
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
        let gun_type = p.weapon_family.clone().unwrap_or_else(|| "none".into());
        let position = crate::terrain::ground_point(Vec2::new(unit.position.0, unit.position.1)) + Vec3::Y * 1.5;
        out.write(ProjectileFired { gun_type, shot_type: p.shot_type.clone(), position, shots: v.shots });
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
