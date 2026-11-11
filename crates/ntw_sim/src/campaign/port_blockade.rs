//! Port blockades: a navy blockading a port slot ([`super::RegionSlot::blockade`], the port object's
//! +0x68 handle linked with the navy's +0xE8 handle, saved as `PORT_GARRISON_MANAGER` #1/#2).
//!
//! **The start.** A navy ordered onto a port slot of a faction it is at war with
//! ([`CampaignModel::port_order`]; the port order's execute `0x0095D6A0`) arrives through the arrival step
//! `0x0094B9B0`, which docks it or starts a blockade (the [`port.arrival`](super::seams::PORT_ARRIVAL) seam,
//! [`port_arrival`]); the blockade is `StartPortBlockade` `0x00A66F80`
//! ([`CampaignModel::start_port_blockade`]), whose only caller is that step.
//!
//! **The end.** The lift `LiftPortSlotBlockade` (`0x00A784E0`, CONFIRMED) is the one function every path
//! that ends a blockade calls; [`CampaignModel::lift_port_blockade`] is ours. Its callers in the exe
//! (CONFIRMED, xrefs and decompile):
//! * a peace (`MakePeace` `0x00B262C0` → `ClearCommerceRaidsBetweenFactions` `0x00A25570`, the call at
//!   `0x00A25627`): [`CampaignModel::lift_blockade_on_peace`];
//! * an owner change's slot pass (`TransferRegionSlotOwners` `0x00B1B300`, the call at `0x00B1B3EF`):
//!   [`CampaignModel::clear_blockades_after_slot_pass`];
//! * the navy's destructor (`0x0088EF90`): [`CampaignModel::lift_blockade_of_navy`];
//! * a battle's casualty step for a force left with no units (`0x008AC010`, whose only caller is
//!   `0x008AC720`; the force's virtual +0x5C answers 2, for a navy `0x008AD6E0` when its unit list is
//!   empty, `0x008F6820` on navy +0x94 at `0x008ADDF1`): an army's
//!   siege ends, a navy's blockade is lifted. Ours: the emptied force is removed
//!   ([`CampaignModel::remove_force`]), which runs both;
//! * the start of the blockading admiral's next order (the order helper `0x008FF470`):
//!   [`CampaignModel::begin_order`];
//! * the end-blockade command (`CCQ_CHARACTER_END_BLOCKADE`, the handler at `0x00932710`):
//!   [`CampaignModel::end_blockade`];
//! * the post-battle sequence `0x008F7880` after the docked navy's attack on its blockader (the call at
//!   `0x008F8981`): [`CampaignModel::lift_after_sally`].
//!
//! **The docked navy's attack.** A navy docked in a blockaded port may attack its blockader once per
//! blockade ([`CampaignModel::sally`], the attack execute `0x0095B370`).
//!
//! **While it lasts.** A navy docked in the blockaded port is no obstacle on the path grid
//! ([`CampaignModel::navies_docked_in_blockaded_ports`]); the port's naval recruitment queue holds its
//! finished ships, as a besieged settlement's land queue holds its units (the hold in
//! [`CampaignModel::region_turn`]); the trade routes are cut only by sieges and hostile navies on their sea
//! legs, not by the port blockade itself ([`super::trade`]).
//!
//! The start's messages and events, the script event `CharacterBlockadedPort` among them, are listed at
//! [`CampaignModel::start_port_blockade`], with what settles them.

use super::commands::{CommandError, Walk, CONTACT_DISTANCE};
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::pathing::GridPath;
use super::world::PortBlockade;
use super::CampaignModel;
use crate::fixed::Fixed20;

/// What a navy's order onto a hostile port does when the navy arrives ([`CampaignModel::port_order`]; the
/// answer of the [`port.arrival`](super::seams::PORT_ARRIVAL) seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortArrival {
    /// The navy sails into the port and docks there.
    Dock,
    /// The navy stays outside the port and blockades it.
    Blockade,
}

