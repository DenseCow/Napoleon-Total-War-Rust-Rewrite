//! Action clips in the battle view: firing, reloading, melee, deaths and knock-downs, played
//! from the model's unit state (display only: the view reads `BattleSim`, never writes it).
//!
//! The slots and their families are the engine's (`ntw_formats::unit_animation`, CONFIRMED
//! slot table); a man plays alternative `selection % count` of a slot (CONFIRMED), and random
//! family members are drawn as the exe draws knock-downs (CONFIRMED formula, our generator
//! state). What the model does not say per man is staged here and is PROVISIONAL:
//! - a volley makes every living man play `FIRE` after a short per-man delay (by selection
//!   number), then `RELOAD_1` or `RELOAD_2`; units that may shoot and have a target stand in
//!   `COMBAT_READY` (`AIM` while aiming);
//! - in melee each man loops a `COMBAT_IDLE_n` and now and then plays an `ATTACK_n`;
//! - the men the model removes (the last figures of the unit) die where they stand: the death
//!   family follows what the unit was doing (`unit_animation::death_family`, INFERRED; moving
//!   deaths are matched by speed, CONFIRMED rule), the body keeps the last frame and stays on the
//!   ground when the unit moves on;
//! - when a unit is caught by a charging cavalry unit, men of its front rank may be knocked
//!   down (`KNOCKDOWN_n`, then `FACE_DOWN_GET_UP`).

use std::collections::VecDeque;
use std::sync::Arc;

use ntw_formats::anim::Anim;
use ntw_formats::unit_animation::{self, DeathCause, SelectionRng, SlotFamily, alternative, family_pick, pick_level};

use crate::soldiers::{FigureKit, KitLevel};

/// What a unit is doing, as the view reads it from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Standing, walking or running (the gait clips).
    Gait(unit_animation::Gait),
    /// Ready to fire (`AIM` when aiming).
    Ready { aiming: bool },
    Melee,
}

/// One one-shot clip pair being played.
#[derive(Clone)]
pub struct Shot {
    pub man: Arc<Anim>,
    pub mount: Option<Arc<Anim>>,
    /// Start time (s, the view's clock).
    pub start: f32,
    /// Keep the last frame forever (deaths).
    pub hold: bool,
}

/// Per-man action state.
#[derive(Clone, Default)]
pub struct ManAct {
    pub dead: bool,
    pub shot: Option<Shot>,
    /// Shots to play after the current one, back to back.
    pub next: VecDeque<(Arc<Anim>, Option<Arc<Anim>>)>,
    /// When this man may next swing in melee.
    pub next_attack: f32,
}

impl ManAct {
    /// Moves on to the next queued shot when the current one ended (`length` = its play length).
    pub fn advance(&mut self, now: f32, length: impl Fn(&Arc<Anim>) -> f32) {
        loop {
            let Some(s) = &self.shot else { return };
            if s.hold {
                return;
            }
            let end = s.start + length(&s.man);
            if now < end {
                return;
            }
            self.shot = self.next.pop_front().map(|(man, mount)| Shot { man, mount, start: end, hold: false });
        }
    }

    /// Starts a chain of one-shots at `start`.
    pub fn play(&mut self, start: f32, chain: Vec<(Arc<Anim>, Option<Arc<Anim>>)>, hold: bool) {
        let mut it = chain.into_iter();
        let Some((man, mount)) = it.next() else { return };
        self.shot = Some(Shot { man, mount, start, hold });
        self.next = it.collect();
    }
}

/// The clip pair of an action slot for a man (his alternative by selection number).
pub fn pick(kit: &FigureKit, slot: &str, sel: u32) -> Option<(Arc<Anim>, Option<Arc<Anim>>)> {
    let level = kit.actions.get(slot)?;
    Some(pair(level, sel))
}

