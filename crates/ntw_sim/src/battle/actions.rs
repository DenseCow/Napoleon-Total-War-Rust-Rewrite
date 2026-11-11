//! The men's action slot: the one rule that ends every living man's action
//! ([`LandUnit::end_men_actions`]) and the exe's places that run it (BACKLOG §0 "Soldier action
//! states left"). The clear and the places it runs: CONFIRMED static except where marked. The
//! full trace of each place is in `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md`, "The soldier
//! action flags" and its list of unit-level clears (actionclears); this note keeps one entry per
//! place: when the exe runs the clear there, and what ours does.
//!
//! A man's action is the one built in place in his action slot (soldier `+0x2CC`; the action at
//! `+0x2D4`, its done flag at `+0x2E4`) with two flags: `+0x35B`, present (set by the base action
//! constructor `0x007E3A50`), and `+0x35C`, started (set by the slot update `0x0066E780` once the
//! action's entry vfunc `+4` has run, before its first update in the same call). Ending it
//! destroys a present action (its vfunc 0 with 1) and clears both flags, as every action install
//! does (`0x006533E0`, `0x00653560`). With none present the man is ready (`0x0063E290`) and his
//! slot update does nothing, so he stands. Ours needs no started flag: it starts a move in the
//! tick it is issued, as the exe's slot update enters and first updates it in one call.
//!
//! Where the exe ends the men's actions (every man of the unit or squad, but for the two entries
//! that end one man's):
//! - the idle state's update `0x005577C0`, on every idle tick: ours, `LandUnit::idle_update`, the
//!   rule's one caller. It runs on a standing unit's idle ticks only: not on the tick the reform
//!   state is entered, nor while that state lasts (`LandUnit::reslot_tick`);
//! - the squad update `0x00580A30`, on a gated tick (squad `+0x515` set, `+0x508` not 1 or
//!   `+0x498` set, no task on its stack `+0x460`) when the placement check `0x00586440` with 0
//!   fails: the unit's placement holder (unit `+0x384`) has no anchor, or one laid for fewer than
//!   the unit's entities (unit `+0x2CC`: its men, mount groups, guns, gun trains and their teams).
//!   When the check holds, a new idle task is pushed instead (`0x00522D70` / `0x0051A3A0`) and no
//!   action ends. Never in ours: a `LandUnit`'s entities are its men (no gun, mount or gun-train
//!   entities, no gun loss: BACKLOG §3 limbering), it has a placement from its first tick
//!   (`LandUnit::reslot_tick`), and its men only fall. In the exe no later insert into the
//!   entity list was found (notes), and the one unit found whose anchor stops covering its
//!   entities is an artillery unit that lost its last gun while a gun train or team is left
//!   (`0x005445A0`). PROVISIONAL (ours): which holder the exe's standing unit keeps after each
//!   order is not traced (with the empty holder `0x0051D3A0`, or the path holder `0x0051D330`
//!   before its node's anchor is laid, the check fails, so the men's actions end on each gated
//!   squad tick and no idle task is pushed), and the gunless battery needs guns and gun trains
//!   as entities;
//! - the constructor `0x0051A230` (vtable `0x013239B0`, update `0x00581200`), the base of one
//!   family of fire orders, not of all: every man, once, when an order of the family is built,
//!   whether the men stand or walk to their slots. The family is the constructor's six callers
//!   (xrefs, complete): the volley order `0x0051A200`, which the missile-attack state
//!   `0x00553C30` builds for drill 0 or a unit with no drill object; the order `0x00621C50`,
//!   built through `0x00621BF0` (by `0x00553C30`, when the key of the unit's `+0xDC4` record is
//!   19 or 20) and through `0x00621C20` (by `0x00572730`); and the orders `0x0051A0E0`,
//!   `0x0051A110`, `0x0051A140` and `0x0051A1B0` (built by `0x005726A0`, `0x00572970`,
//!   `0x005727C0` and `0x005728B0`). Not read: every builder named here but `0x00553C30`, and
//!   the bodies of `0x00621BF0`, `0x00621C20` and `0x00621C50` (only their call links). The
//!   other fire orders `0x00553C30` builds end no action in their constructors, which were read:
//!   the mass-fire order of drill 1 (`0x0051A490`) calls the state base `0x00519190` itself, and
//!   the platoon orders of drills 2-4 (`0x0051B120` / `0x0051B180` / `0x0051B090`, through
//!   `0x0051B060`) and the rank-fire order of drill 5 (`0x0051B240`) reach it through
//!   `0x0051A040`; none goes through `0x0051A230`, and none is a place listed here. Their
//!   entry and update functions were not read, so whether those orders end the men's actions
//!   later, by a place not in this list, is not traced. The order built while ability 7 is
//!   active (`0x00519E80`) was not read. PROVISIONAL (ours): not built, and a difference from
//!   the exe for the volley family. Ours has no fire-order state: a standing unit fires from
//!   the idle state (`Battle::missile_step`, which does not look at `MenMarch::reforming`), and
//!   the idle update's clear does not stand for this one, since it does not run on the reform
//!   state's entry tick or while that state lasts. So a unit that starts a volley of that
//!   family while its men walk to their slots keeps them walking, where the exe ends their
//!   actions when it builds the order. To trace before building it: when `0x00553C30` runs (its
//!   one code reference is the this-adjust thunk `0x004F7A30`, whose one reference is the table
//!   slot `0x0131D50C`: which state owns that slot, once per target or in fire at will too),
//!   the builders not read, and the fire-at-target group's reform entry `0x005732B0` with its
//!   gate. What sends the stopped men back to their slots is not traced either (that entry is a
//!   guess): the order's update `0x00581200` re-sends only the men its gather counts (a present
//!   action that ended short, and after the clear none is present) or after a due re-slot
//!   (notes, "When the men are re-laid"; see the doc of `LandUnit::reslot_tick`);
//! - the state constructor `0x0051AC80` (vtable `0x013256CC`, update `0x00581BB0`): every man of
//!   its owner, once, when the state is built; it then gathers soldier slots of the object the
//!   state occupies. And the fire-line update `0x00580C20` on that object: every man, while the
//!   object's `+0x20DE` is clear and the result of `0x006D0620` differs from the state's stored
//!   byte `+0x24`, before the men are re-seated on the fire-line slots. Not built: both need a
//!   man on a slot, and ours garrisons whole units ([`super::garrison`]). UNKNOWN: what the
//!   owner's `+0x510` value that selects the slot lists stands for, and whether the object is a
//!   building (the notes take it for one);
//! - adding a man to a squad `0x0052F770`, when its third argument is set: that one man's. Not
//!   built (no squads);
//! - the per-man idle animation pick `0x00574090`: a pick of none ends that man's action. Not
//!   built (no man animations in the model);
//! - the destructor `0x00522080` of vtable `0x01325748`: every man. The state is not identified
//!   (only that destructor references the vtable);
//! - `0x00563420` (in the transition table of the naval boarding states) and `0x00553770` (in the
//!   table next to `PATH_INVALIDATED_STATE` and `FINISH_STATE`), each on its own condition: not
//!   built;
//! - the battle teardown `0x00520110`: nothing to model.
//!
//! A present action that is done differs from none only for the volley-family fire order's
//! gather (`0x00539910`, slot 13 of the same vtable `0x013239B0`, through `0x007FF390`, which
//! returns the action when one is present, done or not): it counts the front-rank men whose
//! present move ended short. Ours has no move that ends short (see [`LandUnit::formation`]), so
//! the gather counts none either way and ours keeps no separate "present" flag.

use super::model::LandUnit;

impl LandUnit {
    /// Ends every living man's action (see the module note): his walk to his slot stops
    /// (`ManMarch::walk`) and his move counts as ended (`ManMarch::finished`, which stands for
    /// the ready test `0x0063E290`); his kept speed (`+0x440`) lies outside the action and stays.
    /// The dead men's entries are not touched.
    pub(super) fn end_men_actions(&mut self) {
        let alive = (self.men as usize).min(self.march.men.len());
        for m in &mut self.march.men[..alive] {
            (m.walk, m.finished) = (None, true);
        }
    }
}