/// How far from the port a navy stops when it arrives to blockade it (map units). PROVISIONAL: the arrival
/// step `0x0094B9B0` leaves a blockading navy where its path ended (it places only a docking navy, on the
/// slot's position), and where the port order's path ends is not traced; the model keeps the navy out of the
/// port, which it tells by position ([`CampaignModel::docked_port`]).
const BLOCKADE_STAND_DISTANCE: f32 = CONTACT_DISTANCE;

/// The original rule of the [`port.arrival`](super::seams::PORT_ARRIVAL) seam (the arrival step
/// `0x0094B9B0`, CONFIRMED by decompile): the navy docks (the dock step `0x00A924D0`) when the port has no
/// docked navy (port +0x60 does not resolve) and the slot's garrison test (slot virtual +0x1C) answers 0;
/// otherwise it blockades the port (`StartPortBlockade` `0x00A66F80`).
///
/// PROVISIONAL: the slot's garrison (slot +0xC0) is not held by the model, so only a docked navy makes the
/// arrival a blockade (the same gap as [`CampaignModel::restore_port_holder`]).
pub fn port_arrival(m: &CampaignModel, navy: ForceId, region: RegionId, slot: usize) -> PortArrival {
    if m.docked_navies(region, slot).any(|f| f != navy) { PortArrival::Blockade } else { PortArrival::Dock }
}

/// Cuts `path` short so that it ends [`BLOCKADE_STAND_DISTANCE`] from `port`: the points from the first one
/// inside that distance on are dropped and the path ends where its last leg crosses the distance, at that
/// leg's share of the cost. A path that starts inside the distance keeps its first point only.
fn stop_short(path: &mut GridPath, port: (f32, f32)) {
    let dist = |p: (f32, f32)| ((p.0 - port.0).powi(2) + (p.1 - port.1).powi(2)).sqrt();
    let keep = path.points.iter().rposition(|&p| dist(p) > BLOCKADE_STAND_DISTANCE);
    let Some(keep) = keep else {
        path.points.truncate(1);
        path.costs.truncate(1);
        path.polys.truncate(1);
        return;
    };
    if keep + 1 == path.points.len() {
        return;
    }
    let (a, b) = (path.points[keep], path.points[keep + 1]);
    let at = |t: f32| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    // The distance to a point is convex along a leg, so the leg from outside to inside crosses it once.
    let (mut outside, mut inside) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = (outside + inside) / 2.0;
        if dist(at(mid)) > BLOCKADE_STAND_DISTANCE {
            outside = mid;
        } else {
            inside = mid;
        }
    }
    let cost = path.costs[keep] + (path.costs[keep + 1] - path.costs[keep]) * outside;
    path.points.truncate(keep + 1);
    path.points.push(at(outside));
    path.costs.truncate(keep + 1);
    path.costs.push(cost);
    // The cut point keeps the polygon of the point it replaces (the leg's end).
    path.polys.truncate(keep + 2);
}

