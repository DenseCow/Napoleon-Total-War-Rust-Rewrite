//! Port blockades: a navy blockading a port slot ([`super::RegionSlot::blockade`], the port object's
//! +0x68 handle linked with the navy's +0xE8 handle, saved as `PORT_GARRISON_MANAGER` #1/#2).
//!
//! The lift `LiftPortSlotBlockade` (`0x00A784E0`, CONFIRMED) is the one function every path that ends a
//! blockade calls; [`CampaignModel::lift_port_blockade`] is ours. Its callers in the exe (CONFIRMED,
//! xrefs and disassembly):
//! * a peace (`MakePeace` `0x00B262C0` → `ClearCommerceRaidsBetweenFactions` `0x00A25570`, the call at
//!   `0x00A25627`): [`CampaignModel::lift_blockade_on_peace`];
//! * an owner change's slot pass (`TransferRegionSlotOwners` `0x00B1B300`, the call at `0x00B1B3EF`):
//!   [`CampaignModel::clear_blockades_after_slot_pass`];
//! * the navy's destructor (`0x0088EF90`): [`CampaignModel::lift_blockade_of_navy`];
//! * a character's move that returns 2 (`0x008AC010`), the post-battle state machine (`0x008F7880`), the
//!   attack step's helper (`0x008FF470`) and the end-blockade command (`CCQ_CHARACTER_END_BLOCKADE`, the
//!   handler at `0x00932710`): not in the model, nor is the blockade's start (`0x00A66F80`, from the
//!   move-order step `0x0094B9B0`), so no blockade begins in our campaign yet (BACKLOG).

use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::CampaignModel;

impl CampaignModel {
    /// Ends the blockade of port slot `slot` of `region` (`LiftPortSlotBlockade` `0x00A784E0`, CONFIRMED,
    /// disassembly; the port object at slot +0x1E4 is its `this`): the navy and port handles are unlinked
    /// (port +0x68, navy +0xE8), the trade price update with flag 0 runs
    /// ([`Self::update_commodity_prices_with`], `0x00BCB020(0)`, on every lift), and the port's
    /// +0x74 byte is cleared. Returns whether the slot was blockaded (a slot that is not leaves the exe's
    /// caller before the call: every caller tests the navy's +0xE8 handle first).
    ///
    /// PROVISIONAL: the lift clears the byte +0x58 of the slot's naval recruitment manager (slot +0x1E8, set
    /// to 1 by the blockade start `0x00A66F80`; its reader is not found); the model has no such byte.
    /// PROVISIONAL: its call `0x00B084F0` on `GetPortDockedNavyCharacter`'s character when a navy is docked
    /// (the character's zone/obstacle refresh, see `zones.rs`) is not traced here and not run. The slot's and
    /// the holder faction's events (slot +0x3C → +8 → +0x480, holder +0xA8 → +8 → +0x2A0) have no listener
    /// in the model.
    pub(crate) fn lift_port_blockade(&mut self, region: RegionId, slot: usize) -> bool {
        if !self.clear_port_blockade(region, slot) {
            return false;
        }
        self.update_commodity_prices_with(false);
        true
    }

    /// [`Self::lift_port_blockade`] without its price update, for a caller that runs the update once afterwards.
    fn clear_port_blockade(&mut self, region: RegionId, slot: usize) -> bool {
        self.world.regions.get_mut(&region).and_then(|r| r.slots.get_mut(slot)).and_then(|s| s.blockade.take()).is_some()
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
    /// while the campaign is torn down, campaign +0xFC5).
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
