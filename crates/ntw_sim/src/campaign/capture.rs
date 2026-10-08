//! Taking a settlement: the occupy / loot / liberate choice, and repairing damaged buildings.
//!
//! When a settlement changes hands by force, the original builds a capture report
//! (`0x00885F90`; vtable `0x013574DC`) whose slot 9 (`0x008F8D00`) runs once (CONFIRMED):
//! 1. **Preview** (`0x00B14930`) of three options, each `{buildings (building, new health)[], money,
//!    town wealth after, public order previews, public-order reduction}`:
//!    - option 0, **loot**: every settlement building with health > 1 takes a damage roll in 1–50 %
//!      (`0x00B14560`, below); money = Σ the rolls' values + clamp(`settlement_looting_pct_region_gdp`
//!      × town wealth, 150, 7000) + clamp(that var × 0.15 × GDP, 150, 6000) + `settlement_looting_base_loot`,
//!      times the army's looting multiplier (`0x008D20F0`); town wealth after = TW − min(`…_pct_region_gdp_reduction`
//!      × TW, 6000) (0 if that is not below TW); public-order reduction = `settlement_looting_publicorder_reduction`
//!      (10), or `settlement_looting_surrender_publicorder_reduction` (20) when the report's surrender flag is set;
//!    - option 1, **occupy**: the buildings whose chain's `building_chains` #2 number is 0 (all but `sArmy` and
//!      `tFactory`) take a roll in 50–99 %; no money, town wealth unchanged, no reduction;
//!    - option 2, **liberate**: offered when the region's rebel faction (`REGION` #24) is a campaign faction
//!      other than the capturer that the capturer lets regions return to (relationship #28) — see
//!      [`CampaignModel::liberation_target`] for what is and is not decoded.
//!
//!    Both rolls run for every capture, AI or human, in that order, on the campaign RNG.
//! 2. **Choice**: the AI's comes from `0x00AAABA0` (an AI-side virtual, `+0x2F8`; not decoded, §6); a human
//!    gets the three previews in a UI event (`+0xC78`) and answers with a choice; two report flags (+0x9A,
//!    +0x9B, set from the capturer's faction, `0x00B4DA40` / `0x00B4D9E0`) skip the choice and occupy.
//! 3. **Apply** (`0x008C0310`): 0 → `0x00B541E0` (buildings set to the previewed health, money paid through
//!    the economy, `0x00AAA580`: every population class's war-results base (+0x74) minus the reduction, town
//!    wealth set, region economy refreshed); 1 → `0x00B582E0` (buildings only); 2 → `0x00B4F090` when a
//!    liberation target was found (the region goes to it, `0x00B58A10`, and it gets an army), else loot.
//!
//! The damage roll `0x00B14560` (CONFIRMED from the listing): `r = rng.next16()`,
//! `frac = (hi − lo) × (r × 1.5259022e-5) + lo` in f32, new health = clamp(trunc(frac × health), 1, 99), value =
//! trunc((health − new) × 0.01 × level cost) × 4, then `min(value, max(15000, value / 4))` (spa: × 2, 10000).
//!
//! Repairs (`0x00B66260`, cost `0x00B66410`, CONFIRMED): a damaged building (health < 100, nothing being built in
//! its slot) is repaired for `round((100 + cost mod) × round((100 − health) × level cost × 0.01) × 0.01)` over
//! max(1, floor((100 − health) × 0.01 × level turns)) turns; an AI faction pays at most its treasury.

use super::effects::Effects;
use super::events::CampaignEvent;
use super::ids::{FactionId, ForceId, RegionId};
use super::world::{CampaignModel, ConstructionItem, SlotRef};
use super::CommandError;
use crate::rng::CaRng;

/// The loot pass's damage range (`0x00B14930` pushes 0.01 / 0.5, CONFIRMED).
pub const LOOT_DAMAGE: (f32, f32) = (0.01, 0.5);
/// The occupy pass's damage range (0.5 / 0.99, CONFIRMED).
pub const OCCUPY_DAMAGE: (f32, f32) = (0.5, 0.99);
/// `0x0131A7AC`: the scale of a 16-bit draw (1 / 65535 as f32).
const DRAW_SCALE: f32 = f32::from_bits(0x3780_0080);

