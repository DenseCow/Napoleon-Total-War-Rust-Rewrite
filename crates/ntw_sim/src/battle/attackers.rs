//! A squad's attacker list and the attack direction the morale reads from it.
//!
//! **The list** (squad `+0x74C`, unit `+0xADC → +0x74C`; CONFIRMED static, UNITS_TERRAIN_FIDELITY.md
//! §10.2). Each squad keeps two lists of `{enemy unit, enemy soldiers in contact, updates}` (entry
//! stride `0xC`): one being filled and the committed one the morale pass `0x0053C720` reads.
//! - **Add** (`0x0052EDB0`, called by the soldier-pair collision handler `0x0064CAD0` once for each
//!   soldier of the pair, and by the local group pair loop `0x0066E680`): an enemy soldier counts
//!   when he is within `r_own + r_enemy + 2 m` of one of the squad's soldiers ([`CONTACT_MARGIN_M`],
//!   the radii are the soldiers' `+0x58`, read at `0x0052EE29` / `0x0052EE38`: our
//!   [`LandUnit::man_radius`], not the squad box's cover radius `+0x620`); his unit's entry gains
//!   one contact, or a new entry `{unit, 1, 0}` is appended. A bitset keyed by the enemy soldier's
//!   battle-wide index makes each soldier count at most once per update.
//! - **Commit** (`0x00551A40`, called only from `SQUAD::update` `0x00580A30` at `0x00580B85`, after
//!   the summary refresh, when the squad has men): every entry whose unit was also in the last
//!   committed list takes that entry's update counter plus one; the filled list becomes the
//!   committed one (so a unit no longer in contact drops out), and the next one starts empty (the
//!   bitset is cleared through its dirty list, `0x00542AA0`).
//!
//! **The direction** (`0x0057F310`, CONFIRMED static): [`attack_direction`].

use super::march::SquadBox;
use super::model::{Battle, LandUnit, Reinforcement, dist2};
use super::morale::AttackDirection;

/// The margin added to the two soldiers' radii in the contact test of `0x0052EDB0` (the float at
/// `0x01325D14`, 2.0; CONFIRMED static: in contact when the squared distance is at most the squared
/// sum, `COMISS` / `JC` at `0x0052EE5F`).
pub const CONTACT_MARGIN_M: f32 = 2.0;

/// Squared centre distance (m²) below which `0x0057F310` calls every attack a front attack (the
/// float at `0x01325D20`, 4.0: `JA` at `0x0057F3D9`; CONFIRMED static).
pub const SAME_CENTRE_M2: f32 = 4.0;

/// One entry of the attacker list (the `0xC`-byte record of `0x0052EDB0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackerEntry {
    /// The enemy unit's id (the record's unit pointer).
    pub unit: u32,
    /// The enemy unit's soldiers in contact with this squad in the update that filled the entry.
    pub contacts: u32,
    /// Consecutive commits the unit has been in the list before this one (0 for a new entry).
    pub updates: u32,
}

/// A squad's attacker list: the committed entries and the ones being filled (see the module note).
/// Both buffers are kept, so a commit allocates nothing once they have grown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttackerList {
    committed: Vec<AttackerEntry>,
    filling: Vec<AttackerEntry>,
    /// The squad's men where they stand, for [`Battle::fill_attackers`].
    men: MenScratch,
}

/// Work list of [`Battle::fill_attackers`]: no state, so two are always equal and a clone starts
/// empty (as `march::SlotScratch`).
#[derive(Default)]
struct MenScratch(Vec<(f32, f32)>);

impl Clone for MenScratch {
    fn clone(&self) -> Self {
        MenScratch::default()
    }
}

impl PartialEq for MenScratch {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for MenScratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MenScratch")
    }
}

/// OURS, a broad-phase bound with no exe meaning: an enemy unit is searched man by man only when
/// the two summary boxes' circles come within both men's reach plus this many metres. A box is
/// at most two ticks old (measured on alternate ticks, `LandUnit::refresh_summary`), so its men
/// have moved at most two ticks from it; this bound covers speeds up to 25 m/s, so it never
/// changes which men are in contact.
const BROAD_PHASE_SLACK_M: f32 = 10.0;

