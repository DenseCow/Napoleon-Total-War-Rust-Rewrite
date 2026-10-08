//! Region religion: the per-round conversion of the religion breakdown (`0x00A63FE0`, CONFIRMED from the
//! exe; CAMPAIGN_FIDELITY.md §Religion).
//!
//! At the round end each region updates its population (`0x00AB42F0` → `0x00AB3FF0` → `0x00AB4070`: growth,
//! then conversion, then the town wealth) from the faction's round-end economy `0x008BC650`. Conversion:
//! 1. **Strength per religion** (`0x00A61E90` accumulator): each missionary standing in the region adds his rank
//!    (`0x00A198D0`) to his religion (character type check `0x00F9C710`, religion at the agent +0x1A0), and every
//!    religion of the breakdown adds the region's `conversion` effect for it (`0x00E1EF70(religion, 1)`: the
//!    religion-keyed bonus, `effect_bonus_value_religion_junction`; in `spa_napoleon` the "religions" are the
//!    alignments `align_pro_french` / `align_anti_french`, raised by universities and churches).
//! 2. **Flows**: for each religion i of the breakdown with s = clamp(strength(i), 0, 9) > 0, and each other religion
//!    j with d = s − strength(j) > 0: pool = population × share(j); x = clamp(`points_base` + `zeal_mult` × d² +
//!    mod(j, i), 1, 10) with mod = `religion_conversion_mods` (row j, column i); amount = min((`pop_added` +
//!    `pop_mult` × pool) × `points_mult` × x, pool); i's people += amount, j's −= amount (integer people, each step
//!    truncated as the exe does through float).
//! 3. **Shares**: share(k) += people(k) / population, floored at 0 (`0x00AA4860` follows; not read).
//!
//! Tweaks (built-in defaults, `conversion_constants_*`, CONFIRMED values): points_base 1.9, zeal_mult 0.05,
//! points_mult 2.4, pop_mult 0.004, pop_added 200.

use super::effects::BonusKind;
use super::ids::{CharacterId, FactionId, RegionId};
use super::world::{CampaignModel, CharacterKind};

/// `conversion_constants_conversion_points_base` (CONFIRMED default).
pub const POINTS_BASE: f32 = 1.9;
/// `conversion_constants_conversion_points_zeal_mult`.
pub const ZEAL_MULT: f32 = 0.05;
/// `conversion_constants_conversion_points_mult`.
pub const POINTS_MULT: f32 = 2.4;
/// `conversion_constants_pop_mult`.
pub const POP_MULT: f32 = 0.004;
/// `conversion_constants_pop_added`.
pub const POP_ADDED: f32 = 200.0;

/// The religion a missionary converts to: the agent record +0x1A0 = `agents` #9 (CONFIRMED; spa: catholic and
/// Protestant missionaries `align_anti_french`, orthodox `align_pro_french`). Only the missionary types (agent
/// +0x2C 6..10, `0x00F9C710`) count.
fn missionary_religion(model: &CampaignModel, kind: CharacterKind) -> Option<String> {
    let missionary = matches!(kind, CharacterKind::CatholicMissionary | CharacterKind::OrthodoxMissionary | CharacterKind::ProtestantMissionary)
        || kind.esf_name().to_ascii_lowercase().contains("missionary");
    if !missionary {
        return None;
    }
    model.rules.agent_religions.get(kind.esf_name()).cloned()
}

/// A missionary's rank as the conversion step reads it (`0x00A198D0`, CONFIRMED): his raw `zeal` level plus his
/// theatre's zeal bonus (`zeal_europe`, bonus 0x68, which the trait effects `zeal_spain` map to), clamped to −1..9;
/// in the `spain_main` theatre also his faction's `zeal_europe`. CONFIRMED on the vanilla spa round
/// `nr16_spa_t0` → `nr16_spa_t2` with the debugger log: a "Radical" provocateur (+2) converts with strength 4, a
/// "Politics" priest (+1) with 3, a missionary without traits with 2.
pub fn missionary_rank(model: &CampaignModel, c: CharacterId) -> i32 {
    let Some(ch) = model.world.characters.get(&c) else { return -1 };
    let level = model.world.character_details.get(&c).and_then(|d| d.attributes.iter().find(|(k, _)| k == "zeal").map(|(_, v)| *v)).unwrap_or(-1);
    let mut bonus = super::effects::Effects::character_effects(model, c).get_int("zeal_europe");
    if model.rules.campaign == "spa_napoleon" {
        bonus += super::effects::Effects::faction_sum(model, ch.faction).get_int("zeal_europe");
    }
    (level + bonus).clamp(-1, 9)
}