/// The player's answer to a capture (the index `0x008C0310` takes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureChoice {
    /// 0: loot (money, town wealth cut, unrest, heavy damage).
    Loot,
    /// 1: occupy (light damage to the civil buildings).
    Occupy,
    /// 2: liberate (only when the preview found a target; otherwise it loots, as the original).
    Liberate,
}

/// One option of a capture preview.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CaptureOutcome {
    /// (slot index, health after) of each building the option damages, in slot order.
    pub buildings: Vec<(usize, u32)>,
    /// The fortification's health after (the fortification slot is rolled too, without money), if it has one.
    pub fortification: Option<u32>,
    /// Money the capturer receives.
    pub money: i32,
    /// The region's town wealth afterwards.
    pub town_wealth: u32,
    /// Subtracted from every population class's war-results base.
    pub public_order_reduction: i32,
    /// (lower, upper) public order of the region afterwards (the original's +0x18 / +0x1C previews), for the
    /// capture screen. Computed for human capturers only (display data; no RNG).
    pub public_order_after: Option<(f32, f32)>,
}

/// What the capture screen shows (the report's three option structs).
#[derive(Debug, Clone, PartialEq)]
pub struct CapturePreview {
    /// The region taken.
    pub region: RegionId,
    /// The faction that took it.
    pub faction: FactionId,
    /// The report's surrender flag (+0x99): the larger public-order reduction.
    pub surrender: bool,
    /// Option 0.
    pub loot: CaptureOutcome,
    /// Option 1.
    pub occupy: CaptureOutcome,
    /// Option 2: the faction a liberation would hand the region to.
    pub liberate: Option<FactionId>,
}

/// `0x00B14560`: one building's damage roll. Returns (new health, value). Draws once from `rng`.
pub fn damage_roll(rng: &mut CaRng, health: u32, level_cost: i32, (lo, hi): (f32, f32), spa: bool) -> (u32, u32) {
    let r = rng.next16();
    let frac = (hi - lo) * (r as f32 * DRAW_SCALE) + lo;
    let new = ((frac * health as f32) as i64).clamp(1, 99) as u32;
    let lost = health.wrapping_sub(new) as i32;
    let (m, cap) = if spa { (2u32, 10_000u32) } else { (4, 15_000) };
    let value = ((lost as f32 * 0.01 * level_cost as f32) as i32 as u32).wrapping_mul(m);
    (new, value.min(cap.max(value / m)))
}

impl CampaignModel {
    /// `0x008D20F0`: 1 + the two largest `looting_increase` × 0.01 among the characters of the army's units
    /// (CONFIRMED shape; effect index 0x87).
    pub fn looting_multiplier(&self, force: Option<ForceId>) -> f32 {
        let (mut a, mut b) = (0.0f32, 0.0f32);
        let Some(f) = force.and_then(|f| self.world.forces.get(&f)) else { return 1.0 };
        for c in f.units.iter().filter_map(|u| u.character) {
            let v = Effects::character_effects(self, c).get("looting_increase") * 0.01;
            if v > a {
                b = a;
                a = v;
            } else if v > b {
                b = v;
            }
        }
        a + 1.0 + b
    }

    /// The settlement's own slots (the town buildings the capture report walks): the `settlement:` slots.
    /// INFERRED: the report walks the settlement object's slot list; ports and minor towns are other
    /// residences. The fortification slot ([`Region::fortification`](super::Region::fortification)) is rolled after them, without money.
    fn settlement_slots(&self, region: RegionId) -> Vec<usize> {
        self.world.regions.get(&region).map_or(Vec::new(), |r| {
            r.slots.iter().enumerate().filter(|(_, s)| s.key.starts_with("settlement:")).map(|(i, _)| i).collect()
        })
    }

