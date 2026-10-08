//! Shared test helpers: a tiny made-up campaign model (NOT game data).

use std::collections::BTreeMap;

use ntw_sim::calendar::{Calendar, Date};
use ntw_sim::campaign::{CampaignModel, Faction, FactionId, GovernmentType, World};
use ntw_sim::rng::CaRng;

/// A made-up model with a few eur_napoleon faction KEYS (names only) and made-up numbers.
pub fn tiny_model() -> CampaignModel {
    let mut world = World::default();
    for (i, key) in ["france", "austria", "prussia", "russia", "portugal", "britain"].iter().enumerate() {
        let id = FactionId(100 + i as i32);
        world.factions.insert(
            id,
            Faction {
                id,
                key: key.to_string(),
                treasury: 1000, // made up
                government: GovernmentType::AbsoluteMonarchy,
                government_key: String::new(),
                tax_lower: "tax_normal".into(),
                tax_upper: "tax_normal".into(),
                diplomacy: BTreeMap::new(),
            },
        );
    }
    // Made-up start date (Early September 1805, as in W3 §3.1's example).
    let date = Date { year: 1805, season: 0, month: 8, half: 0 };
    CampaignModel::new(Calendar::new(date, 0), CaRng::new(1), world)
}