impl CampaignModel {
    /// The navies docked in port slot `slot` of `region` ([`Self::docked_port`]), in force id order. The exe's
    /// port holds one docked navy (port +0x60); the model tells a docked navy by its position, so it can hold
    /// several.
    pub(crate) fn docked_navies(&self, region: RegionId, slot: usize) -> impl Iterator<Item = ForceId> + '_ {
        self.world.forces.values().filter(move |f| f.is_navy && self.docked_port(f.id) == Some((region, slot))).map(|f| f.id)
    }

    /// The navies lying docked in a blockaded port, which are no path-grid obstacle (CONFIRMED by decompile):
    /// the blockade start removes the docked navy's character from the obstacle layers (`0x00B653A0`, the call
    /// at `0x00A67105`; it also runs from the character's destructor and on entering a settlement), and the
    /// re-add `0x00B084F0` adds a character in a residence only when `0x00B1D100` lets it: a docked navy only
    /// while its port (`0x00A17D20`) has no blockader (port +0x68 does not resolve). The lift calls
    /// `0x00B084F0` on the docked navy's character after unlinking the blockader, which adds him back. The
    /// model reads obstacles on the fly ([`super::zones`]), so this test stands for that state. Empty, without
    /// allocating, when no port is blockaded.
    pub(crate) fn navies_docked_in_blockaded_ports(&self) -> Vec<ForceId> {
        let ports = self.blockaded_slots(None, |_, _, _| true);
        if ports.is_empty() {
            return Vec::new();
        }
        let docked = |f: &&super::MilitaryForce| f.is_navy && self.docked_port(f.id).is_some_and(|p| ports.contains(&p));
        self.world.forces.values().filter(docked).map(|f| f.id).collect()
    }

    /// The faction a port slot stands for in a navy's order against it: the port order is allowed only to a
    /// navy at war with it (the order's creation `0x00917E20` → `0x009D0980` with flag 0 → `0x00A0B730`, the
    /// "at war" test `0x008CE9B0` against the slot's faction, slot virtual +0x10; CONFIRMED by decompile). A
    /// docked navy's faction holds the slot (the dock step `0x00A924D0` sets it with `SetSettlementOwner`,
    /// CONFIRMED), which the model does not store when a navy docks ([`super::RegionSlot::holder`]), so: the
    /// faction of the first navy docked there, else the slot's holder, else the region's owner. `None` for a
    /// slot that does not exist.
    pub fn port_faction(&self, region: RegionId, slot: usize) -> Option<FactionId> {
        let r = self.world.regions.get(&region)?;
        let s = r.slots.get(slot)?;
        let docked = self.docked_navies(region, slot).next().and_then(|f| self.world.forces.get(&f)).map(|f| f.faction);
        Some(docked.or(s.holder).unwrap_or(r.owner))
    }

    /// The port slot a move of the navy `navy` to `to` is a port order against ([`Self::port_order`]): the
    /// port at `to` ([`Self::port_at`]) when its faction ([`Self::port_faction`]) is at war with the navy's
    /// ([`Self::at_war`]). `None` for an army, a port of the navy's own faction or one it is not at war with:
    /// those moves are plain walks.
    pub(crate) fn hostile_port_at(&self, navy: ForceId, to: (Fixed20, Fixed20)) -> Option<(RegionId, usize)> {
        let faction = self.world.forces.get(&navy).filter(|f| f.is_navy)?.faction;
        let (region, slot) = self.port_at(to)?;
        self.at_war(faction, self.port_faction(region, slot)?).then_some((region, slot))
    }

    /// The start of an order of `character` (the order helper `0x008FF470` with its flag set, CONFIRMED by
    /// decompile): the army he commands ends its siege (`0x00A0B8C0`, army +0x94 →
    /// `EndForceSiegeOfSettlement`, [`Self::end_force_siege`]) and the navy he commands lifts its blockade
    /// (`0x00A0B680`, navy +0xE8 → `LiftPortSlotBlockade`, [`Self::lift_blockade_of_navy`]). The helper acts on
    /// the ordered character's own force only and runs before the order is created, so the siege or blockade
    /// stays ended when the order then fails (no path, no action points). Every character order's execute
    /// calls it with the flag set (30 call sites; the notes list them) except the settlement order when his
    /// army besieges that very settlement (`0x0095D140`, [`Self::enter_settlement`]) and the docked navy's
    /// attack on its blockader, which runs no helper at all ([`Self::sally`]).
    ///
    /// The orders built outside the command handlers call it too (CONFIRMED by decompile): the duel loser's
    /// flight (`0x00A18460` → `0x0094EE20` at `0x00A1852B`) and the post-battle retreat (`0x00A18550` →
    /// `0x0094EE20`) issue through `0x0091B950` the order `0x009069A0` (vtable `0x0135F1DC`), whose execute
    /// `0x0095D080` calls the helper at `0x0095D096`; the reinforcement march (`0x00968B00` → `0x009195A0`,
    /// order `0x00905460`, vtable `0x0135F868`) runs it from its execute `0x0095BA40` at `0x0095BCEA`.
    ///
    /// INFERRED: every walk of the model ([`Self::walk`]) is one of those orders. Not traced: that execute
    /// `0x0095BA40` reaches the helper only when its carrier step (`0x008FD580` / `0x008FD870`) answers 2
    /// (`0x0095BC6F`), which it does for an army with no carrier (army +0xEC empty, `0x008FD604`); for a navy
    /// or an army aboard a ship the step may issue another order first, so whether a merge march of those
    /// lifts the navy's blockade at its start is open.
    pub(crate) fn begin_order(&mut self, character: CharacterId) {
        let Some(force) = self.force_of(character) else { return };
        if self.world.forces.get(&force).is_some_and(|f| f.is_navy) {
            self.lift_blockade_of_navy(force);
        } else {
            self.end_force_siege(force);
        }
    }

    /// A navy's order onto the port slot `(region, slot)` of a faction it is at war with
    /// ([`Self::hostile_port_at`]; `to` is the order's destination, at that port), led by its commander `c`:
    /// the port order's execute `0x0095D6A0` and its arrival `0x0094B9B0` (CONFIRMED by decompile).
    /// 1. A port that already has a blockader (port +0x68 resolves) refuses the order (the order's
    ///    pre-check `0x009243A0`, the refusal at `0x009245E7`), before anything changes. The command runs it
    ///    every time: the navy-to-port command `0x009324F0` goes through the gate `0x00916B30` to
    ///    `0x0091D4A0`, which runs the pre-check at `0x0091D4C9` before it builds the order (`0x009087C0`) and
    ///    then runs it with `0x009651A0(order, 0, 1, 0)` at `0x0091D796`, so the execute skips its own copy.
    ///    Only the turn-start resume of a standing order (`0x009653D0`, at `0x00965415`) has the execute run
    ///    it again, and the model keeps no standing orders (the tag in `turn.rs`).
    /// 2. The order starts ([`Self::begin_order`]).
    /// 3. The navy sails as far as its action points reach. When it arrives, the
    ///    [`port.arrival`](super::seams::PORT_ARRIVAL) seam says what happens ([`port_arrival`] in the
    ///    original): it sails into the port, or it stops outside ([`BLOCKADE_STAND_DISTANCE`]) and the
    ///    blockade starts ([`Self::start_port_blockade`]).
    ///
    /// A navy that would blockade the port it lies in (the model alone can have two navies in one port) is
    /// refused. PROVISIONAL: a navy docking in a hostile port takes the slot over in the exe (the dock step
    /// `0x00A924D0`: the slot's building is damaged, its naval recruitment and construction are cancelled and
    /// the navy's faction becomes its holder); the model docks by position and runs none of it (BACKLOG "Slot
    /// capture").
    pub(crate) fn port_order(
        &mut self,
        c: CharacterId,
        navy: ForceId,
        (region, slot): (RegionId, usize),
        to: (Fixed20, Fixed20),
    ) -> Result<Walk, CommandError> {
        let port = self.world.regions.get(&region).and_then(|r| r.slots.get(slot)).ok_or(CommandError::UnknownRegion(region))?;
        if port.blockade.is_some() {
            return Err(CommandError::Unsupported("the port is already blockaded"));
        }
        let port_pos = port.position;
        let outcome = self.rules.seams.port_arrival.rule()(self, navy, region, slot);
        if outcome == PortArrival::Blockade && self.docked_port(navy) == Some((region, slot)) {
            return Err(CommandError::Unsupported("the navy lies in the port it would blockade"));
        }
        self.begin_order(c);
        let mut plan = self.plan_path_with(c, to, false).ok_or(CommandError::NoPath)?;
        if outcome == PortArrival::Blockade {
            let at = port_pos.unwrap_or(to);
            stop_short(&mut plan.path, (at.0.to_f32(), at.1.to_f32()));
            let points = self.world.characters.get(&c).map_or(0, |ch| ch.movement_points);
            plan.reachable = plan.path.reachable(points as f32);
        }
        let last = plan.path.points.len() - 1;
        let walk = self.walk_planned(c, &plan, last)?;
        if walk.arrived && outcome == PortArrival::Blockade {
            self.start_port_blockade(region, slot, navy);
        }
        Ok(walk)
    }

    /// The navy `navy` starts blockading port slot `slot` of `region` (`StartPortBlockade` `0x00A66F80`,
    /// CONFIRMED by decompile; its only caller is the arrival step `0x0094B9B0`): the port and the navy are
    /// linked (port +0x68, navy +0xE8; `0x008CFB20` links them only when both are free), with the port's
    /// +0x74 byte clear ([`PortBlockade::docked_navy_attacked`]: only the lift clears it, so it is clear
    /// whenever a blockade starts), and the trade price update with flag 0 runs (`0x00BCB020(0)`,
    /// [`Self::update_commodity_prices_with`]). Returns whether the blockade started; a slot that is no port,
    /// one already blockaded and a navy that already blockades are left alone with a warning (the order's
    /// checks keep them out).
    ///
    /// PROVISIONAL, not modelled: the message 0xBA to a human on either side, the event message of type 307
    /// to the holder faction (`0x00A17BE0` → `0x009D39A0` → the faction's event post `0x009D3250`; it names
    /// the navy's faction, the port's region and theatre, CONFIRMED by decompile), the slot's event (+0x480),
    /// the holder faction's event (+0x2A0) and the script event `CharacterBlockadedPort` (vtable
    /// `0x01376C40`). Settled by posting them through the model's event and message queues once those carry
    /// the 0xBA message and the 307 event (BACKLOG "Port blockade"), and the slot's and holder's events once
    /// their listeners are traced (the lift posts the same two, [`Self::lift_port_blockade`]). The start's call
    /// `0x00B653A0` on a docked navy's character takes him off the path grid's obstacles, which the model
    /// reads from the blockade ([`Self::navies_docked_in_blockaded_ports`]); its set of the byte +0x58 of the
    /// slot's naval recruitment manager (slot +0x1E8) has no effect of its own (the queue's turn step writes
    /// the byte before it reads it, [`Self::region_turn`]). PROVISIONAL: the start sets the slot's byte +0x220
    /// (`0x00A6707B`; the lift does not clear it), whose readers are not traced; the model holds no such byte.
    pub(crate) fn start_port_blockade(&mut self, region: RegionId, slot: usize, navy: ForceId) -> bool {
        let free = self.blockaded_slots(None, |_, _, n| n == navy).is_empty();
        let port = self.world.regions.get_mut(&region).and_then(|r| r.slots.get_mut(slot)).filter(|s| s.port && s.blockade.is_none());
        let Some(port) = port.filter(|_| free) else {
            log::warn!("region {region:?} slot {slot}: navy {navy:?} cannot start a blockade here (no free port, or it blockades already)");
            return false;
        };
        port.blockade = Some(PortBlockade { navy, docked_navy_attacked: false });
        self.update_commodity_prices_with(false);
        true
    }

    /// The end-blockade command (`CCQ_CHARACTER_END_BLOCKADE`, the handler at `0x00932710`, CONFIRMED by
    /// decompile): the blockade of the navy `character` commands is lifted ([`Self::lift_port_blockade`]).
    /// The model's commands also check whose turn it is.
    ///
    /// ORIGINAL BUG: no null check (`0x00932710`): the handler resolves the navy's +0xE8 handle on whatever
    /// `GetCharacterForceAsNavy` returns and lifts whatever that resolves to, so a character with no navy, or
    /// a navy that blockades nothing, reads through a null pointer. Ours refuses the command with an error.
    pub(crate) fn end_blockade(&mut self, character: CharacterId) -> Result<Vec<CampaignEvent>, CommandError> {
        let faction = self.world.characters.get(&character).ok_or(CommandError::UnknownCharacter(character))?.faction;
        self.check_turn(faction)?;
        let navy = self.force_of_kind(character, true).ok_or(CommandError::Unsupported("the character commands no navy"))?;
        let slots = self.blockaded_slots(None, |_, _, n| n == navy);
        if slots.is_empty() {
            return Err(CommandError::Unsupported("the navy blockades no port"));
        }
        for (region, slot) in slots {
            self.lift_port_blockade(region, slot);
        }
        Ok(Vec::new())
    }

    /// The docked navy's attack on its blockader (the attack execute `0x0095B370`, CONFIRMED by decompile):
    /// when the attacking navy `force` is docked in a port (navy +0xE0) and that port is the one the attacked
    /// navy `target` blockades (navy +0xE8), the attack is refused if the port's +0x74 byte is set
    /// (`Some(false)`); otherwise the byte is set (`0x0095B667`) and the battle starts at once, with no move
    /// and no order helper (`Some(true)`; the caller starts the battle). Only the lift clears the byte, so a
    /// docked navy attacks once per blockade. `None`: not that attack.
    ///
    /// The battle is the exe's `naval_blockade` type 12 (`SelectCampaignBattleType` `0x008B55A0`: the
    /// attacking navy is docked). The same execute has the besieged army's attack on its besieger (the
    /// residence's byte +285, battle types 4 and 7), which waits for sieges to start in the model (BACKLOG
    /// "Sieges on the campaign map").
    pub(crate) fn sally(&mut self, force: ForceId, target: ForceId) -> Option<bool> {
        let (region, slot) = self.docked_port(force)?;
        let blockade = self.world.regions.get_mut(&region)?.slots.get_mut(slot)?.blockade.as_mut().filter(|b| b.navy == target)?;
        if blockade.docked_navy_attacked {
            return Some(false);
        }
        blockade.docked_navy_attacked = true;
        Some(true)
    }

    /// The port of a battle that is the docked navy's attack on its blockader ([`Self::sally`]): the attacker
    /// is docked in the port its first defender blockades. Read before the battle's casualties.
    pub(crate) fn sally_port(&self, attacker: ForceId, defender: ForceId) -> Option<(RegionId, usize)> {
        let (region, slot) = self.docked_port(attacker)?;
        let blockade = self.world.regions.get(&region)?.slots.get(slot)?.blockade?;
        (blockade.navy == defender).then_some((region, slot))
    }

    /// The post-battle sequence's blockade step after the docked navy's attack on its blockader (`0x008F7880`
    /// stage 7 for battle type 12, the lift at `0x008F8981`; CONFIRMED by decompile), run after the
    /// casualties. The step runs only when the beaten side has no character left to place, which for this
    /// battle means the docked attacker was destroyed: when the blockader won and still blockades the port
    /// (navy +0xE8), the blockade is lifted if no navy is docked there any more (port +0x60 does not resolve)
    /// or the docked navy is of the blockader's own faction (navy +0x7C). A beaten attacker that survives
    /// stays docked and blockaded (types 12 and 13 with the losing admiral's navy in a port place nobody,
    /// the test at `0x008F8317`). When the attacker won, the blockader's blockade ends either way: destroyed,
    /// its navy's removal lifts it ([`Self::remove_force`]); beaten and alive, it retreats (below). The exe
    /// then orders the attacker back onto its port's slot (`0x009550F0`), which in the model it never left.
    ///
    /// The beaten blockader's retreat (CONFIRMED by decompile): the sequence (built by `0x00883120`, which
    /// clears its byte +0xB2 at `0x00883298`; only the constructors `0x00882DF0` and `0x00883680` of other
    /// battles set it) reaches the retreat `0x00A18550` at `0x008F8421` for a beaten admiral whose navy lies in
    /// no port (`0x00A17CF0`, navy +0xE0). The retreat orders him
    /// away through `0x0094EE20` → `0x0091B950` (order vtable `0x0135F1DC`), whose execute `0x0095D080` runs
    /// the order helper at `0x0095D096` before anything else ([`Self::begin_order`]): his blockade is lifted.
    /// When the order cannot run (his byte +0x510 already set, or the order pre-check `0x00921160` fails),
    /// the retreat answers false and the fallback `0x008CF310` deletes his force (its +0x2A8 object's
    /// destructor), whose destructor lifts the blockade too. PROVISIONAL: the retreat's move (`0x00A18550` →
    /// `0x0094EE20`, the order's execute `0x0095D080`) and the fallback's deletion (`0x008CF310`) are not
    /// modelled: the model lifts the blockade and leaves the beaten navy where it is. Settled by porting the
    /// post-battle placement of `0x008F7880` (its retreat for every beaten character, not only this one).
    ///
    /// The result's +4 value 1, under which the destroyed-attacker lift runs, is the defender's win
    /// (CONFIRMED by decompile): the sequence's side record +0x68 is the attacker's (`0x0088353D`) and +0x6C
    /// the defender's (`0x00883579`), and the placement step sends the character of +0x6C, or of +0x68 when
    /// +4 is 1 (`0x008F7EE6`), into the retreat. PROVISIONAL: the exe's step also needs the winning side
    /// record's byte +60 set, whose writer is not traced; ours runs it whenever the other conditions hold.
    /// PROVISIONAL: the event the exe posts after this lift (vtable `0x01355C8C`, posted through `0x00E30700`
    /// at `0x008F89A0`) is not modelled; settled by tracing what listens for it.
    pub(crate) fn lift_after_sally(&mut self, (region, slot): (RegionId, usize), blockader: ForceId, attacker_won: bool) {
        let Some(faction) = self.world.forces.get(&blockader).map(|f| f.faction) else { return };
        let still = self.world.regions.get(&region).and_then(|r| r.slots.get(slot)).and_then(|s| s.blockade).is_some_and(|b| b.navy == blockader);
        if !still {
            return;
        }
        if attacker_won {
            // The beaten blockader's retreat order starts with the order helper, which lifts the blockade.
            self.lift_port_blockade(region, slot);
            return;
        }
        // A port holds at most one docked navy (the single handle at port +0x60), so testing every docked
        // navy is the exe's test of that one.
        let foreign = self.docked_navies(region, slot).any(|n| self.world.forces.get(&n).is_some_and(|f| f.faction != faction));
        if !foreign {
            self.lift_port_blockade(region, slot);
        }
    }

    /// Ends the blockade of port slot `slot` of `region` (`LiftPortSlotBlockade` `0x00A784E0`, CONFIRMED,
    /// disassembly; the port object at slot +0x1E4 is its `this`): the navy and port handles are unlinked
    /// (port +0x68, navy +0xE8), the trade price update with flag 0 runs
    /// ([`Self::update_commodity_prices_with`], `0x00BCB020(0)`, on every lift), and the port's
    /// +0x74 byte is cleared. Returns whether the slot was blockaded (a slot that is not leaves the exe's
    /// caller before the call: every caller but the end-blockade command tests the navy's +0xE8 handle
    /// first, [`Self::end_blockade`]).
    ///
    /// When a navy is docked in the port, the lift then floods and stores its commander's zone again
    /// (`0x00B084F0` on `GetPortDockedNavyCharacter`'s character, CONFIRMED by decompile: with the blockader
    /// unlinked, `0x00B1D100` lets the docked navy be an obstacle again,
    /// [`Self::navies_docked_in_blockaded_ports`]; ours [`Self::rebuild_zone`]). The lift's clear of the byte
    /// +0x58 of the slot's naval recruitment manager (slot +0x1E8) has no effect of its own: the queue's turn
    /// step writes the byte before it reads it (the hold in [`Self::region_turn`]). The slot's and the holder
    /// faction's events (slot +0x3C → +8 → +0x480, holder +0xA8 → +8 → +0x2A0) are not posted, as at the
    /// start (the tag there, [`Self::start_port_blockade`]).
    pub(crate) fn lift_port_blockade(&mut self, region: RegionId, slot: usize) -> bool {
        if !self.clear_port_blockade(region, slot) {
            return false;
        }
        self.update_commodity_prices_with(false);
        true
    }

    /// [`Self::lift_port_blockade`] without its price update, for a caller that runs the update once afterwards.
    fn clear_port_blockade(&mut self, region: RegionId, slot: usize) -> bool {
        let lifted = self.world.regions.get_mut(&region).and_then(|r| r.slots.get_mut(slot)).and_then(|s| s.blockade.take()).is_some();
        if lifted {
            let docked: Vec<CharacterId> =
                self.docked_navies(region, slot).filter_map(|f| self.world.forces.get(&f).and_then(|f| f.commander)).collect();
            for c in docked {
                self.rebuild_zone(c);
            }
        }
        lifted
    }

    /// The slots `pred` picks among the blockaded ones, in region and slot order; `only` limits the walk to one region.
    fn blockaded_slots(
        &self,
        only: Option<RegionId>,
        mut pred: impl FnMut(&super::Region, &super::RegionSlot, ForceId) -> bool,
    ) -> Vec<(RegionId, usize)> {
        let mut out = Vec::new();
        let (one, all) = match only {
            Some(id) => (self.world.regions.get(&id), None),
            None => (None, Some(self.world.regions.values())),
        };
        let regions = one.into_iter().chain(all.into_iter().flatten());
        for r in regions {
            for (i, s) in r.slots.iter().enumerate() {
                if let Some(b) = s.blockade
                    && pred(r, s, b.navy)
                {
                    out.push((r.id, i));
                }
            }
        }
        out
    }

    /// A peace's blockade step for character `c` towards the other side `other` (`ClearCommerceRaidsBetweenFactions`
    /// `0x00A25570`, CONFIRMED, disassembly `0x00A255CA`–`0x00A25627`, after his siege step): the navy he
    /// commands (character +0x2A8 as a navy, `0x009D3A80`, [`Self::force_of_kind`]: an army is not looked
    /// at), if it blockades a port (navy +0xE8) held by `other` (the port's slot +0x70, its holder: slot
    /// virtual +0x14, the slot's holder or else the region's owner), lifts that blockade
    /// ([`Self::lift_port_blockade`], with its price update). The exe reaches a navy only through its
    /// commander's step, so a blockading navy with no commander, or whose commander is in neither side's
    /// character list (+0x764 / +0x768; in the model, a commander id with no character), keeps blockading.
    /// The slots are collected first and lifted as a batch, which is order-free: a lift touches only its own
    /// slot, and each lift runs the price update.
    pub(crate) fn lift_blockade_on_peace(&mut self, c: CharacterId, other: FactionId) {
        let Some(navy) = self.force_of_kind(c, true) else { return };
        for (region, slot) in self.blockaded_slots(None, |r, s, n| n == navy && s.holder.unwrap_or(r.owner) == other) {
            self.lift_port_blockade(region, slot);
        }
    }

    /// The navy `navy` leaves the world (its destructor `0x0088EF90`, CONFIRMED: after its docked port's
    /// holder goes back to the region's owner, a blockade it holds, navy +0xE8, is lifted; skipped there only
    /// while the campaign is torn down, campaign +0xFC5), or its blockade ends for another reason
    /// ([`Self::begin_order`]).
    pub(crate) fn lift_blockade_of_navy(&mut self, navy: ForceId) {
        for (region, slot) in self.blockaded_slots(None, |_, _, n| n == navy) {
            self.lift_port_blockade(region, slot);
        }
    }

    /// The blockade part of an owner change's slot pass (`TransferRegionSlotOwners` `0x00B1B300`, CONFIRMED,
    /// disassembly), run after the slot holders have passed ([`Self::pass_region_slots`]): a blockaded port of
    /// `region` whose holder is now the new owner `faction` is lifted when the blockading navy's faction (navy
    /// +0x7C) is `faction` or not at war with it (`IsFactionAtWarWith` `0x008CE9B0`, [`Self::at_war`]). A port
    /// whose holder stayed (a docked navy kept it) keeps its blockade. The price updates are left to the
    /// caller (see [`Self::pass_region_slots`]); returns whether a blockade was lifted. The slots are lifted in one
    /// batch after the walk, which is order-free: each lift touches only its own slot.
    pub(crate) fn clear_blockades_after_slot_pass(&mut self, region: RegionId, faction: FactionId) -> bool {
        let lifted = self.blockaded_slots(Some(region), |_, s, navy| {
            s.holder.is_none_or(|h| h == faction)
                && self.world.forces.get(&navy).is_some_and(|f| !self.at_war(f.faction, faction))
        });
        for &(region, slot) in &lifted {
            self.clear_port_blockade(region, slot);
        }
        !lifted.is_empty()
    }
}