/// One conversion flow: `amount` people of `region` from religion `from` to `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    /// The religion losing people.
    pub from: String,
    /// The religion gaining them.
    pub to: String,
    /// People moved.
    pub amount: f32,
}

impl CampaignModel {
    /// The conversion strength of each religion in `region` (step 1 of the module docs), in the order the
    /// accumulator is built: the missionaries first, then the breakdown's religions with a non-zero effect.
    pub fn religion_strengths(&self, region: RegionId) -> Vec<(String, f32)> {
        let mut acc: Vec<(String, f32)> = Vec::new();
        let add = |acc: &mut Vec<(String, f32)>, r: &str, v: f32| match acc.iter_mut().find(|(k, _)| k == r) {
            Some(e) => e.1 += v,
            None => acc.push((r.to_string(), v)),
        };
        let Some(reg) = self.world.regions.get(&region) else { return acc };
        let mut missionaries: Vec<(CharacterId, String)> = self
            .world
            .characters
            .values()
            .filter(|c| super::economy::in_region(self, reg, c))
            .filter_map(|c| Some((c.id, missionary_religion(self, c.kind)?)))
            .collect();
        missionaries.sort_by_key(|(c, _)| *c);
        for (c, r) in missionaries {
            let rank = missionary_rank(self, c);
            add(&mut acc, &r, rank as f32);
        }
        let set = super::economy::region_effect_set(self, reg);
        for (r, _) in &reg.religions {
            let v = set.get_qualified(BonusKind::Religion, "conversion", r);
            if v != 0.0 {
                add(&mut acc, r, v);
            }
        }
        acc
    }

    /// The conversion flows of `region` this round (steps 1–2), without changing anything.
    pub fn conversion_flows(&self, region: RegionId) -> Vec<Flow> {
        let Some(reg) = self.world.regions.get(&region) else { return Vec::new() };
        let acc = self.religion_strengths(region);
        let strength = |r: &str| acc.iter().find(|(k, _)| k == r).map_or(0.0, |x| x.1);
        let pop = reg.population as f32;
        let mut out = Vec::new();
        for (to, _) in &reg.religions {
            let s = strength(to).clamp(0.0, 9.0);
            if s <= 0.0 {
                continue;
            }
            for (from, share) in &reg.religions {
                if from == to {
                    continue;
                }
                let d = s - strength(from);
                if d <= 0.0 {
                    continue;
                }
                let modifier = self.rules.conversion_mods.get(&(from.clone(), to.clone())).copied().unwrap_or(0.0);
                let pool = pop * share;
                let x = (POINTS_BASE + ZEAL_MULT * (d * d) + modifier).clamp(1.0, 10.0);
                let amount = ((POP_ADDED + POP_MULT * pool) * (POINTS_MULT * x)).min(pool);
                out.push(Flow { from: from.clone(), to: to.clone(), amount });
            }
        }
        out
    }

    /// Step 3 for one region: applies the flows to the breakdown (people as truncated integers, shares
    /// re-derived from them).
    pub fn convert_region(&mut self, region: RegionId) {
        let flows = self.conversion_flows(region);
        let Some(reg) = self.world.regions.get_mut(&region) else { return };
        if flows.is_empty() || reg.population == 0 {
            return;
        }
        let mut people: Vec<i32> = vec![0; reg.religions.len()];
        let index = |r: &str, reg: &super::world::Region| reg.religions.iter().position(|(k, _)| k == r);
        for f in flows {
            let (Some(i), Some(j)) = (index(&f.to, reg), index(&f.from, reg)) else { continue };
            people[i] = (people[i] as f32 + f.amount) as i32;
            people[j] = (people[j] as f32 - f.amount) as i32;
        }
        let pop = reg.population as f32;
        for ((_, share), p) in reg.religions.iter_mut().zip(people) {
            *share = (*share + p as f32 / pop).max(0.0);
        }
    }

    /// The round-end conversion of every region `faction` owns (from its round-end economy, CONFIRMED place).
    pub fn religion_round_end(&mut self, faction: FactionId) {
        let regions: Vec<RegionId> = self.world.regions.values().filter(|r| r.owner == faction).map(|r| r.id).collect();
        for r in regions {
            self.convert_region(r);
        }
    }
}
