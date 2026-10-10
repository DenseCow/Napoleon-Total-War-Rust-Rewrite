//! Campaign facts beyond the core rules: character names, portraits, traits and ancillaries;
//! governments (faction leader, ministers, governorships and their taxes); capitals; the full
//! diplomatic relationship records. Loaded by `ntw_campaign` from the start position or save
//! (layouts: `analysis/campaign/CAMPAIGN_DATA.md` §3) and kept in [`World`](super::World)'s
//! `character_details`, `faction_details` and `relationships` maps, keyed by the same ids as the
//! core records. The UI and the AI read them; the save writer writes the parts the model changes.
//!
//! Kept in separate maps (not as fields of `Character` / `Faction`) so that code which builds
//! those records by hand does not have to change.

use std::collections::BTreeMap;

use super::ids::{CharacterId, FactionId, RegionId};
use crate::calendar::Date;

/// `CHARACTER_DETAILS` v3 (CONFIRMED structure in all 8 startpos files and the user's saves).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CharacterDetails {
    /// #1 `CAMPAIGN_LOCALISATION` forename: a loc key such as `names_name_names_frenchDenis`
    /// (table `names`, field `name`, key `names_frenchDenis`) (CONFIRMED form).
    pub forename: String,
    /// #2 surname loc key, same form.
    pub surname: String,
    /// #3 `CAMPAIGN_LOCALISATION` with two strings (empty in every sample; INFERRED: a clan or
    /// title name pair).
    pub other_names: (String, String),
    /// #4 utf16: the regnal numeral shown after the name ("III" for George III; empty except for
    /// monarchs). CONFIRMED: the name builder `0x00A0FE80` appends character +0x33C, which
    /// `0x008DCF50` fills from the family member's regnal number ([`super::family::regnal_numeral`]);
    /// the eur startpos holds "III" here for George, Friedrich Wilhelm and Ferdinando
    /// (UI_FIDELITY.md 4.7).
    pub regnal_numeral: String,
    /// #5 `DATE`: birth date (INFERRED: years 1730..1800 for 1805 characters).
    pub birth: Option<Date>,
    /// #6 `DATE` (year 0 in every startpos; UNKNOWN, INFERRED death date).
    pub date_2: Option<Date>,
    /// #8 `PORTRAIT_DETAILS`.
    pub portrait: Portrait,
    /// #9 utf16 faction key (CONFIRMED equal to the owner's key).
    pub faction_key: String,
    /// #0 `TRAITS/TRAIT[]`: {utf16 trait key, i32 points} (CONFIRMED keys of the `character_traits`
    /// table; points INFERRED).
    pub traits: Vec<CharacterTrait>,
    /// #11 `AgentAncillaries[]` {utf16 ancillary key} (CONFIRMED in the saves).
    pub ancillaries: Vec<String>,
    /// #12 `AgentAttributes[]` {utf16 attribute key, i32 level} (`command_land`, `subterfuge`, ...;
    /// -1 = the agent type does not have it, INFERRED).
    pub attributes: Vec<(String, i32)>,
    /// #13 `AgentAbilities[]` {utf16 key, i32, utf16} (`can_assassinate`, ...).
    pub abilities: Vec<(String, i32, String)>,
    /// #14 `AgentAttributeBonuses[]` {utf16 key, u32}.
    pub attribute_bonuses: Vec<(String, u32)>,
    /// #16 `CAMPAIGN_LOCALISATION` on-screen type name, e.g.
    /// `agent_culture_details_onscreen_name_ministereuropean`.
    pub onscreen_name: String,
    /// `CHARACTER` #8 u32: the id of the government post (`CHARACTER_POST` #0) the character
    /// holds, 0 = none (CONFIRMED: France's navy minister holds the `navy` post's id).
    pub post: u32,
    /// `CHARACTER` #34 bool (+0x52C, CONFIRMED offset from the loader `0x00991520`): such a character
    /// skips the yearly death check (`0x009DA190`) and, when killed, is rebuilt as a copy of himself
    /// (`0x0099D2D0` → `0x0098FE60`, placed by `0x009D3B60`). Set for exactly one General per major
    /// power in the vanilla saves (Napoleon for France). INFERRED: the famous commander who is
    /// wounded and returns rather than dying.
    pub returns_after_death: bool,
    /// `CHARACTER` #36 u32 (+0x534): duels lost (the duel ending `0x008BFD90` adds 1 to the loser;
    /// CONFIRMED offset and use).
    pub duels_lost: u32,
    /// `CHARACTER` #37 u32 (+0x538): duels won (the winner, same function).
    pub duels_won: u32,
    /// `CHARACTER` #22 bool (+0x4E0, CONFIRMED offset from the loader `0x00991520`): hidden from other
    /// factions unless they have exposed him (`0x008CE880`). Set by `0x009D3000` from the stealth test
    /// `0x009D1010` (an army commander in the open whose units can all hide, or with
    /// `campaign_map_stealth`, no foreign character in his force; CHARACTERS_FIDELITY.md §10).
    /// PROVISIONAL: the model keeps the loaded value (the stealth test is not ported).
    pub hidden: bool,
    /// `CHARACTER` #28 u32 (+0x504): turns at sea (`0x00A28D90`, CONFIRMED offsets from the loader `0x00991520`).
    pub turns_at_sea: u32,
    /// `CHARACTER` #29 u32 (+0x508): turns in the lands of a faction at war with his (`0x00A28DD0`).
    pub turns_in_enemy_lands: u32,
    /// `CHARACTER` #30 u32 (+0x50C): turns in his own faction's lands (`0x00A28E20`).
    pub turns_at_home: u32,
    /// `CHARACTER` #14 bool (+0x4C5): he did not act in his last turn (`0x009DA210`).
    pub no_action: bool,
    /// `CHARACTER` #15 u32 (+0x4C8): turns without acting.
    pub idle_turns: u32,
    /// `CHARACTER` #27 bool (+0x510): he fled a lost duel (`0x00A18460` sets it before his flight
    /// order; CONFIRMED). The stealth test refuses such a character (`0x009D1010`); the flight
    /// at a turn end (`0x00A18550`) is skipped for him. False in every vanilla save.
    pub fled: bool,
    /// Character +0x512 (not saved): wounded in a duel, walking back (`0x008BFD90` sets it with
    /// the `duel_success` message; the flight order's completion `0x0094EA00` clears it with
    /// message 0x46). PROVISIONAL: cleared when the model's flight ends.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub wounded: bool,
    /// The `historical_characters` key he was made from (CHARACTERS_FIDELITY.md §8; the pool
    /// panel's badge); `None` for a generic character.
    pub historical_key: Option<String>,
}

