//! Character tables: traits, ancillaries and the junctions that limit who may gain them, plus the
//! subculture → culture map the trigger conditions use. Notes: `analysis/fidelity/CHARACTERS_FIDELITY.md`
//! (slot 0-G). Layouts: `analysis/worker1/DB_BUILDERS.md`; every table here parses to its end on the
//! install (`tests/real_install.rs`).

use crate::record::{db_record, Table};

db_record! {
    /// `character_traits` (`sibis`, 163 rows; exe reader 0x00F70470).
    pub struct CharacterTraitRecord in "character_traits", key = key {
        /// #0 trait key, e.g. `C_General_Brave`.
        key: String,
        /// #1 @0x0C the no-going-back level: once reached, falling points never take the trait below it
        /// (CONFIRMED, `0x008B5380`).
        no_going_back_level: i32,
        /// #2 @0x10 bool (INFERRED hidden).
        hidden: bool,
        /// #3 @0x14 eviction priority: with the trait list full, the held trait with the lowest value (then
        /// the fewest points) makes way (CONFIRMED use, `0x008C2D30` / `0x00898EC0`).
        priority: i32,
        /// #4 category, e.g. `Naval`, `Character Quirk`.
        category: String,
    }
}

db_record! {
    /// `trait_info` (`ss`, 163 rows): trait → scope (`agent` for every row; INFERRED: the script's
    /// second `effect.trait` argument).
    pub struct TraitInfo in "trait_info", key = trait_key {
        trait_key: String,
        scope: String,
    }
}

db_record! {
    /// `trait_to_antitraits` (`ss`, 93 rows): (trait, antitrait). Gaining the antitrait first takes points
    /// off the trait (CONFIRMED, `0x008C2D30`).
    pub struct TraitAntitrait in "trait_to_antitraits", key = trait_key {
        trait_key: String,
        antitrait: String,
    }
}

db_record! {
    /// `trait_to_included_agents` (`ss`, 179 rows): (trait, agent type).
    pub struct TraitIncludedAgent in "trait_to_included_agents", key = trait_key {
        trait_key: String,
        agent: String,
    }
}

db_record! {
    /// `ancillaries` (`sssbbbiii`, 275 rows; exe reader 0x00F94B70).
    pub struct AncillaryRecord in "ancillaries", key = key {
        /// #0 ancillary key.
        key: String,
        /// #1 picture (`ui/portraits/ancillaries/<image>.tga`).
        image: String,
        /// #2 type; only `character` ancillaries can be gained (CONFIRMED, `0x00A180C0`).
        kind: String,
        /// #3 bool (UNKNOWN).
        flag_3: bool,
        /// #4 bool, INFERRED world unique (record +0x21).
        world_unique: bool,
        /// #5 bool, INFERRED faction unique (record +0x22).
        faction_unique: bool,
        /// #6 eviction priority with the list full (CONFIRMED use, `0x009D1B90`).
        priority: i32,
        /// #7 first year it can be gained (inclusive).
        start_year: i32,
        /// #8 year from which it can no longer be gained (exclusive, CONFIRMED `0x00A180C0`).
        end_year: i32,
    }
}

db_record! {
    /// `ancillary_to_included_agents` (`ss`, 333 rows): (ancillary, agent type).
    pub struct AncillaryIncludedAgent in "ancillary_to_included_agents", key = ancillary {
        ancillary: String,
        agent: String,
    }
}

db_record! {
    /// `ancillary_to_excluded_ancillaries` (`ss`, 238 rows): (ancillary, an ancillary that blocks it).
    pub struct AncillaryExcluded in "ancillary_to_excluded_ancillaries", key = ancillary {
        ancillary: String,
        excluded: String,
    }
}

db_record! {
    /// `ancillary_included_subcultures` (`ss`, 1235 rows): (ancillary, subculture).
    pub struct AncillarySubculture in "ancillary_included_subcultures", key = ancillary {
        ancillary: String,
        subculture: String,
    }
}

