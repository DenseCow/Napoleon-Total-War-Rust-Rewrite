//! The round start's historical events data ([`ntw_sim::campaign::historical_events`]), built from the
//! merged database and the localisation (CAMPAIGN_FIDELITY.md "Historical events"; CONFIRMED by
//! disassembly). Which events a round picks is the campaign script's (its `HistoricalEvents`
//! handlers, run by `ntw_script`).
//!
//! The exe builds an `events` record (`0x00F74A90`) from each row in the converter `0x00F875E0`,
//! which drops the row when:
//! - its title `events_onscreen_name_<key>` or its text `events_event_text_<key>` is not in the
//!   localisation;
//! - its season (#4) is none of `season_summer`, `season_winter`, `season_spring`, `season_autumn`
//!   (`0x00F55970` compares the whole key, case-sensitive);
//! - one of its rows in `events_effect_group_junct` or `events_view_group_junct` names a key not in
//!   `groupings`, one in `events_hist_chars_junct` a key not in `historical_characters`, or one in
//!   `events_to_policies_junction` a key not in `policies`.
//!
//! The database part is [`rules`] (called by [`crate::rules`]); the localisation part is
//! [`attach`] (called when a campaign opens).

use std::collections::BTreeMap;

use ntw_data::GameDatabase;
use ntw_formats::loc::Localisation;
use ntw_sim::campaign::historical_events::{EventGrouping, HistoricalEventRule, HistoricalEventRules};
use ntw_sim::campaign::CampaignModel;

/// The season keys `0x00F55970` knows (any other key gives 4: no season, and the record is dropped).
const SEASONS: [&str; 4] = ["season_summer", "season_winter", "season_spring", "season_autumn"];

/// The text that means "no message" (`0x008F5990` posts an event's message only when its text is
/// not this).
const NO_TEXT: &str = "n";

/// The localisation key of an event's title.
fn title_key(key: &str) -> String {
    format!("events_onscreen_name_{key}")
}

/// The localisation key of an event's text.
fn text_key(key: &str) -> String {
    format!("events_event_text_{key}")
}

/// The events and groupings from the database: every row whose season is a season and whose
/// junction rows all name known keys (the localisation part is [`attach`]'s). Each dropped row is
/// logged once, with the reason.
pub fn rules(db: &GameDatabase) -> HistoricalEventRules {
    let t = &db.campaign.events;
    let mut events = Vec::new();
    for e in t.events.iter() {
        let effect_groups: Vec<String> = t.effect_groups.iter().filter(|j| j.event == e.key).map(|j| j.grouping.clone()).collect();
        let view_groups: Vec<String> = t.view_groups.iter().filter(|j| j.event == e.key).map(|j| j.grouping.clone()).collect();
        let unknown = if !SEASONS.contains(&e.season.as_str()) {
            Some(format!("season {:?} is not a season", e.season))
        } else if let Some(g) = effect_groups.iter().chain(&view_groups).find(|g| t.groupings.get(g).is_none()) {
            Some(format!("grouping {g:?} is not in groupings"))
        } else if let Some(j) = t.historical_characters.iter().find(|j| j.event == e.key && db.campaign.historical_characters.get(&j.character).is_none()) {
            Some(format!("historical character {:?} is not in historical_characters", j.character))
        } else {
            t.policies.iter().find(|j| j.event == e.key && t.policy_keys.get(&j.policy).is_none()).map(|j| format!("policy {:?} is not in policies", j.policy))
        };
        if let Some(why) = unknown {
            log::warn!("events: {} is dropped, as the original drops it: its {why}", e.key);
            continue;
        }
        events.push(HistoricalEventRule {
            key: e.key.clone(),
            once: e.once,
            picture: e.picture.clone(),
            effect_groups,
            view_groups,
            // Set by `attach` from the localisation.
            posts_message: false,
        });
    }
    let mut groupings: BTreeMap<String, EventGrouping> = t.groupings.iter().map(|g| (g.key.clone(), EventGrouping::default())).collect();
    // A junction row of a grouping not in `groupings` is not read: no event can name that grouping.
    let mut add = |grouping: &str, list: fn(&mut EventGrouping) -> &mut Vec<String>, member: &str| {
        if let Some(g) = groupings.get_mut(grouping) {
            list(g).push(member.to_owned());
        }
    };
    for j in t.grouping_cultures.iter() {
        add(&j.grouping, |g| &mut g.cultures, &j.member);
    }
    for j in t.grouping_empires.iter() {
        add(&j.grouping, |g| &mut g.empires, &j.member);
    }
    for j in t.grouping_factions.iter() {
        add(&j.grouping, |g| &mut g.factions, &j.member);
    }
    for j in t.grouping_regions.iter() {
        add(&j.grouping, |g| &mut g.regions, &j.member);
    }
    for j in t.grouping_continents.iter() {
        add(&j.grouping, |g| &mut g.continents, &j.member);
    }
    for j in t.grouping_subcultures.iter() {
        add(&j.grouping, |g| &mut g.subcultures, &j.member);
    }
    let region_continents = db.regions.iter().map(|r| (r.key.clone(), r.continent.clone())).collect();
    HistoricalEventRules { events, groupings, region_continents }
}