/// `PORTRAIT_DETAILS` v1: card picture, custom picture name, info picture, number (the details'
/// +0x80, +0x98, +0x8C, +0x7C in that order, CONFIRMED by the writer `0x0099F6D0`). Who sets them:
/// [`super::portraits`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Portrait {
    /// #0 e.g. `ui/portraits/european/Cards/minister/young/062.tga` (CONFIRMED path form).
    pub card: String,
    /// #1 the custom picture name (`ui/portraits/custom/...`, `0x009CBD60`): when set, the pictures
    /// come from it instead of the decks (empty for every generated character).
    pub alternative: String,
    /// #2 e.g. `ui/portraits/european/Info/minister/young/062.jpg`.
    pub info: String,
    /// #3 the picture number (62 above); -1 = none yet.
    pub index: i32,
}

impl Default for Portrait {
    /// No pictures and number -1, as the details constructor `0x00992E60` starts a character
    /// (+0x7C = -1, CONFIRMED).
    fn default() -> Self {
        Portrait { card: String::new(), alternative: String::new(), info: String::new(), index: -1 }
    }
}

/// One trait of a character.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CharacterTrait {
    /// `character_traits` key, e.g. `C_Minister_Upright`.
    pub key: String,
    /// i32 points (INFERRED).
    pub points: i32,
}

/// One `GOVERNMENT/POSTS_ARRAY/CHARACTER_POST` v1 (CONFIRMED structure).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GovernmentPost {
    /// #0 i32 post id (referenced by `CHARACTER` #8).
    pub id: i32,
    /// #1 utf16 `ministerial_positions` key: `faction_leader`, `head_of_government`, `finance`,
    /// `army`, `navy`, `justice`, `accident`, `governor_europe`, ... (CONFIRMED keys).
    pub key: String,
    /// #2 u32 holder character id (0 = vacant).
    pub holder: Option<CharacterId>,
    /// #3 bool true + #4 `GOVERNORSHIP` for a governor's post.
    pub governorship: Option<Governorship>,
}