db_record! {
    /// `cultures_subcultures` (`ssis`, 16 rows): subculture → culture.
    pub struct SubcultureRecord in "cultures_subcultures", key = key {
        key: String,
        culture: String,
        /// #2 UNKNOWN.
        unknown_2: i32,
        /// #3 e.g. `Ottoman`, `EU_East`.
        group: String,
    }
}

db_record! {
    /// `agent_attributes` (`ss`, 14 rows; exe reader 0x00E55090): attribute key → its picture.
    pub struct AgentAttributeRecord in "agent_attributes", key = key {
        /// #0 attribute key, e.g. `command_land`, `subterfuge`.
        key: String,
        /// #1 the attribute's picture as the shipped row spells it, e.g.
        /// `data/ui/campaign ui/pips/skill_spying.tga` (a loose file under `data\UI\Campaign UI\Pips\`),
        /// or the literal `PLACEHOLDER` for the six attributes that have none.
        icon: String,
    }
}

/// Every character table. Empty in [`crate::GameDatabase::test_fixture`].
#[derive(Debug, Clone, Default)]
pub struct CharacterTables {
    pub attributes: Table<AgentAttributeRecord>,
    pub traits: Table<CharacterTraitRecord>,
    pub trait_info: Table<TraitInfo>,
    pub antitraits: Table<TraitAntitrait>,
    pub trait_agents: Table<TraitIncludedAgent>,
    pub ancillaries: Table<AncillaryRecord>,
    pub ancillary_agents: Table<AncillaryIncludedAgent>,
    pub ancillary_excluded: Table<AncillaryExcluded>,
    pub ancillary_subcultures: Table<AncillarySubculture>,
    pub subcultures: Table<SubcultureRecord>,
}

impl CharacterTables {
    /// The picture of agent attribute `key`, the `PipPath` / `PrimaryAttributePath` the character
    /// cards and panels draw: the `agent_attributes` row's icon column, verbatim.
    ///
    /// Evidence: the mapping is shipped data (CONFIRMED: `agent_attributes` maps `command_land` →
    /// `skill_command`, `command_sea` → `skill_naval`, `duelling_pistols` → `skill_shooting`,
    /// `duelling_swords` → `skill_swordfighting`, `management` → `skill_managing`, `research` →
    /// `skill_research`, `subterfuge` → `skill_spying`, `zeal` → `skill_persuasion`, all under
    /// `data/ui/campaign ui/pips/`, each a loose file of the install; the other six rows say
    /// `PLACEHOLDER`). The exe's character-details builder (0x009AD250, `PrimaryAttributePath` set at
    /// 0x009AE7C5) gets the picture through 0x009CA690, which looks the attribute up in the
    /// `agent_attributes` table (0x00E0E580 loads it; 0x00F9C760 maps the attribute's index to its key,
    /// 0x00F9DB20, and finds the row; CONFIRMED) and hands the row's string to a path resolver
    /// (0x00A06F20) that splits it at its last `/` or `\` -- so it is the icon column (INFERRED: the
    /// field offset was not traced, but the only other column is the key). A `PLACEHOLDER` row is
    /// passed on as is (INFERRED: no separator, nothing to rewrite). A key with no row gives the empty
    /// string here; the exe logs "is not a valid key for this table" and hands back a shared static
    /// string holding the key text itself (CONFIRMED, 0x00F9C760), which names no file, so nothing is
    /// drawn either way. The exe indexes attributes through a fixed 14-entry key array (0x0145D978);
    /// this lookup is by key, so added rows work (no limit).
    ///
    /// PROVISIONAL: the resolver first tries `<folder>/<skin>/<file>` for the UI skin folders it is
    /// given and keeps the first that exists; the install ships no skin sub-folders under `Pips`,
    /// so for vanilla data the row's path is what it ends with. The skin folders are not modelled.
    pub fn attribute_icon(&self, key: &str) -> &str {
        self.attributes.get(key).map_or("", |r| r.icon.as_str())
    }
}