    /// The faction a capture of `region` by `faction` may liberate (`0x00B14930`'s third option, grand
    /// campaign path). CONFIRMED: the region's rebel-faction record names a campaign faction, other than the
    /// capturer, and the capturer's relationship to it allows regions back (#28, default true). INFERRED: the
    /// faction's +0x824 flag read as "out of the game" — we offer it when the faction holds no region (the
    /// branch for a faction still holding land, +0x72C / a count of 1, is not decoded: not offered). Not
    /// modelled: a campaign-wide switch (+0x124) and a flag of the faction record (+0x4C), both UNKNOWN.
    pub fn liberation_target(&self, region: RegionId, faction: FactionId) -> Option<FactionId> {
        let key = self.world.region_rebel_factions.get(&region)?;
        let target = self.world.factions.values().find(|f| &f.key == key)?.id;
        if target == faction || self.world.regions.values().any(|r| r.owner == target) {
            return None;
        }
        let allows = self.world.relationships.get(&(faction, target)).is_none_or(|r| r.allows_region_return);
        allows.then_some(target)
    }

    /// `0x00B14930`: the three options of a capture. Draws from the campaign RNG (loot pass, then occupy pass).
    pub fn capture_preview(&mut self, region: RegionId, faction: FactionId, by: Option<ForceId>, surrender: bool) -> CapturePreview {
        let spa = self.rules.campaign == "spa_napoleon";
        let slots = self.settlement_slots(region);
        let (tw, gdp) = self.world.regions.get(&region).map_or((0, 0), |r| (r.town_wealth, r.gdp));
        let level_of = |m: &CampaignModel, i: usize| {
            m.world.regions.get(&region).and_then(|r| r.slots.get(i)).and_then(|s| s.building.clone())
        };
        let mut loot = CaptureOutcome { money: self.rules.var("settlement_looting_base_loot", 0.0) as i32, ..Default::default() };
        for &i in &slots {
            let Some(b) = level_of(self, i).filter(|b| b.health > 1) else { continue };
            let cost = self.rules.buildings.get(&b.level_key).map_or(0, |x| x.cost);
            let (h, v) = damage_roll(&mut self.rng, b.health, cost, LOOT_DAMAGE, spa);
            loot.buildings.push((i, h));
            loot.money = loot.money.wrapping_add(v as i32);
        }
        let fort = |m: &CampaignModel| m.world.regions.get(&region).and_then(|r| r.fortification.clone()).filter(|b| b.health > 1);
        if let Some(b) = fort(self) {
            let cost = self.rules.buildings.get(&b.level_key).map_or(0, |x| x.cost);
            loot.fortification = Some(damage_roll(&mut self.rng, b.health, cost, LOOT_DAMAGE, spa).0);
        }
        let pct = self.rules.var("settlement_looting_pct_region_gdp", 0.15);
        let clamp = |x: f32, hi: f32| if 150.0 > x { 150.0 } else if x > hi { hi } else { x };
        loot.money = (loot.money as f32 + clamp(pct * tw as f32, 7000.0)) as i32;
        let from_gdp = (f64::from(pct) * f64::from(0.15f32) * f64::from(gdp as f32)) as f32;
        loot.money = (loot.money as f32 + clamp(from_gdp, 6000.0)) as i32;
        let cut = (self.rules.var("settlement_looting_pct_region_gdp_reduction", 0.8) * tw as f32).min(6000.0) as i32 as u32;
        loot.town_wealth = tw.saturating_sub(cut);
        let po_var = if surrender { "settlement_looting_surrender_publicorder_reduction" } else { "settlement_looting_publicorder_reduction" };
        loot.public_order_reduction = self.rules.var(po_var, if surrender { 20.0 } else { 10.0 }) as i32;
        loot.money = (loot.money as f32 * self.looting_multiplier(by)) as i32;

        let mut occupy = CaptureOutcome { town_wealth: tw, ..Default::default() };
        for &i in &slots {
            let Some(b) = level_of(self, i).filter(|b| b.health > 1) else { continue };
            let Some(rules) = self.rules.buildings.get(&b.level_key) else { continue };
            if self.rules.chain_kinds.get(&rules.chain).copied().unwrap_or(0) != 0 {
                continue;
            }
            let (h, _) = damage_roll(&mut self.rng, b.health, rules.cost, OCCUPY_DAMAGE, spa);
            occupy.buildings.push((i, h));
        }
        // The fortification: no chain test in this pass (CONFIRMED).
        if let Some(b) = fort(self) {
            let cost = self.rules.buildings.get(&b.level_key).map_or(0, |x| x.cost);
            occupy.fortification = Some(damage_roll(&mut self.rng, b.health, cost, OCCUPY_DAMAGE, spa).0);
        }
        let mut preview = CapturePreview { region, faction, surrender, loot, occupy, liberate: self.liberation_target(region, faction) };
        if self.turn.humans.contains(&faction) {
            for o in [&mut preview.loot, &mut preview.occupy] {
                let mut m = self.clone();
                m.apply_outcome(region, faction, o, false);
                let po = super::economy::public_order(&m, region);
                o.public_order_after = Some((po.lower, po.upper));
            }
        }
        preview
    }