/// `GOVERNORSHIP` v1 (CONFIRMED structure).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Governorship {
    /// #0 `GOVERNORSHIP_TAXES`.
    pub taxes: GovernorshipTaxes,
    /// #1 i32 (equal to the `REGION` #22 theatre link of the governed regions; INFERRED theatre id).
    pub theatre_id: i32,
    /// #2 u32[] the governed regions (CONFIRMED: France's 12 regions, Paris's region among them).
    pub regions: Vec<RegionId>,
    /// #3 u32 the faction id.
    pub faction: FactionId,
    /// #4, #5 bools (UNKNOWN).
    pub flags: (bool, bool),
}

/// `GOVERNORSHIP_TAXES` v1: {u32 lower, u32 upper, u8 lower rate, u8 upper rate}.
/// The u8 is the level's `taxes_levels` rate (CONFIRMED: 15 = `tax_normal`, with level 2 in every
/// sample). The u32 is the level index in rate order, 0 `tax_minimal` .. 4 `tax_extortionate`
/// (INFERRED: only level 2 / rate 15 occurs in the shipped files and the user's saves).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GovernorshipTaxes {
    /// Lower classes' level index.
    pub lower: u32,
    /// Upper classes' level index.
    pub upper: u32,
    /// Lower classes' rate in percent (`i32` like `taxes_levels`; the save field is a u8, which the
    /// ESF writer checks).
    pub lower_rate: i32,
    /// Upper classes' rate in percent.
    pub upper_rate: i32,
}

/// The `taxes_levels` keys in index order (see [`GovernorshipTaxes`]; INFERRED order).
pub const TAX_LEVELS: [&str; 5] = ["tax_minimal", "tax_low", "tax_normal", "tax_high", "tax_extortionate"];

impl GovernorshipTaxes {
    /// The `taxes_levels` key of a level index.
    pub fn level_key(index: u32) -> Option<&'static str> {
        TAX_LEVELS.get(index as usize).copied()
    }
    /// The index of a `taxes_levels` key.
    pub fn level_index(key: &str) -> Option<u32> {
        TAX_LEVELS.iter().position(|k| *k == key).map(|i| i as u32)
    }
}

