//! The AI's choice after it takes a settlement by force: loot, occupy or liberate (the
//! [`CAPTURE_AI_CHOICE`](super::seams::CAPTURE_AI_CHOICE) seam; the original's is [`capture_choice_original`]).
//!
//! The capture report's step (`0x008F8E70`) asks `ResolveAiSettlementCaptureOption` (`0x00AAABA0`) with the
//! settlement, the capturer, the settlement's owner before the capture and the capture preview
//! ([`CapturePreview`]); it answers occupy when any of the AI objects is missing, else calls the capturer's
//! attitude object's virtual `+0x2F8`, `0x00D8C8B0` (CONFIRMED, every step below):
//! 1. **Always liberate**: the capturer is the campaign's liberator (`spa_portugal`) and the preview offers to
//!    liberate the target (`spa_spain`) → liberate.
//! 2. **Contested liberation**: the capturer is the contested liberator (`spa_britain`), the preview offers the
//!    target, the liberator and the target are factions of the campaign, the settlement's region R borders a
//!    region of the rival (`spa_france`), and another region of the capturer borders the rival too:
//!    a. the target out of the game → liberate;
//!    b. S = the target's region count, B = the capturer's; across each boundary of R, a region of the
//!    capturer adds 1 to S, else one of the target adds 1 to B (as the exe writes it);
//!    c. B ≤ 1 and the target has no army with units in R → step 3;
//!    d. r = a CAI draw in 1..=S + B; r ≤ B → liberate, else step 3.
//! 3. **Rebels**: the capturer is the rebels → loot chance p = 0.7 (`0x01325CDC`); else the previous owner is
//!    the rebels → p = 0.2 (`0x01325CA8`); either goes straight to step 6.
//! 4. **Personality** (the capturer's, [`TUNABLES`], getters `0x00CB0860`..`0x00CB0850` at vtable
//!    `0x0138AE88`..`0x0138AEA0`, fields +0x3B4..+0x3CC set by name in `0x00DE31E0` from the table's percent,
//!    then × 0.01 in the post-load pass `0x00DDFFB0`, `PERCENT`): p = base; the running
//!    economy record's spending (categories 13..=24, `0x00BC7840`) × the expenditure pct above its income
//!    (0..=12, `0x00BC7860`), in f32 → p = economic + base; p += |min(attitude total towards the previous
//!    owner, 0) / 20| × diplomatic (the division truncates); no boundary of R reaches a region of the capturer
//!    → p += region grouping.
//! 5. **Clamp**: p ≤ 0 (or NaN) → 0, p ≥ 1 → 1; p ≤ the never-loot floor → occupy; p ≥ the always-loot
//!    ceiling → loot.
//! 6. **Draw**: r = a CAI float in [0, 1]; p < r → occupy, else loot.
//!
//! The CAI draws (`0x00A9D9C0` / `0x00A9DA10`) are [`crate::rng::CaRng::int_range`] /
//! [`crate::rng::CaRng::float_range`] on the CAI's own stream, [`CampaignModel::cai_rng`] (CAI interface
//! +0x71C: `CAI_INTERFACE` #0 of the file, reseeded from the campaign RNG when a new campaign starts).

use super::capture::{CaptureChoice, CapturePreview};
use super::ids::{FactionId, RegionId};
use super::world::{economy_sum, CampaignModel};

