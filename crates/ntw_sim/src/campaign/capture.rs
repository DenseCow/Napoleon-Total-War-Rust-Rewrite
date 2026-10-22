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
//! 2. **Choice**: the AI's comes from `0x00AAABA0` (the capturer's AI virtual `+0x2F8`, `0x00D8C8B0`, CONFIRMED;
//!    the [`CAPTURE_AI_CHOICE`](super::seams::CAPTURE_AI_CHOICE) seam, [`super::capture_ai`]); a human gets the
//!    three previews in a UI event (`+0xC78`) and answers with a choice; two report flags (+0x9A, +0x9B, set
//!    from the rebel data, `0x00B4DA40` / `0x00B4D9E0`) skip the choice and occupy.
//! 3. **Apply** (`0x008C0310`): 0 → `0x00B541E0` (buildings set to the previewed health, money paid through
//!    the economy, `0x00AAA580`: every population class's resistance base (+0x74) minus the reduction, town
//!    wealth set, region economy refreshed); 1 → `0x00B582E0` (buildings only); 2 → `0x00B4F090` when a
//!    liberation target was found (the region goes to it, `0x00B58A10`, and it gets an army), else loot.
//!
//! The damage roll `0x00B14560` (CONFIRMED from the listing): `r = rng.next16()`,
//! `frac = (hi − lo) × (r × 1.5259022e-5) + lo` in f32, new health = clamp(trunc(frac × health), 1, 99), value =
//! trunc((health − new) × 0.01 × level cost) × 4, then `min(value, max(15000, value / 4))` (the campaign's
//! [`loot_value`](super::features::CampaignFeatures::loot_value); spa: × 2, 10000).
//!
//! Repairs (`0x00B66260`, cost `0x00B66410`, CONFIRMED): a damaged building (health < 100, nothing being built in
//! its slot) is repaired for `round((100 + cost mod) × round((100 − health) × level cost × 0.01) × 0.01)` over
//! max(1, floor((100 − health) × 0.01 × level turns)) turns; an AI faction pays at most its treasury.

use super::effects::Effects;
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::treaties::AttitudeEvent;
use super::world::{CampaignModel, ConstructionItem, ConstructionKind, SlotRef, Stance};
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CaptureOutcome {
    /// (slot index, health after) of each building the option damages, in slot order.
    pub buildings: Vec<(usize, u32)>,
    /// The fortification's health after (the fortification slot is rolled too, without money), if it has one.
    pub fortification: Option<u32>,
    /// Money the capturer receives.
    pub money: i32,
    /// The region's town wealth afterwards.
    pub town_wealth: u32,
    /// Subtracted from every population class's resistance base.
    pub public_order_reduction: i32,
    /// (lower, upper): the region's [`public_order`](super::economy::public_order) after this option, for the
    /// capture screen's `LowerOrder` / `UpperOrder`. CONFIRMED (`0x00B14930`): option +0x1C = `0x00A610C0`, the net
    /// order `0x008E2AB0` of the government's lower class; +0x18 = `0x00A61400`, its upper class; each 0 when the
    /// region has no such class; the classes are those the region tests `0x008CE260` / `0x008CE320` name
    /// ([`region_government_classes`](super::economy::region_government_classes): the government's, or for a
    /// rebel region its first risen army's, none with no army); `0x009AB1B0` reads +0x1C as `LowerOrder`,
    /// +0x18 as `UpperOrder`. Not traced: the effect set the copy is recomputed with (`0x00B0D3B0`, tagged
    /// where the preview is computed).
    /// The exe computes it for every capturer; ours for human capturers only (display data, no RNG draw).
    pub public_order_after: Option<(f32, f32)>,
}

/// What the capture screen shows (the report's three option structs).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// The capturing force's commander at the capture (report +0x8: `0x00885F90`'s first argument, from
    /// force +0x74, in `0x00B58560` / `0x00B58C00`); the loot choice posts it in
    /// `CharacterLootedSettlement`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub looter: Option<CharacterId>,
}