/// Faction facts that are not part of [`Faction`](super::Faction).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FactionDetails {
    /// `FACTION` plain value 2: display name (`France`; the rebels' `NOT FOR DISPLAY (...)`).
    pub display_name: String,
    /// `GOVERNMENT` #0 i32 government object id.
    pub government_id: i32,
    /// `GOVERNMENT/POSTS_ARRAY`, in file order.
    pub posts: Vec<GovernmentPost>,
    /// The first i32 after the second `FORT_UPGRADE_MANAGER`: the capital region's id
    /// (CONFIRMED: France's is `eur_france`, Paris; INFERRED meaning "capital").
    pub capital: Option<RegionId>,
    /// The i32 after it (equal to the capital in every startpos; UNKNOWN, INFERRED original
    /// capital).
    pub capital_2: Option<RegionId>,
    /// The utf16 religion key after `FACTION`'s u32[1] and bool (`rel_catholic`).
    pub religion: String,
    /// The `FACTION` bool right before `CHARACTER_ARRAY` (faction +0x524): a major power. CONFIRMED: the
    /// exe reads it there (0x0087A190) and picks `faction_gdp_other` for a major, else
    /// `faction_gdp_other_minor` (0x00BBC710); set for France, Austria, Britain, Prussia, Russia, the
    /// Ottomans and Spain in the eur startpos. `None` when not loaded.
    pub major: Option<bool>,
    /// Trade income as the original last stored it: `FACTION_ECONOMICS` history, last `ECONOMICS_DATA`,
    /// #1[2] (income category 7, written by the trade code 0x00BB3490; CONFIRMED position). Kept for
    /// checks (the model computes trade income itself).
    pub stored_trade_income: Option<i32>,
    /// `FACTION_TECHNOLOGY_MANAGER` `techs[]`: (technology key, state). States (CAMPAIGN_FIDELITY.md
    /// §Research): 0 researched (CONFIRMED: completion 0x008EED20 sets it), 2 available, 4 not yet available.
    pub technologies: Vec<(String, u32)>,
    /// Research under way or banked, by technology key: `techs[]` #2 f32 progress and #3 u32 the
    /// researching school (`REGION_SLOT` #2 id; 0 = none). Only entries with progress or a researcher.
    pub research: std::collections::BTreeMap<String, TechResearch>,
    /// The campaign difficulty of the faction's stored difficulty block (`CAMPAIGN_PLAYER_SETUP_INGAME_MODIFIABLES`
    /// #0, faction +0x6E4; −2 very hard .. 1 easy, CONFIRMED source, see `ntw_ai::campaign::keys`).
    pub difficulty: i32,
    /// The saved base effect entries (`FACTION` #54 `CAMPAIGN_BONUS_VALUES`, faction +0x8C4: agent caps and what
    /// scripts and events gave; EFFECTS_FIDELITY.md §1).
    pub bonus_base: Vec<super::effects::SavedBonus>,
    /// The saved base + difficulty entries (`FACTION` #55, faction +0x8D4); kept for checks, the model
    /// rebuilds this from `bonus_base` and the difficulty.
    pub bonus_with_difficulty: Vec<super::effects::SavedBonus>,
    /// `CHARACTER_RECRUITMENT_MANAGER` (FACTION #75): the general and admiral recruitment pools, each
    /// {candidate character ids, u32 refill timer = the turn the next candidate appears, 0 = idle}
    /// (CHARACTERS_FIDELITY.md §8; structure CONFIRMED, timer meaning INFERRED from `0x00A252B0`).
    pub general_pool: (Vec<CharacterId>, u32),
    /// The admiral pool, as `general_pool`.
    pub admiral_pool: (Vec<CharacterId>, u32),
    /// `FAMILY` (the royal family: leader, spouse, children, relatives; `super::family`). `None` when
    /// not loaded.
    pub family: Option<super::family::Family>,
    /// `EXPOSED_CHARACTERS[]` = {i32 character id} (faction +0x80C / +0x810, CONFIRMED list; added to by
    /// `0x008A8B30`): the foreign hidden characters this faction has spotted
    /// ([`super::agents::knows_character`]). France holds one hidden Spanish general in a vanilla
    /// Peninsular save.
    pub exposed: Vec<CharacterId>,
}

impl FactionDetails {
    /// The holder of the post with this key.
    pub fn holder_of(&self, post: &str) -> Option<CharacterId> {
        self.posts.iter().find(|p| p.key == post).and_then(|p| p.holder)
    }
    /// The faction leader (`faction_leader` post).
    pub fn leader(&self) -> Option<CharacterId> {
        self.holder_of("faction_leader")
    }
    /// The ministers: every non-governor, non-leader post with a holder, in file order.
    pub fn ministers(&self) -> impl Iterator<Item = (&str, CharacterId)> {
        self.posts
            .iter()
            .filter(|p| p.governorship.is_none() && p.key != "faction_leader")
            .filter_map(|p| Some((p.key.as_str(), p.holder?)))
    }
    /// The first governorship (Napoleon campaigns have one theatre).
    pub fn governorship(&self) -> Option<&Governorship> {
        self.posts.iter().find_map(|p| p.governorship.as_ref())
    }
}

/// The 24 attitude factor keys, in the order of `DIPLOMACY_RELATIONSHIP` #1 (CONFIRMED: the exe's
/// static key array at `0x0042F300`; the slots written by the war, abused-access, embargo and
/// initial-modifier code sit at exactly these indices). Each key is a `diplomacy_factor_strings`
/// row (the 25th row, `diplo_character_bonus`, has no slot).
pub const ATTITUDE_FACTORS: [&str; 24] = [
    "state_gift",
    "alliance",
    "alliance_broken",
    "cultural_alliance_broken",
    "declared_war_against_enemies",
    "trade",
    "trade_broken",
    "war",
    "peace_treaty",
    "allied_with_enemies",
    "declared_war_against_friends",
    "abandoned_ally_in_war",
    "annexed_territory",
    "abused_military_access",
    "assasination_attempt",
    "religion",
    "government_type",
    "initial_modifier",
    "sabotage_attempt",
    "spying_attempt",
    "threatened",
    "faction_leader",
    "enlightenment",
    "trade_embargoed",
];

