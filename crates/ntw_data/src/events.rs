//! The historical events' tables (`events` and its junctions) and the groupings they name
//! (`groupings_*_junct`). The round-start historical events (`ntw_sim::campaign::historical_events`,
//! CAMPAIGN_FIDELITY.md "Historical events") read them. Layouts: `analysis/worker1/DB_BUILDERS.md`
//! (`events_tables` row reader `0x00F70B70`; every junction here is read by the two-string row
//! reader `0x00DD2940`, CONFIRMED); each parses to its end on the install (`tests/real_install.rs`).

use crate::record::{db_record, Table};

db_record! {
    /// `events` (`ssbisosi`, 8 rows, v1): the historical events. The record the exe builds from a row
    /// (`0x00F74A90`, converter `0x00F875E0`) is dropped when its title or text localisation
    /// (`events_onscreen_name_<key>`, `events_event_text_<key>`) is missing, its season is not a
    /// season key (`0x00F55970`) or a junction names an unknown key (CONFIRMED).
    pub struct EventRecord in "events", key = key {
        /// #0: the key, e.g. `1812_war`.
        key: String,
        /// #1: a picture (`us-nation emerges.tga`; the same as #6 in every vanilla row; not read by
        /// the round-start events).
        picture_1: String,
        /// #2: once only (event record +0x18): the condition `IsTriggerableHistoricalEvent`
        /// (`0x008A1DC0`) adds the key to the triggered list and refuses a key already there
        /// (CONFIRMED; false in every vanilla row).
        once: bool,
        /// #3: the year the event falls in.
        year: i32,
        /// #4: the season key (`season_summer`); not read by the condition (its season argument is
        /// unused, CONFIRMED), the date comes from #3 and #7.
        season: String,
        /// #5: an optional string (`true` in every vanilla row; meaning UNKNOWN, not read by the
        /// round-start events).
        unknown_5: Option<String>,
        /// #6: the message picture (record +0x38): the message's picture is
        /// `ui/eventpics/<recipient's culture>/<this>` (`0x00F840E0`, CONFIRMED).
        picture: String,
        /// #7 {v>=1}: the turn of the year the event falls on, counted from 1 (the vanilla script
        /// passes it as the condition's first argument: CONFIRMED equal for all 8 rows).
        turn_of_year: i32 => since 1,
    }
}

db_record! {
    /// `events_view_group_junct` (`ss`, 8 rows): the groupings that may receive an event as a random
    /// event (event record +0x78).
    pub struct EventViewGroup in "events_view_group_junct", key = event + grouping {
        /// #0: the event key.
        event: String,
        /// #1: the `groupings` key.
        grouping: String,
    }
}

db_record! {
    /// `events_effect_group_junct` (`ss`, empty in vanilla): the groupings that always receive an event
    /// (event record +0x48: the "forced" buckets).
    pub struct EventEffectGroup in "events_effect_group_junct", key = event + grouping {
        /// #0: the event key.
        event: String,
        /// #1: the `groupings` key.
        grouping: String,
    }
}

db_record! {
    /// `groupings_factions_junct` (`ss`, 9 rows): a grouping's factions (grouping record +0x30).
    pub struct GroupingFaction in "groupings_factions_junct", key = grouping + member {
        /// #0: the `groupings` key.
        grouping: String,
        /// #1: the faction key.
        member: String,
    }
}

db_record! {
    /// `groupings_cultures_junct` (`ss`, empty in vanilla): a grouping's cultures (grouping record +0x10).
    pub struct GroupingCulture in "groupings_cultures_junct", key = grouping + member {
        /// #0: the `groupings` key.
        grouping: String,
        /// #1: the culture key.
        member: String,
    }
}

db_record! {
    /// `groupings_subcultures_junct` (`ss`, empty in vanilla): a grouping's subcultures (record +0x60).
    pub struct GroupingSubculture in "groupings_subcultures_junct", key = grouping + member {
        /// #0: the `groupings` key.
        grouping: String,
        /// #1: the subculture key.
        member: String,
    }
}