/// The personality tunables the choice reads, in the exe's order (vtable `+0x358`..`+0x370`).
pub const TUNABLES: [&str; 7] = [
    BASE_CHANCE,
    EXPENDITURE_PCT,
    ECONOMIC_MODIFIER,
    DIPLOMATIC_MODIFIER,
    REGION_GROUPING_MODIFIER,
    NEVER_LOOT_FLOOR,
    ALWAYS_LOOT_CEILING,
];
const BASE_CHANCE: &str = "LOOT_SETTLEMENT_BASE_CHANCE";
const EXPENDITURE_PCT: &str = "LOOT_SETTLEMENT_EXPENDITURE_PCT";
const ECONOMIC_MODIFIER: &str = "LOOT_SETTLEMENT_ECONOMIC_MODIFIER";
const DIPLOMATIC_MODIFIER: &str = "LOOT_SETTLEMENT_DIPLOMATIC_MODIFIER";
const REGION_GROUPING_MODIFIER: &str = "LOOT_SETTLEMENT_REGION_GROUPING_MODIFIER";
const NEVER_LOOT_FLOOR: &str = "LOOT_SETTLEMENT_NEVER_LOOT_FLOOR";
const ALWAYS_LOOT_CEILING: &str = "LOOT_SETTLEMENT_ALWAYS_LOOT_CEILING";

/// `0x01325CDC`: a rebel capturer's loot chance.
const REBEL_CAPTURER_CHANCE: f32 = 0.7;
/// `0x01325CA8`: the loot chance after taking a settlement from the rebels.
const FROM_REBELS_CHANCE: f32 = 0.2;
/// `0x00DDFFB0` multiplies each of the seven tunables by this f32 (`0x0139E870`) once the table is loaded.
const PERCENT: f32 = 0.01;
/// The attitude total's divisor (`0x14`).
const ATTITUDE_DIVISOR: i32 = 20;

/// The original's choice (`0x00AAABA0` → `0x00D8C8B0`, see the module docs). `previous` is the settlement's
/// owner before the capture (report +0x10); none → occupy, as `0x00AAABA0` answers for a missing AI object.
pub fn capture_choice_original(model: &mut CampaignModel, preview: &CapturePreview, previous: Option<FactionId>) -> CaptureChoice {
    let Some(previous) = previous else { return CaptureChoice::Occupy };
    let own = preview.faction;
    if liberates(model, preview) {
        return CaptureChoice::Liberate;
    }
    let p = if model.is_rebel_faction(own) {
        REBEL_CAPTURER_CHANCE
    } else if model.is_rebel_faction(previous) {
        FROM_REBELS_CHANCE
    } else {
        match personality_chance(model, preview.region, own, previous) {
            Ok(p) => p,
            Err(choice) => return choice,
        }
    };
    let r = model.cai_rng.float_range(0.0, 1.0);
    if p < r { CaptureChoice::Occupy } else { CaptureChoice::Loot }
}

/// Steps 1 and 2: whether the capturer liberates (step 2's draw included).
fn liberates(model: &mut CampaignModel, preview: &CapturePreview) -> bool {
    let Some(rule) = model.rules.features.ai_capture_liberation.clone() else { return false };
    let key_of = |m: &CampaignModel, f: FactionId| m.world.factions.get(&f).map(|x| x.key.clone()).unwrap_or_default();
    let own_key = key_of(model, preview.faction);
    let Some(target) = preview.liberate.filter(|&t| key_of(model, t) == rule.target) else { return false };
    if own_key == rule.liberator {
        return true;
    }
    if own_key != rule.contested_liberator {
        return false;
    }
    // The CAI factions by key (`0x00C3C480`, none when absent); the capturer is the contested liberator.
    let by_key = |k: &str| model.world.factions.values().find(|f| f.key == k).map(|f| f.id);
    let (own, rival) = (preview.faction, by_key(&rule.rival));
    let region = preview.region;
    let areas = super::director::areas(model);
    let borders = |r: RegionId, f: Option<FactionId>| boundary_owners(&areas, r).any(|o| o.is_some() && o == f);
    if !borders(region, rival) {
        return false;
    }
    // Only whether such a region exists matters. The CAI faction's region list (`0x00A676E0`, +0x114) as
    // `owner == f`, as the director reads it.
    let other = model.world.regions.values().any(|r| r.owner == own && r.id != region && borders(r.id, rival));
    if !other {
        return false;
    }
    if !model.in_the_game(target) {
        return true;
    }
    let count = |f: FactionId| model.world.regions.values().filter(|r| r.owner == f).count() as i32;
    let (mut s, mut b) = (count(target), count(own));
    for o in boundary_owners(&areas, region) {
        if o == Some(own) {
            s += 1;
        } else if o == Some(target) {
            b += 1;
        }
    }
    if b <= 1 && !army_with_units_in(model, region, target) {
        return false;
    }
    drop(areas);
    model.cai_rng.int_range(1, s + b) <= b
}