/// The 14 `force_diplomacy` option keys, in the order of `DIPLOMACY_RELATIONSHIP` #18 (CONFIRMED:
/// the script handler `0x009792D0` maps these strings to the indices). Index 10 is the stance key
/// `war`.
pub const DIPLOMACY_OPTIONS: [&str; 14] = [
    "trade agreement",
    "military access",
    "cancel military access",
    "alliance",
    "regions",
    "technology",
    "state_gift",
    "payments",
    "protectorate",
    "peace",
    "war",
    "join_war",
    "break_trade",
    "break_alliance",
];

/// The [`DIPLOMACY_OPTIONS`] index of a `force_diplomacy` option key: an exact, case-sensitive
/// match (CONFIRMED: `0x009792D0` compares with `0x0044F000`; an unknown key stores nothing).
pub fn diplomacy_option_index(key: &str) -> Option<usize> {
    DIPLOMACY_OPTIONS.iter().position(|k| *k == key)
}

/// The value `force_diplomacy(a, b, option, offer, accept)` stores on `a`'s relationship towards
/// `b`: 1 when `accept` is false, plus 2 when `offer` is false (CONFIRMED `0x009792D0`: the Lua
/// arguments are read from the top of the stack, accept first).
pub fn diplomacy_option_value(offer: bool, accept: bool) -> u32 {
    u32::from(!accept) + 2 * u32::from(!offer)
}

/// Whether a relationship's option value lets its owner propose the option to the target: not 2
/// or 3 (CONFIRMED readers: the negotiation records `0x00BF5CB3`, the AI intentions `0x00CAD070`,
/// the deal goals `0x00CCB150`).
pub fn option_allows_proposal(value: u32) -> bool {
    !matches!(value, 2 | 3)
}

/// Whether a relationship's option value lets its owner accept the option from the target: not 1
/// or 3 (CONFIRMED readers: the AI deal evaluator `0x00AA5ED0`, the counter-offers `0x00CAD0E0`,
/// the deal goals `0x00CCB150`).
pub fn option_allows_acceptance(value: u32) -> bool {
    !matches!(value, 1 | 3)
}

/// One attitude factor (an item of `DIPLOMACY_RELATIONSHIP_ATTITUDES_ARRAY`: i32, i32, i32,
/// bool, i32, bool; CONFIRMED meanings from the per-turn update `0x00B290D0` and the attitude sum
/// `0x00B0DB60`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AttitudeFactor {
    /// Change per turn, added to `value` at each end of turn (e.g. -2 for `war`).
    pub drift: i32,
    /// The current value.
    pub value: i32,
    /// The value the drift stops at when `limited` (e.g. 80 for `alliance`, -200 for `war`).
    pub limit: i32,
    /// Whether `limit` applies.
    pub limited: bool,
    /// The most this factor adds to the total when `capped` (positive: at most `cap`; zero or
    /// negative: at least `cap`). Set at campaign start on `declared_war_against_enemies` (15) and
    /// `declared_war_against_friends` (-15) in every sample.
    pub cap: i32,
    /// Whether `cap` applies.
    pub capped: bool,
}

impl AttitudeFactor {
    /// What this factor adds to the attitude total (the exe's sum `0x00B0DB60`, CONFIRMED).
    pub fn contribution(&self) -> i32 {
        if !self.capped {
            self.value
        } else if self.cap > 0 {
            self.value.min(self.cap)
        } else {
            self.value.max(self.cap)
        }
    }
}

/// One `REGULAR_PAYMENTS` item (CONFIRMED layout: reader `0x00AE90E0`, writer `0x00B7A4F0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RegularPayment {
    /// i32 amount per turn (INFERRED direction: paid by the owner to the target).
    pub amount: i32,
    /// u32 turns left (CONFIRMED: decremented each turn, removed at 0).
    pub turns: u32,
}

/// One `ALLIED_IN_WAR_AGAINST` item (CONFIRMED: added by `0x00B0CCE0` when the owner joins the
/// target's war).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AlliedWar {
    /// u32 the common enemy's faction id.
    pub enemy: FactionId,
    /// i32 the military access turns the owner gave before the war. The war sets the access to
    /// -1 (unlimited); this value is put back when the alliance ends, and counts down each turn.
    pub saved_access_turns: i32,
}