/// The enemy unit's men are on the map (they exist in the exe's soldier world): it has men and
/// is on the field or walking in as a reinforcement.
fn men_on_map(e: &LandUnit) -> bool {
    e.men > 0 && !e.left_field && (e.active || matches!(e.reinforcement, Reinforcement::Arriving { .. }))
}

impl Battle {
    /// Fills unit `idx`'s attacker list with the enemy soldiers in contact with its men and
    /// commits it (`0x0052EDB0` per soldier pair, then `0x00551A40`), in the unit's
    /// `SQUAD::update` slot. Skipped for a unit without men, as the exe's commit (`+0x18` test at
    /// `0x00580A62`).
    ///
    /// APPROXIMATION (the model has no soldier collision pass): every pair of one of our men and
    /// one enemy man is tested, so an enemy soldier counts when any of our men is within reach. The
    /// exe tests only the pairs its collision handler `0x0064CAD0` and group loop `0x0066E680` are
    /// handed, and marks the enemy soldier as seen before its friend and distance tests, so the
    /// first pair handed over decides for him; the two agree whenever that first pair is in reach.
    /// INFERRED: the friend test (soldier `+0x44` equal) is our `side`; the soldier pair handler
    /// takes the same field as the line between enemy melee (`0x00664100`) and friendly pushing
    /// (`0x00667260`), at `0x0064CB06`. INFERRED: the pass runs at the unit's own update, on the
    /// men where they stand then; where the exe's soldier collisions run in the frame is not traced.
    pub(super) fn fill_attackers(&mut self, idx: usize) {
        if self.units[idx].men == 0 {
            return;
        }
        let mut list = std::mem::take(&mut self.units[idx].attackers);
        let u = &self.units[idx];
        let mut own = std::mem::take(&mut list.men.0);
        own.clear();
        own.extend(u.men_standing().map(|m| m.pos));
        // The reach is both men's own radius `+0x58` (`0x0052EE29` / `0x0052EE38`), not the cover
        // radius `+0x620` the squad box reads (`LandUnit::cover_radius`).
        let (own_box, own_r) = (u.squad_box(), u.man_radius);
        let (mut lo, mut hi) = ((f32::INFINITY, f32::INFINITY), (f32::NEG_INFINITY, f32::NEG_INFINITY));
        for &(x, z) in own.iter() {
            (lo, hi) = ((lo.0.min(x), lo.1.min(z)), (hi.0.max(x), hi.1.max(z)));
        }
        for e in &self.units {
            if e.side == u.side || !men_on_map(e) {
                continue;
            }
            let reach = own_r + e.man_radius + CONTACT_MARGIN_M;
            let e_box = e.squad_box();
            let far = own_box.radius + e_box.radius + reach + BROAD_PHASE_SLACK_M;
            if dist2(own_box.centre, e_box.centre) > far * far {
                continue;
            }
            for m in e.men_standing() {
                let (x, z) = m.pos;
                if x < lo.0 - reach || x > hi.0 + reach || z < lo.1 - reach || z > hi.1 + reach {
                    continue;
                }
                if own.iter().any(|&p| soldiers_in_contact(p, own_r, m.pos, e.man_radius)) {
                    list.add_contact(e.id);
                }
            }
        }
        list.commit();
        list.men.0 = own;
        self.units[idx].attackers = list;
    }
}

impl AttackerList {
    /// The committed entries, in the order the units were first found in contact.
    pub fn entries(&self) -> &[AttackerEntry] {
        &self.committed
    }

    /// One enemy soldier of unit `unit` in contact (the append of `0x0052EDB0`, after its tests).
    pub fn add_contact(&mut self, unit: u32) {
        match self.filling.iter_mut().find(|e| e.unit == unit) {
            Some(e) => e.contacts += 1,
            None => self.filling.push(AttackerEntry { unit, contacts: 1, updates: 0 }),
        }
    }