fn pair(level: &KitLevel, sel: u32) -> (Arc<Anim>, Option<Arc<Anim>>) {
    let man = level.man[alternative(sel, level.man.len())].clone();
    let mount = (!level.mount.is_empty()).then(|| level.mount[alternative(sel, level.mount.len())].clone());
    (man, mount)
}

/// The slots of a family the kit has (rider families carry the `RIDER_` prefix).
pub fn present(kit: &FigureKit, family: SlotFamily, rider: bool) -> Vec<String> {
    family.slots().map(|s| if rider { format!("RIDER_{s}") } else { s }).filter(|s| kit.actions.contains_key(s)).collect()
}

/// The death clip pair for a man dying while the unit does `cause` at `speed` m/s: moving
/// deaths matched by speed when the kit has them and the man moves (CONFIRMED rule, `0x006611E0`),
/// else a random member of the cause's family.
pub fn death(kit: &FigureKit, cause: DeathCause, speed: f32, sel: u32, rng: &mut SelectionRng) -> Option<(Arc<Anim>, Option<Arc<Anim>>)> {
    let rider = kit.mount.is_some();
    if speed > 0.5 {
        let moving = present(kit, unit_animation::DEATH_MOVING, rider);
        let levels: Vec<&KitLevel> = moving.iter().filter_map(|s| kit.actions.get(s)).collect();
        if let Some((i, _)) = pick_level(levels.iter().map(|l| l.speed), speed) {
            return Some(pair(levels[i], sel));
        }
    }
    let family = if rider {
        SlotFamily { stem: "DEATH_STAND", count: 5 }
    } else {
        unit_animation::death_family(cause, kit.trained)
    };
    let mut slots = present(kit, family, rider);
    if slots.is_empty() {
        // Fall back to any standing death the table has.
        slots = present(kit, unit_animation::DEATH_STAND, rider);
        if slots.is_empty() {
            slots = present(kit, unit_animation::DEATH_STAND_TRAINED, rider);
        }
    }
    if slots.is_empty() {
        return None;
    }
    let k = family_pick(rng, slots.len() as u32) as usize;
    pick(kit, &slots[k], sel)
}

/// A per-man delay before his part of a volley (PROVISIONAL: up to 0.35 s by selection number).
pub fn volley_delay(sel: u32) -> f32 {
    (sel % 8) as f32 * 0.05
}

/// Seconds between a man's melee swings (PROVISIONAL).
pub fn swing_gap(sel: u32) -> f32 {
    1.2 + (sel % 7) as f32 * 0.35
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::anim::Anim;

    fn anim(frames: usize) -> Arc<Anim> {
        Arc::new(Anim { frame_rate: 20.0, duration: 0.0, bones: Vec::new(), frames: vec![Vec::new(); frames], events: Vec::new() })
    }

    #[test]
    fn shots_chain_and_hold() {
        let (a, b) = (anim(21), anim(11));
        let len = |x: &Arc<Anim>| (x.frames.len() - 1) as f32 / 20.0;
        let mut m = ManAct::default();
        m.play(1.0, vec![(a.clone(), None), (b.clone(), None)], false);
        m.advance(1.5, len);
        assert!(Arc::ptr_eq(&m.shot.as_ref().unwrap().man, &a));
        m.advance(2.2, len); // a ends at 2.0, b runs 2.0 .. 2.5
        let s = m.shot.as_ref().unwrap();
        assert!(Arc::ptr_eq(&s.man, &b) && (s.start - 2.0).abs() < 1e-6);
        m.advance(3.0, len);
        assert!(m.shot.is_none());
        m.play(4.0, vec![(a.clone(), None)], true);
        m.advance(100.0, len);
        assert!(m.shot.is_some(), "a held shot never ends");
    }

    #[test]
    fn delays_are_short_and_varied() {
        let d: Vec<f32> = (0..16).map(volley_delay).collect();
        assert!(d.iter().all(|x| (0.0..0.4).contains(x)));
        assert!(d.iter().any(|x| *x > 0.0));
        assert!(swing_gap(3) > 1.0);
    }
}
