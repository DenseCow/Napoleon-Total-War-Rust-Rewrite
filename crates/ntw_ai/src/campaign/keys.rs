//! The campaign AI's data stored in a startpos or save that the model does not hold.
//!
//! The factions' manager / personality keys and the region base values are in the model
//! (`ntw_sim::campaign::World::ai_keys`, `World::region_base_values`), filled by the campaign
//! source (`ntw_campaign::ai_keys` for the original's files); [`FactionAiKeys`] is re-exported
//! from `ntw_sim`.

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfNode, EsfRecord};

pub use ntw_sim::campaign::FactionAiKeys;

/// A faction's stored difficulty block (faction `+0x6E0..+0x6F4`, round 6): the record
/// `FACTION/CAMPAIGN_PLAYER_SETUP` holds `{CAMPAIGN_VICTORY_CONDITIONS}`,
/// `{CAMPAIGN_PLAYER_SETUP_INGAME_MODIFIABLES i32, i32, i32, bool}` (the 16-byte block `+0x6E4..`
/// that `0x008DD090` copies from the player slot), the faction key and three flags; the first flag
/// is true only for the human's faction in the user's saves (INFERRED `+0x6E0`, is human).
/// INFERRED meanings of the block: campaign difficulty, battle difficulty, a third value (UNKNOWN,
/// autoresolve?) and a flag; every faction stores its own, so a save played on hard would show
/// whether AI factions get the player's value or its negation (all shipped and user saves are
/// on normal: 0 everywhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FactionDifficulty {
    pub campaign: i32,
    pub battle: i32,
    pub third: i32,
    pub flag: bool,
    /// The first flag after the faction key (INFERRED is human).
    pub human: bool,
}

/// Every faction's [`FactionDifficulty`] in a startpos or save, by faction key.
pub fn read_difficulties(root: &EsfRecord) -> BTreeMap<String, FactionDifficulty> {
    let mut out = BTreeMap::new();
    let Some(arr) = root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") else { return out };
    for rec in arr.records().filter(|r| r.name == "FACTION") {
        let Some(setup) = rec.child("CAMPAIGN_PLAYER_SETUP") else { continue };
        let Some(m) = setup.child("CAMPAIGN_PLAYER_SETUP_INGAME_MODIFIABLES") else { continue };
        let ints: Vec<i32> = m.values().filter_map(EsfNode::as_i32).collect();
        let flags: Vec<bool> = m.values().filter_map(EsfNode::as_bool).collect();
        let vals: Vec<&EsfNode> = setup.values().collect();
        let Some(key) = vals.iter().find_map(|v| v.as_str()) else { continue };
        let human = vals.iter().filter_map(|v| v.as_bool()).next().unwrap_or(false);
        if ints.len() >= 3 {
            out.insert(
                key.to_string(),
                FactionDifficulty { campaign: ints[0], battle: ints[1], third: ints[2], flag: flags.first().copied().unwrap_or(false), human },
            );
        }
    }
    out
}

