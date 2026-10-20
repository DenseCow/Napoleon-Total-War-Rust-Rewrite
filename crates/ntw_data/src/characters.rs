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
        /// #1 @0x0C the no-going-back level: once the held level has reached it, a loss of points that reaches a
        /// lower level lands the trait on it, its points set to its threshold (CONFIRMED, `0x008B5380`).
        /// Points that reach no level (below the first threshold) change nothing, and below 1 the trait is gone.
        no_going_back_level: i32,
        /// #2 @0x10 hidden: the character's trait list in the UI leaves the trait out (CONFIRMED, the list
        /// builder 0x008C65A0 keeps only traits with this false and a current level above 0).
        hidden: bool,
        /// #3 @0x14 eviction priority: with the trait list full, the held trait with the lowest value (then
        /// the fewest points) makes way (CONFIRMED use, `0x008C2D30` / `0x00898EC0`).
        priority: i32,
        /// #4 @0x18 category, e.g. `Naval`, `Character Quirk` (a `trait_categories` key).
        category: String,
    }
}

db_record! {
    /// `trait_categories` (`so`, 12 rows): a trait category and its picture, the `IconFilename` of every
    /// trait in it on the character and enlist panels (CONFIRMED, 0x008C9C70 reads trait record +0x18 →
    /// category record +0x0C).
    pub struct TraitCategoryRecord in "trait_categories", key = category {
        /// #0 category key, e.g. `Military`.
        category: String,
        /// #1 the picture, e.g. `data/ui/campaign ui/pips/effect_army.tga`.
        icon: Option<String>,
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
    /// `cultures` (`sio`): built into `CULTURE_RECORD` (stride 0x24) by `0x00F73880`: key +0x00,
    /// #1 +0x10, #2 +0x14.
    pub struct CultureRecord in "cultures", key = key {
        key: String,
        /// #1 UNKNOWN.
        unknown_1: i32,
        /// #2 another culture key (vanilla: egy_european, PIR_european, PIR_natives, PIR_pirates,
        /// tribal → european; egy_middle_east, indian → middle_east; middle_east → indian; empty for
        /// european). The victory movie lookup `0x00F54F80` takes a row whose
        /// culture has this column equal to the faction's culture key (CONFIRMED by disassembly).
        alternate: Option<String>,
    }
}

db_record! {
    /// `agent_attributes`(`ss`, 14 rows; exe reader 0x00E55090): attribute key → its picture.
    pub struct AgentAttributeRecord in "agent_attributes", key = key {
        /// #0 attribute key, e.g. `command_land`, `subterfuge`.
        key: String,
        /// #1 the attribute's picture as the shipped row spells it, e.g.
        /// `data/ui/campaign ui/pips/skill_spying.tga` (a loose file under `data\UI\Campaign UI\Pips\`),
        /// or the literal `PLACEHOLDER` for the six attributes that have none.
        icon: String,
    }
}

db_record! {
    /// `agent_to_agent_attributes` (`ssio`, 30 rows): an agent type's starting level of one attribute.
    /// Every character maker (`0x00992E60`, called by the generic, historical, unit-officer and
    /// template makers) sets all 14 attributes to −1 (`0x00984AD0`) and then copies the agent's rows
    /// (CONFIRMED: in 530 of the 531 characters of the `eur_napoleon` start position the saved level
    /// minus his trait and ancillary attribute bonuses is his type's row, or −1 without one; the one
    /// exception is as the start position was written, `ntw_campaign` test
    /// `stored_attributes_are_the_start_levels_plus_the_effects`). The table reader is `0xF94B10`
    /// (getter `0x00E0EB40`, database +0x568). Merge key: the two key columns (INFERRED).
    pub struct AgentToAgentAttribute in "agent_to_agent_attributes", key = attribute + agent {
        /// #0 the attribute key (`agent_attributes`), e.g. `command_land`.
        attribute: String,
        /// #1 the agent type (`agents` #0), e.g. `General`.
        agent: String,
        /// #2 the starting level.
        level: i32,
        /// #3 UNKNOWN (1 on most rows; 0 on `morale_land` / `movement_points_land` for General and on
        /// every level-0 row, yet those characters still start at the row's level).
        flag: bool,
    }
}

/// Every character table. Empty in [`crate::GameDatabase::test_fixture`].
#[derive(Debug, Clone, Default)]
pub struct CharacterTables {
    pub attributes: Table<AgentAttributeRecord>,
    pub traits: Table<CharacterTraitRecord>,
    pub trait_categories: Table<TraitCategoryRecord>,
    pub trait_info: Table<TraitInfo>,
    pub antitraits: Table<TraitAntitrait>,
    pub trait_agents: Table<TraitIncludedAgent>,
    pub ancillaries: Table<AncillaryRecord>,
    pub ancillary_agents: Table<AncillaryIncludedAgent>,
    pub ancillary_excluded: Table<AncillaryExcluded>,
    pub ancillary_subcultures: Table<AncillarySubculture>,
    pub subcultures: Table<SubcultureRecord>,
    pub cultures: Table<CultureRecord>,
    pub agent_start_attributes: Table<AgentToAgentAttribute>,
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