/// One `DIPLOMACY_RELATIONSHIP` v14 (CONFIRMED structure, the exe's writer `0x00AFC340` and reader
/// `0x00AE9480`), from the owning faction towards `target`. The stance itself is in
/// [`Faction::diplomacy`](super::Faction::diplomacy). Field meanings and evidence:
/// `analysis/campaign/S1_LEFTOVERS.md` §1. The rules that change them: [`treaties`](super::treaties).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Relationship {
    /// #1 the 24 attitude factors, in [`ATTITUDE_FACTORS`] order (CONFIRMED).
    pub attitudes: Vec<AttitudeFactor>,
    /// #2 bool trade agreement (CONFIRMED: "current_treaty_trade_agreement").
    pub trade_agreement: bool,
    /// #3 i32 military access the owner gives the target: turns left, -1 = indefinite, 0 = none
    /// (CONFIRMED: `RetrieveRemainingMilitaryAccessTurns`, "current_treaty_giving_military_access_*").
    pub military_access_turns: i32,
    /// #5 i32 the faction whose call brought the owner into this war (CONFIRMED a faction id;
    /// INFERRED meaning: set when war is declared on an ally's behalf, cleared at peace).
    pub war_ally: Option<FactionId>,
    /// #6 u32 turns left of the alliance commitment (20 on every startpos alliance; breaking the
    /// alliance while it runs costs attitude with others) (CONFIRMED countdown; INFERRED meaning).
    pub alliance_commitment_turns: u32,
    /// #7 i32 war momentum: each battle result adds -8..+8; it decays by 2 a turn when positive
    /// and recovers by 1 when negative; the AI peace evaluation reads it (CONFIRMED).
    pub war_momentum: i32,
    /// #8 i32 protectorate tribute the owner pays while it is the target's protectorate
    /// (economics category 6) (CONFIRMED source; INFERRED name).
    pub protectorate_tribute: i32,
    /// #9 i32 protectorate income the owner receives as the target's patron (economics category 2)
    /// (CONFIRMED source; INFERRED name).
    pub protectorate_income: i32,
    /// #10 i32 at war: region value balance, (captured - lost) / 1000 clamped to -10..10 (CONFIRMED).
    pub war_region_balance: i32,
    /// #11 i32 at war: strength/wealth balance / 2500 clamped to -10..10 (CONFIRMED formula shape).
    pub war_wealth_balance: i32,
    /// #12 u32 turns at war (CONFIRMED: +1 each turn at war, 0 at peace).
    pub war_turns: u32,
    /// #13 u32 turns since the last battle in this war (CONFIRMED: reset by a battle result).
    pub turns_since_battle: u32,
    /// #14 `REGULAR_PAYMENTS[]` (CONFIRMED).
    pub payments: Vec<RegularPayment>,
    /// #15 u32 recent-friendship countdown: set to 10 by peace, military access, an alliance or a
    /// regular payment, -1 a turn (CONFIRMED; INFERRED name).
    pub friendship_turns: u32,
    /// #16 u32: CONFIRMED unused by the original (only constructed, reset, loaded, saved and
    /// copied; S1_LEFTOVERS.md §1). Kept so saves round-trip; 0 in every sample.
    pub unknown_16: u32,
    /// #17 `ALLIED_IN_WAR_AGAINST[]` (CONFIRMED).
    pub allied_in_war_against: Vec<AlliedWar>,
    /// #18 u32[14] `force_diplomacy` permissions per [`DIPLOMACY_OPTIONS`] entry: 0 allowed,
    /// +1 the owner declines the option from the target, +2 the owner may not propose it
    /// ([`diplomacy_option_value`]; CONFIRMED; 0 everywhere in the samples). Read through
    /// [`CampaignModel::may_propose`](super::CampaignModel::may_propose) /
    /// [`CampaignModel::may_accept`](super::CampaignModel::may_accept).
    pub diplomacy_options: [u32; DIPLOMACY_OPTIONS.len()],
    /// #19 u32 consecutive turns the military access has been in force (CONFIRMED).
    pub military_access_streak: u32,
    /// #20 utf16 a second stance (INFERRED: the previous stance; written by our save on a change).
    pub previous_stance: String,
    /// #21 bool: CONFIRMED unused by the original (only loaded, saved and copied). Kept so saves
    /// round-trip.
    pub unknown_21: bool,
    /// #22 bool: CONFIRMED unused (as #21).
    pub unknown_22: bool,
    /// #23 i32 the start attitude, a `diplomatic_relations_attitudes` threshold (-85, -45, 0, 45,
    /// 85), applied to the `initial_modifier` factor at campaign start (CONFIRMED values; INFERRED use).
    pub start_attitude: i32,
    /// #24 i32 military access as granted: turns, -1 = indefinite (CONFIRMED setter `0x00B44550`).
    pub military_access_granted: i32,
    /// #25 u32 turns elapsed of timed military access (CONFIRMED).
    pub military_access_elapsed: u32,
    /// #26 u32 grievance from early cancelled military access (50/60/70/90 by the granted length,
    /// minus a share of the turns used; -2 a turn) (CONFIRMED formula shape; INFERRED name).
    pub access_cancel_grievance: u32,
    /// #27 u32 trade embargo turns left (10 when imposed, -1 a turn; CONFIRMED).
    pub trade_embargo_turns: u32,
    /// #28 bool (true unless the owner, as patron, went to war with the target; read by the
    /// region-return rule on capture) (CONFIRMED uses; INFERRED name).
    pub allows_region_return: bool,
}