db_record! {
    /// `groupings_regions_junct` (`ss`, empty in vanilla): a grouping's regions (record +0x40).
    pub struct GroupingRegion in "groupings_regions_junct", key = grouping + member {
        /// #0: the `groupings` key.
        grouping: String,
        /// #1: the region key.
        member: String,
    }
}

db_record! {
    /// `groupings_continents_junct` (`ss`, 2 rows): a grouping's continents (record +0x50).
    pub struct GroupingContinent in "groupings_continents_junct", key = grouping + member {
        /// #0: the `groupings` key.
        grouping: String,
        /// #1: the continent key (`regions` #1).
        member: String,
    }
}

db_record! {
    /// `groupings_empires_junct` (`ss`, empty in vanilla): a grouping's empires (record +0x20; the
    /// exe's matcher never tests them, CAMPAIGN_FIDELITY.md "Historical events").
    pub struct GroupingEmpire in "groupings_empires_junct", key = grouping + member {
        /// #0: the `groupings` key.
        grouping: String,
        /// #1: the empire key.
        member: String,
    }
}

db_record! {
    /// `events_hist_chars_junct` (`ss`, empty in vanilla): an event's historical characters (event
    /// record's second list); only checked when the record is built: a key not in
    /// `historical_characters` drops the event (`0x00F875E0`, CONFIRMED).
    pub struct EventHistoricalCharacter in "events_hist_chars_junct", key = event + character {
        /// #0: the event key.
        event: String,
        /// #1: the `historical_characters` key.
        character: String,
    }
}

db_record! {
    /// `events_to_policies_junction` (`ss`, empty in vanilla): an event's policies (event record's
    /// third list); only checked when the record is built: a key not in `policies` drops the event
    /// (`0x00F875E0`, CONFIRMED).
    pub struct EventPolicy in "events_to_policies_junction", key = event + policy {
        /// #0: the event key.
        event: String,
        /// #1: the `policies` key.
        policy: String,
    }
}

db_record! {
    /// `groupings` (`s`, 25 rows): the grouping keys. A junction naming a grouping not here drops
    /// its event (`0x00F875E0`, CONFIRMED).
    pub struct GroupingRecord in "groupings", key = key {
        /// #0: the key, e.g. `all_factions`.
        key: String,
    }
}

db_record! {
    /// `policies` (`ss`, 2 rows): the policy keys (only their keys are read here, to check
    /// [`EventPolicy`] rows).
    pub struct PolicyRecord in "policies", key = key {
        /// #0: the key, e.g. `religious_tolerance`.
        key: String,
        /// #1: a string (`n` in both vanilla rows; meaning UNKNOWN, not read).
        unknown_1: String,
    }
}

/// The historical events' tables. Empty in [`crate::GameDatabase::test_fixture`].
#[derive(Debug, Clone, Default)]
pub struct EventTables {
    /// `events`.
    pub events: Table<EventRecord>,
    /// `events_view_group_junct`.
    pub view_groups: Table<EventViewGroup>,
    /// `events_effect_group_junct`.
    pub effect_groups: Table<EventEffectGroup>,
    /// `events_hist_chars_junct`.
    pub historical_characters: Table<EventHistoricalCharacter>,
    /// `events_to_policies_junction`.
    pub policies: Table<EventPolicy>,
    /// `groupings`.
    pub groupings: Table<GroupingRecord>,
    /// `policies` (the policy keys).
    pub policy_keys: Table<PolicyRecord>,
    /// `groupings_factions_junct`.
    pub grouping_factions: Table<GroupingFaction>,
    /// `groupings_cultures_junct`.
    pub grouping_cultures: Table<GroupingCulture>,
    /// `groupings_subcultures_junct`.
    pub grouping_subcultures: Table<GroupingSubculture>,
    /// `groupings_regions_junct`.
    pub grouping_regions: Table<GroupingRegion>,
    /// `groupings_continents_junct`.
    pub grouping_continents: Table<GroupingContinent>,
    /// `groupings_empires_junct`.
    pub grouping_empires: Table<GroupingEmpire>,
}