/// `0x00B14560`: one building's damage roll. Returns (new health, value). Draws once from `rng`.
/// `(m, cap)` is the campaign's loot value ([`super::features::CampaignFeatures::loot_value`]).
pub fn damage_roll(rng: &mut CaRng, health: u32, level_cost: i32, (lo, hi): (f32, f32), (m, cap): (u32, u32)) -> (u32, u32) {
    let r = rng.next16();
    let frac = (hi - lo) * (r as f32 * DRAW_SCALE) + lo;
    let new = ((frac * health as f32) as i64).clamp(1, 99) as u32;
    let lost = health.wrapping_sub(new) as i32;
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
    /// capturer, and the capturer's relationship to it allows regions back (#28, default true), and the faction is out of the game
    /// (+0x824 set, [`Self::in_the_game`]; CONFIRMED meaning: the destruction `0x008AB6C0` sets it). The
    /// branch for a faction still in the game (+0x72C / a count of 1) is not decoded: not offered. The
    /// scripts' switch (episodic restrictions +0x124, [`World::liberation_options_disabled`]) offers
    /// none, on both of the exe's paths (tests `0x00B15134` / `0x00B15214`, CONFIRMED). Not modelled:
    /// a flag of the faction record (+0x4C), UNKNOWN.
    ///
    /// [`World::liberation_options_disabled`]: super::World::liberation_options_disabled
    pub fn liberation_target(&self, region: RegionId, faction: FactionId) -> Option<FactionId> {
        if self.world.liberation_options_disabled {
            return None;
        }
        let key = self.world.region_rebel_factions.get(&region)?;
        let target = self.world.factions.values().find(|f| &f.key == key)?.id;
        if target == faction || self.in_the_game(target) {
            return None;
        }
        let allows = self.world.relationships.get(&(faction, target)).is_none_or(|r| r.allows_region_return);
        allows.then_some(target)
    }

    /// `0x00B14930`: the three options of a capture. Draws from the campaign RNG (loot pass, then occupy pass).
    pub fn capture_preview(&mut self, region: RegionId, faction: FactionId, by: Option<ForceId>, surrender: bool) -> CapturePreview {
        let loot_value = self.rules.features.loot_value;
        let slots = self.settlement_slots(region);
        let (tw, gdp) = self.world.regions.get(&region).map_or((0, 0), |r| (r.town_wealth, r.gdp));
        let level_of = |m: &CampaignModel, i: usize| {
            m.world.regions.get(&region).and_then(|r| r.slots.get(i)).and_then(|s| s.building.clone())
        };
        let mut loot = CaptureOutcome { money: self.rules.var("settlement_looting_base_loot", 0.0) as i32, ..Default::default() };
        for &i in &slots {
            let Some(b) = level_of(self, i).filter(|b| b.health > 1) else { continue };
            let cost = self.rules.buildings.get(&b.level_key).map_or(0, |x| x.cost);
            let (h, v) = damage_roll(&mut self.rng, b.health, cost, LOOT_DAMAGE, loot_value);
            loot.buildings.push((i, h));
            loot.money = loot.money.wrapping_add(v as i32);
        }
        let fort = |m: &CampaignModel| m.world.regions.get(&region).and_then(|r| r.fortification.clone()).filter(|b| b.health > 1);
        if let Some(b) = fort(self) {
            let cost = self.rules.buildings.get(&b.level_key).map_or(0, |x| x.cost);
            loot.fortification = Some(damage_roll(&mut self.rng, b.health, cost, LOOT_DAMAGE, loot_value).0);
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
            let (h, _) = damage_roll(&mut self.rng, b.health, rules.cost, OCCUPY_DAMAGE, loot_value);
            occupy.buildings.push((i, h));
        }
        // The fortification: no chain test in this pass (CONFIRMED).
        if let Some(b) = fort(self) {
            let cost = self.rules.buildings.get(&b.level_key).map_or(0, |x| x.cost);
            occupy.fortification = Some(damage_roll(&mut self.rng, b.health, cost, OCCUPY_DAMAGE, loot_value).0);
        }
        let looter = by.and_then(|f| self.world.forces.get(&f)).and_then(|f| f.commander);
        let mut preview = CapturePreview { region, faction, surrender, loot, occupy, liberate: self.liberation_target(region, faction), looter };
        if self.turn.humans.contains(&faction) {
            for o in [&mut preview.loot, &mut preview.occupy] {
                let mut m = self.clone();
                m.apply_outcome(region, faction, o, false);
                // INFERRED: the exe recomputes the copy's classes with the region's effect set adjusted by
                // `0x00B0D3B0(capturer, region, set)` (not traced) less the damaged buildings' effects; ours reads
                // the option-applied model's `region_effect_set` (the governing faction's tax bundles), which already
                // excludes the damaged buildings' effects (`Region::effect_buildings` keeps only full-health ones).
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
            // 0x008EF780 on every class: class +0x74 (the resistance base) -= reduction. A region loaded without
            // classes gets the government's two.
            self.ensure_population_classes(region, faction);
            let r = self.world.regions.get_mut(&region).expect("checked");
            for c in &mut r.class_bases {
                c.resistance -= o.public_order_reduction;
            }
            // spa_napoleon (`0x00AAA5B0`, CONFIRMED; the campaign's `looting_alignment`): the population turns against
            // the looter: its alignment loses 0.5 of the share (floored at 0) and the other alignment gains 0.5 (capped
            // at 1). The rebels skip it (`0x008CEEF0`).
            if let Some((pro, anti)) = &self.rules.features.looting_alignment {
                let own = self.world.faction_details.get(&faction).map(|d| d.religion.clone()).unwrap_or_default();
                let other = if own == *pro { anti.as_str() } else { pro.as_str() };
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
            // The option's money +0x10, when not 0, is credited with category 0 (`0x00B541E0` → `0x00BB3810` at
            // `0x00B5431C`, CONFIRMED): 32-bit wrapping.
            super::treasury::credit(&mut f.treasury, o.money);
        }
    }

    /// The campaign's [`LootResentment`](super::features::LootResentment) after `looter` loots `region`
    /// (`0x00B541E0`, CONFIRMED, after the alignment shift `0x00AAA580`): the region's rebel faction key (region
    /// +0x10C) picks the resentful homeland faction (the first matching row, found by key, `FindCampaignFactionByKey` `0x00955B70`); unless it
    /// is the looter itself (`0x00898B80`) or the looter is exempt, its record towards the looter adds `value` to
    /// the factor (`0x00B544F0`: only when the two are not at war; then `0x00B02CB0` with the factor event's drift
    /// and limit, `0x00B27FF0`). No relation-change broadcast follows (CONFIRMED: neither function makes one), so
    /// the AI's stored relation belief keeps its old value ([`super::director`]).
    fn loot_resentment(&mut self, region: RegionId, looter: FactionId) {
        let Some(rule) = &self.rules.features.loot_resentment else { return };
        let Some(rebels) = self.world.region_rebel_factions.get(&region) else { return };
        let Some((_, homeland)) = rule.homelands.iter().find(|(r, _)| r == rebels) else { return };
        let Some(resentful) = self.world.factions.values().find(|f| &f.key == homeland).map(|f| f.id) else { return };
        let exempt = rule.exempt_looter.is_some() && self.world.factions.get(&looter).map(|f| f.key.as_str()) == rule.exempt_looter.as_deref();
        if resentful == looter || exempt || self.world.stance(resentful, looter) == Stance::War {
            return;
        }
        // A row with an unknown factor was logged and dropped when the rules loaded (`CampaignFeatures::validated`).
        let Some(e) = rule.event() else { return };
        let (factor, value) = (rule.factor.clone(), rule.value);
        self.relationship_mut(resentful, looter).add_factor(&factor, AttitudeEvent { value, ..e });
    }

    /// A capture by force: `faction` occupies the settlement ([`CampaignModel::occupy`]), the report builds the
    /// preview, then the AI chooses at once or the choice waits for a human ([`CampaignModel::pending_capture`],
    /// answered with `ChooseCapture`). The report keeps the settlement's owner before the capture (+0x10) for the
    /// AI's choice.
    pub(crate) fn settle_capture(&mut self, region: RegionId, faction: FactionId, by: Option<ForceId>, surrender: bool, events: &mut Vec<CampaignEvent>) {
        let previous = self.world.regions.get(&region).map(|r| r.owner);
        // The skip flags are taken before the settlement changes hands (`0x00B586AF`, before `0x00B449F0`).
        let skip_choice = self.capture_skips_choice(region, by);
        self.occupy(region, faction, by, events);
        let preview = self.capture_preview(region, faction, by, surrender);
        if skip_choice {
            self.resolve_capture(preview, CaptureChoice::Occupy, events);
        } else if self.turn.humans.contains(&faction) {
            // PROVISIONAL: the exe also occupies at once for a human when a campaign byte is set (model
            // +0xFA8 → +0x114, read at `0x008F8D22`, tested at `0x008F8DBF`); what sets it is not traced.
            // A capture still waiting is settled as an occupation first (PROVISIONAL: the original's screen is modal).
            if let Some(old) = self.pending_capture.take() {
                self.resolve_capture(old, CaptureChoice::Occupy, events);
            }
            events.push(CampaignEvent::CaptureChoicePending { region, faction });
            self.pending_capture = Some(preview);
        } else {
            let rules = std::sync::Arc::clone(&self.rules);
            let mut writes = super::mod_state::ModWrites::default();
            let choice = rules.seams.capture_ai_choice.rule()(self, &preview, previous, &mut writes);
            self.mod_state.apply(&mut writes);
            self.resolve_capture(preview, choice, events);
        }
    }

    /// The capture report's two skip flags (+0x9A, +0x9B): either one makes the report occupy at once, for
    /// an AI and a human capturer alike (CONFIRMED by disassembly: `0x008F8DCA` / `0x008F8DD7` for a human,
    /// `0x008F8E70` / `0x008F8E79` before the AI's choice `0x00AAABA0`, both jumping to the occupy at
    /// `0x008F8EB3`). The settle step computes them for the capturing force and passes them to the report's
    /// constructor (`0x00B586AF`..`0x00B58739`), both false without a force:
    /// - +0x9A (`0x00B4DA40`): the force has a `REBEL_DATA` block, its +0x34 is set
    ///   ([`RebelArmySide::link`](super::RebelArmySide::link)) and the region it rose in (block +0x68,
    ///   [`World::region_risen_armies`](super::World::region_risen_armies)) holds the captured settlement;
    /// - +0x9B (`0x00B4D9E0`): the force has a `REBEL_DATA` block whose faction (block +0x10) the campaign's
    ///   faction registry finds (model +0xF5C, `0x00955B30`; [`Self::registry_faction`]) and that faction is
    ///   still in the game (+0x824 clear, [`Self::in_the_game`]).
    fn capture_skips_choice(&self, region: RegionId, by: Option<ForceId>) -> bool {
        let Some((force, side)) = by.and_then(|f| Some((f, self.world.rebel_army_sides.get(&f)?))) else {
            return false;
        };
        let rose_here = || self.world.region_risen_armies.get(&region).is_some_and(|armies| armies.contains(&force));
        let restores = side.link != 0 && rose_here();
        let rises_for_live_faction = self.registry_faction(&side.faction).is_some_and(|f| self.in_the_game(f.id));
        restores || rises_for_live_faction
    }

    /// A capture by force without the battle: `faction` takes the settlement as after a won assault or an
    /// undefended settlement (`settle_capture`: a human's choice waits in [`CampaignModel::pending_capture`]).
    /// For harnesses and tests that stage a capture; the game's own path goes through the battle or
    /// `AttackSettlement`.
    pub fn capture_by_force(&mut self, region: RegionId, faction: FactionId, by: Option<ForceId>) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
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
                // The loot's town wealth is set through `0x00AAA580` (`0x00B5432D`), which then refreshes the
                // region's town wealth without apply (`0x00AB4410(0)`, CONFIRMED). The occupation's path sets no
                // town wealth through it and runs no refresh.
                self.refresh_region_town_wealth(p.region);
                self.loot_resentment(p.region, p.faction);
                // 0x00B541E0 posts CharacterLootedSettlement (at 0x00B54468) after the option and the
                // resentment are applied, with the report's commander (CONFIRMED). It posts even with no
                // commander, but the poster 0x00E30700 hands an event to Lua only when its vtable +0xB8
                // (0x008F9EE0 for this event) passes, and that returns 0 when the character getter (+0x1C,
                // 0x0044A500) is null: no script ever sees a looter-less loot, so ours raises none (CONFIRMED).
                if let Some(character) = p.looter {
                    events.push(CampaignEvent::CharacterLootedSettlement { character });
                }
                p.loot.money
            }
            CaptureChoice::Occupy => {
                self.apply_outcome(p.region, p.faction, &p.occupy, false);
                0
            }
            CaptureChoice::Liberate => {
                let target = p.liberate.expect("checked");
                // 0x00B4F090 first posts FactionBecomesLiberationProtectorate (0x00B4F0FE / 0x00B5089D)
                // with the region's owner at that moment, the liberator (CONFIRMED), then puts the
                // liberator's armies out of the region (0x008D9DC0 at 0x00B508C5, unconditional,
                // CONFIRMED), then calls 0x00B58A10(target, 0, 1): the liberated faction comes back if
                // it was out of the game, then the region passes to it through the shared owner change
                // (`liberate_region` → `transfer_region`), which expels the other factions' armies
                // again (0x00A64AC0 with its fourth argument 0 → 0x008D9DC0,
                // `expel_after_owner_change`; only factions with a capital, so the first call is what
                // puts a capturer without one out of the settlement). The army the original raises
                // for the target (0x00B4F090: units chosen from what the region's sArmy / rHorse /
                // tGuns buildings allow, a new general) is not modelled. Liberation posts no
                // SettlementOccupied (its constructor 0x00889EE0 is called only by the three capture
                // variants, and 0x00B58A10 only forwards to the owner change 0x00B449F0, CONFIRMED).
                // `liberate_region` refuses only a missing region or one `target` already owns, and
                // then changes nothing.
                if let Some(liberator) = self.world.regions.get(&p.region).map(|r| r.owner) {
                    events.push(CampaignEvent::FactionBecomesLiberationProtectorate { faction: liberator });
                    self.expel_from_region(liberator, p.region, events);
                }
                self.liberate_region(p.region, target, events);
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
        r.building_at(slot).is_some_and(|b| b.is_damaged())
            && held
            && !r.construction.iter().any(|c| c.slot == slot)
    }

    /// Command `RepairBuilding` (`0x00B66260`): charges the repair cost when it is above 0 and queues the
    /// repair, with no affordability test (its callers make it: the panel's `can_afford_repair` and the AI,
    /// [`super::treasury::can_pay_repair`]). The repair is a construction item
    /// ([`ConstructionKind::Repair`], ESF type 2) that raises the building's health each turn and ends at 100
    /// ([`super::turn::construction_step`]); its length is max(1, floor((100 − health) × 0.01 × level turns)).
    pub(crate) fn repair_building(&mut self, region: RegionId, slot: SlotRef) -> Result<Vec<CampaignEvent>, CommandError> {
        let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
        self.check_turn(owner)?;
        self.queue_repair(region, slot)
    }

    /// How many turns repairing the building in `slot` takes: max(1, floor((100 − health) × 0.01 ×
    /// level turns)) (0 when nothing stands there), from the building's health now (`0x00AE7010`; a queued repair
    /// keeps its own length, [`ConstructionKind::Repair`]).
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
        // The step per turn (`0x00AE7010` → item +0x18, CONFIRMED): what the building lacks, divided by the turns
        // (unsigned, the remainder dropped); the last turn sets health 100 (`0x00B78490`).
        let b = self.world.regions[&region].building_at(slot).cloned().expect("checked");
        let f = self.world.factions.get_mut(&owner).ok_or(CommandError::UnknownFaction(owner))?;
        // `0x00B66260` tests no funds and charges only a cost above 0 (signed); the item keeps the cost either way
        // (`0x00AE7010` → item +0x14), and a cancel credits that stored cost back ([`super::treasury::credit`]).
        if cost > 0 {
            super::treasury::pay(&mut f.treasury, cost);
        }
        self.world.regions.get_mut(&region).expect("checked").construction.push(ConstructionItem {
            slot,
            level_key: b.level_key,
            turns_remaining: turns,
            cost,
            kind: ConstructionKind::Repair { health_per_turn: (100u32.saturating_sub(b.health)) / turns.max(1), turns },
            active: true,
        });
        Ok(Vec::new())
    }

    /// Turns the repairs of a save written before items kept their [`ConstructionKind`] into repair items,
    /// and returns how many it converted. Such saves (our own format before version 4, and our ESF writer
    /// before it wrote item types 2 and 3) stored a repair as a build of the standing building's own level,
    /// and the building kept the health it had when the repair was queued (the step left it alone). So the
    /// length and step are those [`Self::queue_repair`] gives from that health, and the health gets the
    /// steps already run (`0x00B78490`). An original save never holds a build of the standing level (an
    /// upgrade names the next level), so this changes nothing in one. Needs the rules (the level's turns):
    /// run it after they are attached.
    pub fn convert_legacy_repairs(&mut self) -> usize {
        let mut found = Vec::new();
        for (&region, reg) in &self.world.regions {
            for (k, item) in reg.construction.iter().enumerate() {
                if item.kind == ConstructionKind::Build && reg.building_at(item.slot).is_some_and(|b| b.level_key == item.level_key) {
                    found.push((region, k));
                }
            }
        }
        for &(region, k) in &found {
            let slot = self.world.regions[&region].construction[k].slot;
            let turns = self.repair_turns(region, slot);
            let reg = self.world.regions.get_mut(&region).expect("found above");
            let remaining = reg.construction[k].turns_remaining;
            let stepped = turns.saturating_sub(remaining);
            let turns = turns.max(remaining);
            if let Some(Some(b)) = reg.building_mut(slot) {
                let health_per_turn = 100u32.saturating_sub(b.health) / turns;
                b.health = b.health.saturating_add(health_per_turn.saturating_mul(stepped)).min(100);
                reg.construction[k].kind = ConstructionKind::Repair { health_per_turn, turns };
            }
        }
        found.len()
    }

    /// Whether the building in `slot` can be demolished now: not the road, held by the owner, and a building
    /// standing (`0x00B16540`, CONFIRMED; the panel's `CanDemolishBuilding` `0x009B7920` tests the same: slot
    /// resolve `0x009BB6E0` shared with cancel/repair, not the `settlement_road` slot, `0x00A91FC0`). An item
    /// queued in the slot does not block it: the demolition cancels it ([`Self::demolish_building`]).
    pub fn can_demolish(&self, region: RegionId, slot: SlotRef) -> bool {
        if slot == SlotRef::Road {
            return false;
        }
        let Some(r) = self.world.regions.get(&region) else { return false };
        let held = r.slot_held(slot);
        r.building_at(slot).is_some() && held
    }

    /// Command `DemolishBuilding` (exe `DemolishBuilding` `0x009E2380` → `0x009B9590` → queue id
    /// `0x84`; fort `DemolishFort` `0x009E23D0` → `0x009BA250` → queue id `0x89`; both CONFIRMED):
    /// the handler (`QueueSlotDemolition` `0x00B286C0`, CONFIRMED) cancels any item queued in the slot, crediting
    /// its stored cost back as [`CampaignCommand::CancelConstruction`] does, then, when the building can be
    /// demolished (`0x00B16540`), queues a demolition: a one-turn item that costs nothing
    /// ([`ConstructionKind::Demolish`], `0x00AE6ED0`, ESF type 3). It empties the slot at the round end
    /// ([`super::turn::construction_step`]). Fires no script event.
    pub(crate) fn demolish_building(&mut self, region: RegionId, slot: SlotRef) -> Result<Vec<CampaignEvent>, CommandError> {
        let owner = self.world.regions.get(&region).ok_or(CommandError::UnknownRegion(region))?.owner;
        self.check_turn(owner)?;
        if !self.can_demolish(region, slot) {
            return Err(CommandError::CannotBuild("nothing to demolish in that slot".into()));
        }
        // The exe cancels before it tests; the test does not depend on the item, so testing first (no change on
        // a refused command) gives the same result for every command its panel can send.
        self.cancel_slot_item(region, slot);
        self.world.regions.get_mut(&region).expect("checked").construction.push(ConstructionItem { slot, level_key: String::new(), turns_remaining: 1, cost: 0, kind: ConstructionKind::Demolish, active: true });
        Ok(Vec::new())
    }

    /// The AI's repairs (PROVISIONAL stand-in for the AI's call of `0x00AA4910`, which repairs a building when
    /// it can be repaired (`0x00B1A6B0`) and paid for (`0x00B16430`: [`super::treasury::can_pay_repair`] on the
    /// repair cost; an AI faction's cost is capped at its treasury, so the test always passes and is not repeated
    /// here, CONFIRMED)): at an AI faction's turn start, every damaged building, the settlement walls included (an
    /// ordinary building slot in the exe, CONFIRMED). This is what lets a damaged AI building be upgraded again:
    /// upgrades need full health ([`Self::can_build`]). No repair where the AI may not shop
    /// ([`Self::ai_shopping_hidden`]; the gate is CONFIRMED at `0x00AA491D..0x00AA4945`: when the CAI slot's
    /// `+0x104` object exists, its position is tested with `0x00A8B2D0` and a hidden one is skipped). The
    /// position is INFERRED: the slot's own position ([`super::world::RegionSlot::position`], none = no gate,
    /// as the exe's null object), the settlement's for the walls; what the CAI slot's `+0x104` object is
    /// is not traced.
    pub(crate) fn ai_repairs(&mut self, faction: FactionId) {
        if self.turn.humans.contains(&faction) {
            return;
        }
        // In region-id order, part of the stand-in: the exe repairs from the CAI's slot intentions (`0x00CE9A60`: repair
        // `0x00AA4910` when the slot's building handle is the intention's +0x110, else build `0x00A67890`), in the
        // intention order, not traced. The order matters: each repair's cost is capped at the treasury left.
        let regions: Vec<RegionId> = self.world.regions.values().filter(|r| r.owner == faction).map(|r| r.id).collect();
        for region in regions {
            let n = self.world.regions[&region].slots.len();
            for slot in (0..n).map(SlotRef::Slot).chain([SlotRef::Walls]) {
                let r = &self.world.regions[&region];
                let position = match slot {
                    SlotRef::Slot(i) => r.slots[i].position,
                    _ => Some(r.settlement.position),
                };
                if position.is_some_and(|p| self.ai_shopping_hidden(p)) {
                    continue;
                }
                if self.can_repair(region, slot) {
                    let _ = self.queue_repair(region, slot);
                }
            }
        }
    }
}
