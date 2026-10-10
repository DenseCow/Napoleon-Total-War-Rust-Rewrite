//! # The campaign model
//!
//! The deterministic campaign-map simulation: factions, regions, characters, armies, diplomacy,
//! commands, the turn loop, the economy, movement and battles. Plain Rust, std only, no Bevy and
//! no I/O (DESIGN §1, §3.3).
//!
//! How it fits together:
//! - [`CampaignModel`] is the whole state (the original's `CAMPAIGN_MODEL`, W3 §3): a
//!   [`Calendar`](crate::calendar::Calendar), the campaign [`CaRng`](crate::rng::CaRng), the
//!   [`World`] and the [`TurnState`]. It also carries game data that is not state: the
//!   [`CampaignRules`] (DB tables) and the map's movement grid ([`pathing`]).
//! - The display (or the AI, or a network peer) never edits the model directly. It sends a
//!   [`CampaignCommand`] (the original's `CAMPAIGN_COMMAND_QUEUE` / `CCQ_*`, CONFIRMED) through
//!   [`CampaignModel::apply`], which checks it and either changes the state or returns a
//!   [`CommandError`] without changing anything.
//! - Every change reports [`CampaignEvent`]s, named after the original's script events, so that
//!   the Lua layer can forward them.
//! - The turn loop ([`turn`]) runs factions in the start position's order with a **PROVISIONAL**
//!   phase order (the real one is UNKNOWN, W1 §10.4), one step at a time so scripts can react
//!   between phases.
//!
//! Evidence tags, as elsewhere in this crate: CONFIRMED / INFERRED / UNKNOWN / PLACEHOLDER /
//! PROVISIONAL. Structure, names and DB values are the original's; the formulas that combine them
//! (economy, movement costs, autoresolve potentials, ...) are marked PROVISIONAL where they are
//! our own.

pub mod agents;
pub mod autoresolve;
pub mod battles;
pub mod capture;
pub mod commander_recruitment;
pub mod commands;
pub mod details;
pub mod deal_value;
pub mod diplomacy;
pub mod economy;
pub mod effects;
pub mod characters;
pub mod embark;
pub mod family;
pub mod features;
pub mod names;
pub mod negotiation;
pub mod movers;
pub mod naval;
pub mod events;
pub mod ids;
pub mod pathing;
pub mod population;
pub mod polypath;
pub mod polysmooth;
pub mod rtcut;
pub mod zoc;
pub mod religion;
pub mod research;
pub mod rules;
pub mod trade;
pub mod treaties;
pub mod treasury;
pub mod turn;
pub mod pool;
pub mod portraits;
pub mod visibility;
pub mod world;

pub use battles::{BattleResult, PendingBattle};
pub use capture::{CaptureChoice, CaptureOutcome, CapturePreview};
pub use commander_recruitment::{CommanderOption, CommanderRecruitment};
pub use commands::{CampaignCommand, CommandError, CommandQueue, ConstructionOption, PlannedMove};
pub use details::{CharacterDetails, FactionDetails, GovernmentPost, Governorship, GovernorshipTaxes, Relationship};
pub use events::CampaignEvent;
pub use ids::{CharacterId, FactionId, FortId, ForceId, RecruitmentItemId, RegionId, UnitId};
pub use rules::{BuildingRules, BuildingTable, CampaignRules, TaxClass, UnitAutoresolve, UnitRules};
pub use turn::{TurnState, TurnStep};
pub use world::{SlotRef, FactionAiKeys,
    BuildingRef, CampaignModel, CampaignUnit, Character, CharacterKind, ConstructionItem, Faction,
    GovernmentType, MilitaryForce, RecruitmentItem, RecruitmentSource, Region, RegionSlot, Settlement, Stance,
    Terrain, World,
};
pub use visibility::{FogState, Shroud, SightGrid};
pub use world::Fort;
pub use world::{EconomyRecord, ECONOMY_HISTORY_LEN};

#[cfg(test)]
mod tests;