    /// Commits the entries filled since the last commit (`0x00551A40`).
    pub fn commit(&mut self) {
        std::mem::swap(&mut self.committed, &mut self.filling);
        for e in &mut self.committed {
            if let Some(old) = self.filling.iter().find(|o| o.unit == e.unit) {
                e.updates = old.updates + 1;
            }
        }
        self.filling.clear();
    }
}

/// Where an attacker comes from, `0x0057F310` (CONFIRMED static). Front when the defender has
/// `square_formation` or `pike_square_formation` active (`in_square`, `0x0055C9A0` on `+0xDAC`
/// with 1 then 0), or when the two summary centres (`0x0053FBE0`, summary `+0xB0`) are under 2 m
/// apart ([`SAME_CENTRE_M2`]). Otherwise the defender's box (width `+0x650`, depth `+0x654`, side
/// direction `+0x658` = (front.y, −front.x), front `+0x660`) is cut by its two diagonals: the
/// attacker's centre offset `d` is in front when it lies between the two front corners (both cross
/// products with the front-right and front-left corner vectors at least 0), behind when it lies
/// between the two rear corners, else on a flank. So a wide line's front and rear wedges are wide
/// and its flanks narrow; the diagonals themselves count as front or rear.
pub fn attack_direction(defender: &SquadBox, in_square: bool, attacker_centre: (f32, f32)) -> AttackDirection {
    if in_square {
        return AttackDirection::Front;
    }
    let (dx, dz) = (attacker_centre.0 - defender.centre.0, attacker_centre.1 - defender.centre.1);
    if dx * dx + dz * dz < SAME_CENTRE_M2 {
        return AttackDirection::Front;
    }
    let f = defender.front();
    let side = (f.1, -f.0);
    let (hw, hd) = (0.5 * defender.width, 0.5 * defender.depth);
    // The front-right and front-left corners, relative to the centre.
    let k1 = (hd * f.0 + hw * side.0, hd * f.1 + hw * side.1);
    let k2 = (hd * f.0 - hw * side.0, hd * f.1 - hw * side.1);
    let t1 = dx * k2.1 - dz * k2.0;
    let t2 = dz * k1.0 - dx * k1.1;
    // The exe's order of tests; a NaN fails every one and lands on the flank.
    if t1 >= 0.0 && t2 >= 0.0 {
        AttackDirection::Front
    } else if t1 > 0.0 {
        AttackDirection::Flank
    } else if t2 <= 0.0 {
        AttackDirection::Rear
    } else {
        AttackDirection::Flank
    }
}