    /// Applies one option's changes to the region (and, with `pay`, the money to `faction`).
    fn apply_outcome(&mut self, region: RegionId, faction: FactionId, o: &CaptureOutcome, pay: bool) {
        let Some(r) = self.world.regions.get_mut(&region) else { return };
        for &(i, h) in &o.buildings {
            if let Some(b) = r.slots.get_mut(i).and_then(|s| s.building.as_mut()) {
                b.health = h;
            }
        }
        // The fortification goes through its damage object (vtable +0xC0 in `0x00B541E0`); the model sets its health
        // to the rolled value (INFERRED equivalent).
        if let (Some(h), Some(b)) = (o.fortification, r.fortification.as_mut()) {
            b.health = h;
        }
        r.town_wealth = o.town_wealth;
        if o.public_order_reduction != 0 {
            // 0x008EF780 on every class: class +0x74 (the war-results base) -= reduction. A region loaded without
            // classes gets the government's two.
            if r.class_bases.is_empty() {
                let (upper, lower) = super::economy::government_classes(self, faction);
                let r = self.world.regions.get_mut(&region).expect("checked");
                r.class_bases = vec![(lower, 0, 0), (upper, 0, 0)];
            }
            let r = self.world.regions.get_mut(&region).expect("checked");
            for c in &mut r.class_bases {
                c.2 -= o.public_order_reduction;
            }
            // spa_napoleon (`0x00AAA5B0`, CONFIRMED): the population turns against the looter: its alignment loses 0.5 of
            // the share (floored at 0) and the other alignment gains 0.5 (capped at 1). The rebels skip it (`0x008CEEF0`).
            if self.rules.campaign == "spa_napoleon" {
                let own = self.world.faction_details.get(&faction).map(|d| d.religion.clone()).unwrap_or_default();
                let other = if own == "align_pro_french" { "align_anti_french" } else { "align_pro_french" };
                let r = self.world.regions.get_mut(&region).expect("checked");
                let has = |k: &str, r: &super::world::Region| r.religions.iter().any(|(x, _)| x == k);
                if !own.is_empty() && has(&own, r) && has(other, r) {
                    for (k, s) in &mut r.religions {
                        if *k == own {
                            *s = (*s - 0.5).max(0.0);
                        } else if k == other {
                            *s = (*s + 0.5).min(1.0);
                        }
                    }
                }
            }
        }
        if pay && o.money != 0 && let Some(f) = self.world.factions.get_mut(&faction) {
            f.treasury = f.treasury.saturating_add(o.money);
        }
    }