/// Finishes the model's events with the localisation: drops the events whose title or text is
/// missing and marks the ones whose text is not `n` as posting their message. A missing
/// localisation drops every event (as the original, which finds no text); the dropped events are
/// logged once.
pub fn attach(model: &mut CampaignModel, loc: Option<&Localisation>) {
    let rules = &mut std::sync::Arc::make_mut(&mut model.rules).historical_events;
    let mut dropped = Vec::new();
    rules.events.retain_mut(|e| {
        let text = loc.and_then(|l| Some((l.get(&title_key(&e.key))?, l.get(&text_key(&e.key))?)));
        match text {
            Some((_, text)) => {
                e.posts_message = text != NO_TEXT;
                true
            }
            None => {
                dropped.push(e.key.clone());
                false
            }
        }
    });
    if !dropped.is_empty() {
        log::warn!("Historical events: {dropped:?} have no title or text in the localisation and are dropped, as the original drops them");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MADE-UP rows: one event per drop reason, and one kept, with its junction rows in file order.
    #[test]
    fn rows_with_unknown_keys_or_no_season_are_dropped() {
        use ntw_data::events::*;
        use ntw_data::record::Table;
        let mut db = GameDatabase::test_fixture();
        let t = &mut db.campaign.events;
        let event = |key: &str, season: &str| EventRecord {
            key: key.into(),
            picture_1: String::new(),
            once: false,
            year: 1805,
            season: season.into(),
            unknown_5: None,
            picture: "pic.tga".into(),
            turn_of_year: 3,
        };
        t.events = Table::from_rows(1, vec![event("kept", "season_winter"), event("bad_season", "Season_Winter"), event("bad_group", "season_summer"), event("bad_policy", "season_summer")]);
        t.groupings = Table::from_rows(1, vec![GroupingRecord { key: "g1".into() }, GroupingRecord { key: "g2".into() }]);
        t.view_groups = Table::from_rows(1, vec![
            EventViewGroup { event: "kept".into(), grouping: "g2".into() },
            EventViewGroup { event: "kept".into(), grouping: "g1".into() },
            EventViewGroup { event: "bad_group".into(), grouping: "nowhere".into() },
        ])
        ;
        t.effect_groups = Table::from_rows(1, vec![EventEffectGroup { event: "kept".into(), grouping: "g1".into() }]);
        t.policies = Table::from_rows(1, vec![EventPolicy { event: "bad_policy".into(), policy: "none".into() }]);
        t.grouping_factions = Table::from_rows(1, vec![GroupingFaction { grouping: "g1".into(), member: "f".into() }, GroupingFaction { grouping: "gone".into(), member: "f".into() }]);
        t.grouping_continents = Table::from_rows(1, vec![GroupingContinent { grouping: "g2".into(), member: "c".into() }]);
        let r = rules(&db);
        assert_eq!(r.events.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["kept"]);
        let e = &r.events[0];
        assert_eq!((e.picture.as_str(), e.once), ("pic.tga", false));
        assert_eq!(e.view_groups, ["g2", "g1"], "junction rows in file order");
        assert_eq!(e.effect_groups, ["g1"]);
        assert_eq!(r.groupings.keys().collect::<Vec<_>>(), ["g1", "g2"]);
        assert_eq!(r.groupings["g1"].factions, ["f"]);
        assert_eq!(r.groupings["g2"].continents, ["c"]);
    }

    /// MADE-UP localisation: an event without its text is dropped; a text of `n` posts no message.
    #[test]
    fn the_localisation_drops_events_and_n_posts_nothing() {
        let mut model = CampaignModel::new(ntw_sim::calendar::Calendar::new(ntw_sim::calendar::Date { year: 1805, season: 1, month: 0, half: 0 }, 0), ntw_sim::rng::CaRng::new(7), Default::default());
        let rule = |key: &str| HistoricalEventRule { key: key.into(), ..Default::default() };
        std::sync::Arc::make_mut(&mut model.rules).historical_events.events = vec![rule("a"), rule("b"), rule("c")];
        let mut loc = Localisation::new();
        let mut file = ntw_formats::loc::LocFile { version: 1, entries: Vec::new() };
        for (k, v) in [("events_onscreen_name_a", "A"), ("events_event_text_a", "Text"), ("events_onscreen_name_b", "B"), ("events_event_text_b", "n"), ("events_onscreen_name_c", "C")] {
            file.entries.push(ntw_formats::loc::LocEntry { key: k.into(), text: v.into(), flag: false });
        }
        loc.add(&file);
        attach(&mut model, Some(&loc));
        let r = &model.rules.historical_events;
        assert_eq!(r.events.iter().map(|e| (e.key.as_str(), e.posts_message)).collect::<Vec<_>>(), [("a", true), ("b", false)]);
        attach(&mut model, None);
        assert!(model.rules.historical_events.events.is_empty(), "no localisation: no event");
    }
}