impl Relationship {
    /// The attitude total: the sum of [`AttitudeFactor::contribution`] (CONFIRMED, `0x00B0DB60`).
    pub fn attitude_total(&self) -> i32 {
        self.attitudes.iter().map(AttitudeFactor::contribution).sum()
    }

    /// The factor with this key ([`ATTITUDE_FACTORS`]).
    pub fn factor(&self, key: &str) -> Option<&AttitudeFactor> {
        ATTITUDE_FACTORS.iter().position(|k| *k == key).and_then(|i| self.attitudes.get(i))
    }
}

/// A target of a mission: a model object found through the original's global id map, or the
/// raw saved id when no loaded object has it (forts and ports are not modelled yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MissionTarget<T> {
    /// No target (the saved value was 0).
    None,
    /// The loaded object.
    Found(T),
    /// A saved id that names no loaded object of the expected kind.
    Unresolved(u32),
}

impl<T> MissionTarget<T> {
    /// The object, if found.
    pub fn found(&self) -> Option<&T> {
        match self {
            MissionTarget::Found(t) => Some(t),
            _ => None,
        }
    }
}

/// A mission given by the campaign script (`trigger_custom_mission`), loaded from the faction's
/// `CAMPAIGN_MISSION_MANAGER` (layout: S1_MISSIONS_UI.md §(a)). The saved targets are object
/// ids, which the original maps through its global id → object map after loading (CONFIRMED:
/// the faction post-load fix-up `0x008E07F0` → `0x00A18670` → `0x00A186A0` / `0x00A18720`,
/// lookup `0x0105AC60`); the loader does the same with the model's ids.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CampaignMission {
    /// The script's mission key, e.g. `eur_take_vienna`.
    pub script_key: String,
    /// The objective kind (the original's numbers: 0 capture_city .. 15 sabotage_enemy_building).
    pub kind: u32,
    /// Turn limit (INFERRED; 0 = none).
    pub turns: u32,
    /// Turns elapsed (INFERRED).
    pub elapsed: u32,
    /// The settlement to take or infiltrate, as the region that holds it.
    pub settlement: Option<MissionTarget<RegionId>>,
    /// The fort (raw id: forts are not modelled).
    pub fort: u32,
    /// The faction to ally with, make peace with or trade with.
    pub faction: Option<MissionTarget<FactionId>>,
    /// The port (raw id: ports are not modelled).
    pub port: u32,
    /// The character to assassinate.
    pub character: Option<MissionTarget<CharacterId>>,
    /// The region (end_rebellion, restore_public_order, liberate_region).
    pub region: Option<MissionTarget<RegionId>>,
    /// The regions of protectorate_region_capture.
    pub regions: Vec<MissionTarget<RegionId>>,
    /// `building_levels` / `units` / `technologies` keys (empty when unused).
    pub building_level: String,
    /// See [`CampaignMission::building_level`].
    pub unit: String,
    /// See [`CampaignMission::building_level`].
    pub technology: String,
    /// Reward money.
    pub reward_money: u32,
    /// Reward: the faction taken over.
    pub reward_takeover: Option<MissionTarget<FactionId>>,
    /// Reward: agents granted (agent type, region).
    pub reward_agents: Vec<(String, MissionTarget<RegionId>)>,
    /// Reward: units granted (unit key and the two saved values, mapped by the original too;
    /// meaning UNKNOWN).
    pub reward_units: Vec<(String, u32, u32)>,
    /// Reward experience for armies and navies.
    pub reward_experience: (u32, u32),
    /// Reward: the unit whose recruitment is enabled.
    pub reward_recruitment: String,
}