    /// After a capture by force: builds the preview, then lets the AI choose at once or leaves the choice to a
    /// human ([`CampaignModel::pending_capture`], answered with `ChooseCapture`).
    pub(crate) fn settle_capture(&mut self, region: RegionId, faction: FactionId, by: Option<ForceId>, surrender: bool, events: &mut Vec<CampaignEvent>) {
        let preview = self.capture_preview(region, faction, by, surrender);
        if self.turn.humans.contains(&faction) {
            // A capture still waiting is settled as an occupation first (PROVISIONAL: the original's screen is modal).
            if let Some(old) = self.pending_capture.take() {
                self.resolve_capture(old, CaptureChoice::Occupy, events);
            }
            events.push(CampaignEvent::CaptureChoicePending { region, faction });
            self.pending_capture = Some(preview);
        } else {
            // The AI's choice (0x00AAABA0) is not decoded: PROVISIONAL occupy (AI_RESEARCH.md §7).
            self.resolve_capture(preview, CaptureChoice::Occupy, events);
        }
    }

    /// A capture by force without the battle: `faction` takes the settlement (`occupy`, as after a
    /// won assault or an undefended settlement) and the capture report follows (`settle_capture`:
    /// a human's choice waits in [`CampaignModel::pending_capture`]). For harnesses and tests that
    /// stage a capture; the game's own path goes through the battle or `AttackSettlement`.
    pub fn capture_by_force(&mut self, region: RegionId, faction: FactionId, by: Option<ForceId>) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        self.occupy(region, faction, by, &mut events);
        self.settle_capture(region, faction, by, false, &mut events);
        events
    }

    /// `0x008C0310`: applies a choice.
    pub(crate) fn resolve_capture(&mut self, p: CapturePreview, choice: CaptureChoice, events: &mut Vec<CampaignEvent>) {
        let choice = match choice {
            CaptureChoice::Liberate if p.liberate.is_none() => CaptureChoice::Loot,
            c => c,
        };
        let money = match choice {
            CaptureChoice::Loot => {
                self.apply_outcome(p.region, p.faction, &p.loot, true);
                p.loot.money
            }
            CaptureChoice::Occupy => {
                self.apply_outcome(p.region, p.faction, &p.occupy, false);
                0
            }
            CaptureChoice::Liberate => {
                let target = p.liberate.expect("checked");
                // 0x00B58A10: the region passes to the liberated faction. The capturer's army leaves the
                // settlement (INFERRED). The army the original raises for it (0x00B4F090: units chosen from
                // what the region's sArmy / rHorse / tGuns buildings allow, a new general) is not modelled.
                if let Some(g) = self.world.regions.get(&p.region).and_then(|r| r.garrison)
                    && let Some(c) = self.world.forces.get(&g).and_then(|f| f.commander)
                    && let Some(ch) = self.world.characters.get_mut(&c)
                {
                    ch.garrisoned_in = None;
                }
                self.occupy(p.region, target, None, events);
                0
            }
        };
        events.push(CampaignEvent::CaptureResolved { region: p.region, faction: p.faction, choice, money });
    }

    /// Command `ChooseCapture`: the human's answer to [`CampaignModel::pending_capture`].
    pub(crate) fn choose_capture(&mut self, choice: CaptureChoice) -> Result<Vec<CampaignEvent>, CommandError> {
        let p = self.pending_capture.take().ok_or(CommandError::Unsupported("no capture is waiting for a choice"))?;
        let mut events = Vec::new();
        self.resolve_capture(p, choice, &mut events);
        Ok(events)
    }

    /// `0x00B66410`: what repairing the building in `slot` costs now (0 when there is nothing to repair).
    /// For an AI faction the original caps it at the treasury.
    pub fn repair_cost(&self, region: RegionId, slot: SlotRef) -> i32 {
        let cost = self.repair_cost_uncapped(region, slot);
        match self.world.regions.get(&region) {
            Some(r) if !self.turn.humans.contains(&r.owner) => cost.min(self.world.factions.get(&r.owner).map_or(cost, |f| f.treasury)),
            _ => cost,
        }
    }

    /// [`Self::repair_cost`] without the AI's treasury cap.
    pub fn repair_cost_uncapped(&self, region: RegionId, slot: SlotRef) -> i32 {
        let Some(r) = self.world.regions.get(&region) else { return 0 };
        let Some(b) = r.building_at(slot) else { return 0 };
        let Some(rules) = self.rules.buildings.get(&b.level_key) else { return 0 };
        use super::commands::{building_cost, fistp, unsigned_f32};
        // All f32 (SSE) with the ints read unsigned, each step rounded by FISTP (CONFIRMED, `0x00B66410`).
        // A fortification (`0x00A8B580`): FISTP(cost × (1.0 − strength)) from its damage object; the model keeps its
        // strength as the health (INFERRED equivalent: strength = health × 0.01, without the damage object's own
        // percent rounding).
        // Health is unsigned in the exe (+0x18) as here. Above 100 the base goes negative and the last step reads it
        // unsigned (about 4.29e9): the result is 0x80000000 unless the modifier is about −50 or below, which brings
        // the product back into range (a large finite cost), as in the exe. No repair starts there (`0x00B1A6B0`:
        // health < 100).
        let base = if slot == SlotRef::Walls {
            fistp(unsigned_f32(rules.cost) * (1.0 - b.health as f32 * 0.01))
        } else {
            fistp((100.0 - b.health as f32) * unsigned_f32(rules.cost) * 0.01)
        };
        // A zero base costs zero whatever the modifier: skip building the region's effect set.
        if base == 0 {
            return 0;
        }
        // Then FISTP((modifier + 100) × base × 0.01) with base read unsigned: the construction cost's step
        // (`building_cost`), and its modifier is the same lookup (0x00E1EF10(chain, 0) on the 0x00A67530 set).
        building_cost(base, self.building_cost_modifier(region, &b.level_key))
    }

    /// Whether the building in `slot` can be repaired now (`0x00B1A6B0`): damaged, held by the owner, and
    /// nothing being built or repaired there.
    ///
    /// PROVISIONAL: the road is never repairable here. The exe's road repair is not traced; nothing
    /// in the game damages a road (capture rolls damage on the slots and the walls only), and the
    /// original's infrastructure panel hides the repair button (`Construction.lua:408`).
    pub fn can_repair(&self, region: RegionId, slot: SlotRef) -> bool {
        if slot == SlotRef::Road {
            return false;
        }
        let Some(r) = self.world.regions.get(&region) else { return false };
        let held = r.slot_held(slot);
        r.building_at(slot).is_some_and(|b| b.health < 100)
            && held
            && !r.construction.iter().any(|c| c.slot == slot)
    }

    /// Command `RepairBuilding` (`0x00B66260`): charges the repair cost when it is above 0 and queues the
    /// repair, with no affordability test (its callers make it: the panel's `can_afford_repair` and the AI,
    /// [`super::treasury::can_pay_repair`]). The repair is a
    /// construction item of the building's own level (it ends with health 100); its length is
    /// max(1, floor((100 − health) × 0.01 × level turns)).
    pub(crate) fn repair_building(&mut self, region: RegionId, slot: SlotRef) -> Result<Vec<CampaignEvent>, CommandError> {
        let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
        self.check_turn(owner)?;
        self.queue_repair(region, slot)
    }

    /// How many turns repairing the building in `slot` takes: max(1, floor((100 − health) × 0.01 ×
    /// level turns)) (0 when nothing stands there). The building keeps its health while it is being
    /// repaired, so this is also the length of a repair already queued.
    pub fn repair_turns(&self, region: RegionId, slot: SlotRef) -> u32 {
        let Some(b) = self.world.regions.get(&region).and_then(|r| r.building_at(slot)) else { return 0 };
        let level_turns = self.rules.buildings.get(&b.level_key).map_or(1, |x| x.turns);
        ((100.0 - b.health as f32) * 0.01 * level_turns as f32).floor().max(1.0) as u32
    }

    fn queue_repair(&mut self, region: RegionId, slot: SlotRef) -> Result<Vec<CampaignEvent>, CommandError> {
        let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
        if !self.can_repair(region, slot) {
            return Err(CommandError::CannotBuild("nothing to repair in that slot".into()));
        }
        let cost = self.repair_cost(region, slot);
        let turns = self.repair_turns(region, slot);
        let b = self.world.regions[&region].building_at(slot).cloned().expect("checked");
        let f = self.world.factions.get_mut(&owner).ok_or(CommandError::UnknownFaction(owner))?;
        // `0x00B66260` tests no funds and charges only a cost above 0 (signed); the item keeps the cost either way
        // (`0x00AE7010` → item +0x14), and a cancel credits that stored cost back ([`super::treasury::refund`]).
        if cost > 0 {
            super::treasury::pay(&mut f.treasury, cost);
        }
        self.world.regions.get_mut(&region).expect("checked").construction.push(ConstructionItem {
            slot,
            level_key: b.level_key,
            turns_remaining: turns,
            cost,
        });
        Ok(Vec::new())
    }

    /// Whether the building in `slot` can be demolished now: standing, held by the owner, and
    /// nothing being built or repaired there (mirrors [`Self::can_repair`]). The original's
    /// `CanDemolishBuilding` (`0x009B7920`, CONFIRMED: slot resolve `0x009BB6E0` shared with
    /// cancel/repair, not the `settlement_road` slot, `0x00A91FC0`, selected item present)
    /// excludes the road, and so does this ([`SlotRef::Road`] is never demolished).
    pub fn can_demolish(&self, region: RegionId, slot: SlotRef) -> bool {
        if slot == SlotRef::Road {
            return false;
        }
        let Some(r) = self.world.regions.get(&region) else { return false };
        let held = r.slot_held(slot);
        r.building_at(slot).is_some()
            && held
            && !r.construction.iter().any(|c| c.slot == slot)
    }

    /// Command `DemolishBuilding` (exe `DemolishBuilding` `0x009E2380` → `0x009B9590` → queue id
    /// `0x84`; fort `DemolishFort` `0x009E23D0` → `0x009BA250` → queue id `0x89`; both CONFIRMED):
    /// removes the standing building at once, with no refund (PROVISIONAL: the original's
    /// resolution — refund, timing — is UNKNOWN beyond the queued ids). Fires no script event
    /// (none known, like [`CampaignCommand::CancelConstruction`]).
    pub(crate) fn demolish_building(&mut self, region: RegionId, slot: SlotRef) -> Result<Vec<CampaignEvent>, CommandError> {
        let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
        self.check_turn(owner)?;
        if !self.can_demolish(region, slot) {
            return Err(CommandError::CannotBuild("nothing to demolish in that slot".into()));
        }
        let r = self.world.regions.get_mut(&region).expect("checked");
        if let Some(b) = r.building_mut(slot) {
            *b = None;
        }
        Ok(Vec::new())
    }

    /// The AI's repairs (PROVISIONAL stand-in for the AI's call of `0x00AA4910`, which repairs a building when
    /// it can be repaired (`0x00B1A6B0`) and paid for (`0x00B16430`: [`super::treasury::can_pay_repair`] on the
    /// repair cost; an AI faction's cost is capped at its treasury, so the test always passes and is not repeated
    /// here, CONFIRMED)): at an AI faction's turn start, every damaged building, the settlement walls included (an
    /// ordinary building slot in the exe, CONFIRMED). This is what lets a damaged AI building be upgraded again:
    /// upgrades need full health ([`Self::can_build`]). The other gates of `0x00AA4910` (`0x0047B090` /
    /// `0x00A8B2D0`) are not modelled.
    pub(crate) fn ai_repairs(&mut self, faction: FactionId) {
        if self.turn.humans.contains(&faction) {
            return;
        }
        let regions: Vec<RegionId> = self.world.regions.values().filter(|r| r.owner == faction).map(|r| r.id).collect();
        for region in regions {
            let n = self.world.regions[&region].slots.len();
            for slot in (0..n).map(SlotRef::Slot).chain([SlotRef::Walls]) {
                if self.can_repair(region, slot) {
                    let _ = self.queue_repair(region, slot);
                }
            }
        }
    }
}