/// The owners of the areas across each boundary of the region, in boundary order (`CAI_REGION` +0x170/+0x174,
/// the other side `0x00C1BCE0`, its owner vtable `+0xC`); a sea or theatre area has none.
fn boundary_owners<'a>(areas: &'a [super::director::Area<'a>], region: RegionId) -> impl Iterator<Item = Option<FactionId>> + 'a {
    let links = areas.iter().find(|a| a.region.is_some_and(|r| r.id == region)).map_or(&[][..], |a| a.links.as_slice());
    links.iter().map(|&i| areas.get(i).and_then(|a| a.region).map(|r| r.owner))
}

/// `CheckCaiRegionHasFactionArmyInside` (`0x00C3DEE0`): one of the faction's forces with units stands in the
/// region (the CAI region's situated mobiles, [`super::economy::in_region`] at the force's commander).
fn army_with_units_in(model: &CampaignModel, region: RegionId, faction: FactionId) -> bool {
    let Some(reg) = model.world.regions.get(&region) else { return false };
    model.world.forces.values().any(|f| {
        f.faction == faction
            && !f.units.is_empty()
            && f.commander.and_then(|c| model.world.characters.get(&c)).is_some_and(|c| super::economy::in_region(model, reg, c))
    })
}

/// Steps 4 and 5: the personality's loot chance, or the choice the floor or the ceiling forces.
fn personality_chance(model: &CampaignModel, region: RegionId, own: FactionId, previous: FactionId) -> Result<f32, CaptureChoice> {
    let rules = &model.rules;
    let faction_key = model.world.factions.get(&own).map_or("", |f| f.key.as_str());
    let stored = model.world.ai_keys.get(&own).map(|k| k.personality.as_str());
    let personality = rules.ai_personalities.resolve(&rules.campaign, faction_key, stored);
    // The table holds percent (scaled once at load by `0x00DDFFB0`). A tunable the personality and the
    // default lack reads 0 (logged once when the rules load).
    let t = |k: &str| rules.ai_personalities.tunable(&personality, k).unwrap_or(0.0) * PERCENT;
    let base = t(BASE_CHANCE);
    let mut p = base;
    // The running economy record (`0x00BABE00`); a model faction without history has both sums 0.
    let (spent, income) = model.world.last_economy_record(own).map_or((0, 0), |r| (economy_sum(r, 13..25), economy_sum(r, 0..13)));
    if spent as f32 * t(EXPENDITURE_PCT) > income as f32 {
        p = t(ECONOMIC_MODIFIER) + base;
    }
    let attitude = model.world.relationships.get(&(own, previous)).map_or(0, |r| r.attitude_total());
    let q = attitude.min(0) / ATTITUDE_DIVISOR;
    p += q.unsigned_abs() as f32 * t(DIPLOMATIC_MODIFIER);
    let areas = super::director::areas(model);
    if !boundary_owners(&areas, region).any(|o| o == Some(own)) {
        p += t(REGION_GROUPING_MODIFIER);
    }
    // `!(p > 0)` and `!(1 > p)`: a NaN clamps to 0.
    if p.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        p = 0.0;
    } else if p.partial_cmp(&1.0) != Some(std::cmp::Ordering::Less) {
        p = 1.0;
    }
    if t(NEVER_LOOT_FLOOR) >= p {
        return Err(CaptureChoice::Occupy);
    }
    if p >= t(ALWAYS_LOOT_CEILING) {
        return Err(CaptureChoice::Loot);
    }
    Ok(p)
}