/// Whether two soldiers are in contact for the attacker list (`0x0052EDB0`): centres at most
/// `r_a + r_b + 2 m` apart.
pub fn soldiers_in_contact(a: (f32, f32), ra: f32, b: (f32, f32), rb: f32) -> bool {
    let reach = ra + rb + CONTACT_MARGIN_M;
    let (dx, dz) = (a.0 - b.0, a.1 - b.1);
    dx * dx + dz * dz <= reach * reach
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::abilities::angle16;

    /// A box at the origin facing +y (heading 0), `width` across and `depth` along.
    fn bx(width: f32, depth: f32) -> SquadBox {
        SquadBox { centre: (0.0, 0.0), heading: 0, width, depth, radius: 0.0 }
    }

    #[test]
    fn direction_wedges_follow_the_box_diagonals() {
        use AttackDirection::*;
        // A 40 m wide, 4 m deep line facing +y: its diagonals are 0.1 rad off the line's axis,
        // so most of the front half is front and the flanks are narrow.
        let line = bx(40.0, 4.0);
        assert_eq!(attack_direction(&line, false, (0.0, 30.0)), Front);
        assert_eq!(attack_direction(&line, false, (30.0, 4.0)), Front); // 7.6° off the axis
        assert_eq!(attack_direction(&line, false, (30.0, 2.0)), Flank); // inside the right wedge
        assert_eq!(attack_direction(&line, false, (-30.0, 0.0)), Flank);
        assert_eq!(attack_direction(&line, false, (-30.0, -4.0)), Rear);
        assert_eq!(attack_direction(&line, false, (0.0, -30.0)), Rear);
        // On a diagonal: front (front corners) and rear (rear corners), never flank.
        assert_eq!(attack_direction(&line, false, (20.0, 2.0)), Front);
        assert_eq!(attack_direction(&line, false, (-20.0, 2.0)), Front);
        assert_eq!(attack_direction(&line, false, (20.0, -2.0)), Rear);
        assert_eq!(attack_direction(&line, false, (-20.0, -2.0)), Rear);
        // A square box splits at 45°.
        let block = bx(10.0, 10.0);
        assert_eq!(attack_direction(&block, false, (9.0, 10.0)), Front);
        assert_eq!(attack_direction(&block, false, (11.0, 10.0)), Flank);
        assert_eq!(attack_direction(&block, false, (11.0, -10.0)), Flank);
        assert_eq!(attack_direction(&block, false, (9.0, -10.0)), Rear);
        // A column (deeper than wide) has wide flanks.
        let column = bx(4.0, 40.0);
        assert_eq!(attack_direction(&column, false, (10.0, 10.0)), Flank);
        assert_eq!(attack_direction(&column, false, (1.0, 30.0)), Front);
    }

    #[test]
    fn direction_turns_with_the_heading() {
        use AttackDirection::*;
        // Facing +x (heading a quarter turn): an attacker at +x is in front, one at +y on a flank.
        let east = SquadBox { heading: angle16((1.0, 0.0)), ..bx(20.0, 6.0) };
        assert_eq!(attack_direction(&east, false, (30.0, 0.0)), Front);
        assert_eq!(attack_direction(&east, false, (0.0, 30.0)), Flank);
        assert_eq!(attack_direction(&east, false, (-30.0, 0.0)), Rear);
        // Off-origin centre: the offset is from the defender's centre.
        let moved = SquadBox { centre: (100.0, 50.0), ..bx(20.0, 6.0) };
        assert_eq!(attack_direction(&moved, false, (100.0, 20.0)), Rear);
    }

    #[test]
    fn square_and_close_centres_are_front() {
        use AttackDirection::*;
        let line = bx(40.0, 4.0);
        assert_eq!(attack_direction(&line, true, (0.0, -30.0)), Front);
        // Under 2 m apart: front; exactly 2 m is classified by the box.
        assert_eq!(attack_direction(&line, false, (0.0, -1.99)), Front);
        assert_eq!(attack_direction(&line, false, (0.0, -2.0)), Rear);
    }

    #[test]
    fn contact_reach_is_both_radii_plus_two_metres() {
        assert!(soldiers_in_contact((0.0, 0.0), 0.5, (3.0, 0.0), 0.5));
        assert!(!soldiers_in_contact((0.0, 0.0), 0.5, (3.01, 0.0), 0.5));
        assert!(soldiers_in_contact((0.0, 0.0), 0.0, (0.0, -2.0), 0.0));
    }

    #[test]
    fn list_adds_commits_and_drops() {
        let mut l = AttackerList::default();
        assert!(l.entries().is_empty());
        l.add_contact(7);
        l.add_contact(3);
        l.add_contact(7);
        // Nothing is read before the commit.
        assert!(l.entries().is_empty());
        l.commit();
        let e = |unit, contacts, updates| AttackerEntry { unit, contacts, updates };
        assert_eq!(l.entries(), [e(7, 2, 0), e(3, 1, 0)]);
        // Next update: 3 stays (its counter rises), 7 left contact, 9 is new.
        l.add_contact(9);
        l.add_contact(3);
        l.commit();
        assert_eq!(l.entries(), [e(9, 1, 0), e(3, 1, 1)]);
        l.add_contact(3);
        l.commit();
        assert_eq!(l.entries(), [e(3, 1, 2)]);
        // An update with no contact empties the list; a unit back later starts over.
        l.commit();
        assert!(l.entries().is_empty());
        l.add_contact(3);
        l.commit();
        assert_eq!(l.entries(), [e(3, 1, 0)]);
    }
}