/// The script missions of each faction, in their saved order.
pub type MissionMap = BTreeMap<FactionId, Vec<CampaignMission>>;

/// The detail maps kept by [`World`](super::World).
pub type CharacterDetailsMap = BTreeMap<CharacterId, CharacterDetails>;
/// Faction details by faction.
pub type FactionDetailsMap = BTreeMap<FactionId, FactionDetails>;
/// Relationships by (owner, target).
pub type RelationshipMap = BTreeMap<(FactionId, FactionId), Relationship>;

#[cfg(test)]
mod tests {
    use super::*;

    /// `force_diplomacy`'s two booleans (`0x009792D0`: !accept + 2·!offer) and the bits the
    /// readers test (offer 2|3 at `0x00BF5CB3` / `0x00CAD070`, accept 1|3 at `0x00AA5ED0` / `0x00CAD0E0`).
    #[test]
    fn diplomacy_option_bits() {
        assert_eq!(diplomacy_option_value(true, true), 0);
        assert_eq!(diplomacy_option_value(true, false), 1);
        assert_eq!(diplomacy_option_value(false, true), 2);
        assert_eq!(diplomacy_option_value(false, false), 3);
        assert_eq!([0, 1, 2, 3].map(option_allows_proposal), [true, true, false, false]);
        assert_eq!([0, 1, 2, 3].map(option_allows_acceptance), [true, false, true, false]);
        assert_eq!(diplomacy_option_index("military access"), Some(1));
        assert_eq!(diplomacy_option_index("Peace"), None, "exact, case-sensitive (0x0044F000)");
    }

    #[test]
    fn tax_levels_and_posts() {
        assert_eq!(GovernorshipTaxes::level_key(2), Some("tax_normal"));
        assert_eq!(GovernorshipTaxes::level_index("tax_high"), Some(3));
        let d = FactionDetails {
            posts: vec![
                GovernmentPost { id: 1, key: "faction_leader".into(), holder: Some(CharacterId(7)), governorship: None },
                GovernmentPost { id: 2, key: "finance".into(), holder: Some(CharacterId(8)), governorship: None },
                GovernmentPost { id: 3, key: "army".into(), holder: None, governorship: None },
            ],
            ..Default::default()
        };
        assert_eq!(d.leader(), Some(CharacterId(7)));
        assert_eq!(d.ministers().collect::<Vec<_>>(), vec![("finance", CharacterId(8))]);
    }

    #[test]
    fn attitude_total_applies_caps() {
        assert_eq!(ATTITUDE_FACTORS[7], "war");
        assert_eq!(DIPLOMACY_OPTIONS[10], "war");
        let mut r = Relationship { attitudes: vec![AttitudeFactor::default(); 24], ..Default::default() };
        r.attitudes[1].value = 40; // alliance
        r.attitudes[4] = AttitudeFactor { value: 72, cap: 15, capped: true, ..Default::default() };
        r.attitudes[10] = AttitudeFactor { value: -30, cap: -15, capped: true, ..Default::default() };
        r.attitudes[7].value = -140; // war
        assert_eq!(r.attitudes[4].contribution(), 15);
        assert_eq!(r.attitudes[10].contribution(), -15);
        assert_eq!(r.attitude_total(), 40 + 15 - 15 - 140);
        assert_eq!(r.factor("alliance").map(|f| f.value), Some(40));
        assert!(r.factor("nonsense").is_none());
    }
}

/// One technology's research (`FACTION_TECHNOLOGY_MANAGER` `techs[]` #2 / #3 / #5, CONFIRMED layout).
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TechResearch {
    /// Research points gathered (#2 f32; the technology's cost once researched).
    pub progress: f32,
    /// The researching school slot (`RegionSlot::id`), 0 when none.
    pub researcher: u32,
    /// How often the faction handed the technology over in a deal (#5 u32, entry +0x28: raised by
    /// `0x008F3DD0`, saved by `0x00894430`, read by the AI's deal value `0x00A36B20`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub traded: u32,
}

impl Eq for TechResearch {}
