//! The round start's historical events (CAMPAIGN_FIDELITY.md "Historical events"; CONFIRMED by
//! disassembly unless a line says otherwise).
//!
//! At each round start, after the trade update, `0x008F5990` (the model's historical event manager,
//! model +0xF84) gathers the round's candidate events and gives each human faction its share of them:
//!
//! 1. The candidates: the manager posts the `HistoricalEvents` script event (`0x0087F960`); each handler
//!    asks `conditions.IsTriggerableHistoricalEvent` (`0x008A1DC0`) and, when it passes, pushes the
//!    event with `effect.historical_event` (`0x008C97B0`). The condition finds the `events` record (none:
//!    false), builds the date `year` + the turn of year (`0x008EDC80` with `(turn − 1) · 48 /
//!    turns_per_year` quarter-months, [`event_date`]) and compares it with the calendar's whole date
//!    (`0x00898AF0`); a once-only event also enters the manager's triggered list (`0x008BBC50`), and an
//!    event already listed fails. So the condition lists the key even when no faction gets the event.
//!    The vanilla script (`export_historic_events.lua`, required by every campaign's `scripting.lua`
//!    but the tutorial's) has one handler per `events` row, in table order, with the row's own year and
//!    turn of year; the model runs the same check over the table ([`HistoricalEventRules::scripted`]
//!    says whether the campaign's script registers them).
//! 2. For each listed faction (the world's turn list) that is human (+0x6E0): each candidate, in order,
//!    goes to a "forced" bucket by the event's effect groupings ([`grouping_priority`]), else to a
//!    "random" bucket by its view groupings, else nowhere. Forced events: the buckets' events in bucket
//!    order, the first 4 of them. Otherwise one event of the first non-empty random bucket, drawn with
//!    the campaign RNG (+0xFB8, the `0x005F0830` draw inlined at `0x008F5F58`,
//!    [`crate::rng::CaRng::uniform_below`]): one draw per human faction that has random candidates and
//!    no forced one, none otherwise.
//! 3. Each chosen event whose text is not `n` posts its message (`0x00A06DA0`, model +0xFA0) to the
//!    faction ([`CampaignMessage::HistoricalEvent`]). The event's effects are not applied (the exe
//!    fills its effect text with "Not Implemented: displaying event effects").
//!
//! The model calls it through the [`events.historical`](super::seams::HISTORICAL_EVENTS) seam
//! ([`CampaignModel::historical_events_round_start`]).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::events::{CampaignEvent, CampaignMessage};
use super::ids::FactionId;
use super::mod_state::ModWrites;
use super::world::CampaignModel;
use crate::calendar::{Date, QUARTERS_PER_YEAR};

/// The matcher's "no grouping matches" value (`0x008C98C0` starts at 13).
pub const NO_MATCH: u8 = 13;
/// The buckets per kind (forced, random) `0x008F5990` keeps: indexes 0..12 of the matcher's values.
const BUCKETS: usize = 12;
/// The most forced events one faction receives in a round (`0x008F5990`).
pub const MAX_FORCED: usize = 4;

/// A grouping (`groupings` and its `groupings_*_junct` rows, the exe's `GROUPING_RECORD`, constructor
/// `0x00F41480`): the lists in junction row order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EventGrouping {
    /// `groupings_cultures_junct` (record +0x10).
    pub cultures: Vec<String>,
    /// `groupings_empires_junct` (record +0x20): never matched (see [`grouping_priority`]).
    pub empires: Vec<String>,
    /// `groupings_factions_junct` (record +0x30).
    pub factions: Vec<String>,
    /// `groupings_regions_junct` (record +0x40).
    pub regions: Vec<String>,
    /// `groupings_continents_junct` (record +0x50).
    pub continents: Vec<String>,
    /// `groupings_subcultures_junct` (record +0x60).
    pub subcultures: Vec<String>,
}

/// One `events` record (the exe's `EVENT_RECORD`, constructor `0x00F74A90`), as far as the round
/// start reads it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistoricalEventRule {
    /// `events` #0.
    pub key: String,
    /// `events` #2 (record +0x18): once only; also the matcher's flag ([`grouping_priority`]).
    pub once: bool,
    /// `events` #3: the year.
    pub year: i32,
    /// `events` #7: the turn of the year, from 1.
    pub turn_of_year: i32,
    /// `events` #6 (record +0x38): the message picture, under `ui/eventpics/<recipient's culture>/`
    /// (`0x00F840E0`).
    pub picture: String,
    /// `events_effect_group_junct` groupings, in row order (record +0x48): the forced buckets.
    pub effect_groups: Vec<String>,
    /// `events_view_group_junct` groupings, in row order (record +0x78): the random buckets.
    pub view_groups: Vec<String>,
    /// The event's text (`events_event_text_<key>`) is not `n`: only then is the message posted.
    pub posts_message: bool,
}

/// The historical events' data (built by `ntw_campaign` from the merged database).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistoricalEventRules {
    /// The `events` records the exe keeps, in table order (a record is dropped when its title or text
    /// is missing, its season is not a season, or a junction names an unknown key: `0x00F875E0`).
    pub events: Vec<HistoricalEventRule>,
    /// The groupings by key.
    pub groupings: BTreeMap<String, EventGrouping>,
    /// `regions` #1, the continent, by region key.
    pub region_continents: BTreeMap<String, String>,
    /// The campaign's script registers the round start's `HistoricalEvents` handlers (its
    /// `scripting.lua` requires `export_historic_events`): without them no event is a candidate.
    pub scripted: bool,
}

impl HistoricalEventRules {
    /// The record with this key (`0x008C9850`).
    pub fn event(&self, key: &str) -> Option<&HistoricalEventRule> {
        self.events.iter().find(|e| e.key == key)
    }
}

/// The date `IsTriggerableHistoricalEvent` (`0x008A1DC0`) gives an event: `0x008EDC80` with the year
/// and `(turn_of_year − 1) · 48 / turns_per_year` quarter-months (32-bit, the division unsigned; a
/// result of 48 or more is January's first quarter, [`Date::set_quarter`]). `None` for a calendar of
/// 0 turns a year, where the exe's division faults.
pub fn event_date(year: i32, turn_of_year: i32, turns_per_year: u32) -> Option<Date> {
    if turns_per_year == 0 {
        return None;
    }
    let quarters = (turn_of_year.wrapping_sub(1).wrapping_mul(QUARTERS_PER_YEAR as i32) as u32) / turns_per_year;
    let mut date = Date { year: 0, season: 0, month: 0, half: 0 };
    date.set_quarter(year as u32, quarters);
    Some(date)
}

/// The bucket a faction puts an event in for `groupings` (`0x008C98C0`): the best (lowest) value of the
/// groupings that name the faction, [`NO_MATCH`] when none does. Each grouping in order, its lists in
/// this order, each value with `flag` (the event's once flag) / without:
/// - continents: one of the faction's regions lies on a listed continent: 4 / 10;
/// - cultures: the culture of the faction's subculture is listed (`0x008F3490`: faction record +0x10, its
///   subculture, whose +0x10 is the culture): 1 / 7;
/// - empires: walked but never tested (an empty loop at `0x008C9AC3`), so they never match: an empire
///   record (`0x00F873D0`) holds only a key, a name and regions, with no link to a faction, so no rule is
///   lost to restore (not an ORIGINAL BUG; vanilla has no `groupings_empires_junct` rows);
/// - factions: the faction is listed: 3 / 9;
/// - regions: the faction owns a listed region: 5 / 11, set even when the value so far is better (the
///   exe does not compare here, so a later regions match can worsen it);
/// - subcultures: the faction's subculture is listed: 2 / 8.
pub fn grouping_priority(model: &CampaignModel, faction: FactionId, groupings: &[String], flag: bool) -> u8 {
    let Some(f) = model.world.factions.get(&faction) else { return NO_MATCH };
    let rules = &model.rules;
    let culture = rules.faction_cultures.get(&f.key).map(String::as_str);
    let subculture = rules.faction_subcultures.get(&f.key).map(String::as_str);
    let owned = || model.world.regions.values().filter(move |r| r.owner == faction);
    let value = |with: u8, without: u8| if flag { with } else { without };
    let mut best = NO_MATCH;
    let better = |best: &mut u8, v: u8| *best = (*best).min(v);
    for key in groupings {
        let Some(g) = rules.historical_events.groupings.get(key) else { continue };
        for continent in &g.continents {
            if owned().any(|r| rules.historical_events.region_continents.get(&r.key) == Some(continent)) {
                better(&mut best, value(4, 10));
            }
        }
        if g.cultures.iter().any(|c| Some(c.as_str()) == culture) {
            better(&mut best, value(1, 7));
        }
        if g.factions.contains(&f.key) {
            better(&mut best, value(3, 9));
        }
        if g.regions.iter().any(|key| owned().any(|r| &r.key == key)) {
            best = value(5, 11);
        }
        if g.subcultures.iter().any(|s| Some(s.as_str()) == subculture) {
            better(&mut best, value(2, 8));
        }
    }
    best
}

/// The order of the triggered list (`0x008BBC50`'s binary search, `CompareUniStrings`: case-sensitive,
/// by UTF-16 code unit).
fn utf16_order(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// A triggered list read from elsewhere (a save) in the list's own order, each key once (the exe's
/// loader `0x0087F980` inserts each saved key with `0x008BBC50`).
pub fn sorted_triggered(keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut list = Vec::new();
    for k in keys {
        mark_triggered(&mut list, &k);
    }
    list
}

/// Lists `key` in the triggered list (`0x008BBC50`): false when it was listed already.
fn mark_triggered(list: &mut Vec<String>, key: &str) -> bool {
    match list.binary_search_by(|k| utf16_order(k, key)) {
        Ok(_) => false,
        Err(i) => {
            list.insert(i, key.to_owned());
            true
        }
    }
}

/// The round's candidates: the `events` records whose condition passes, in table order (see the
/// module docs, step 1). Lists each once-only candidate in the triggered list.
///
/// PROVISIONAL: stands in for the original running the `HistoricalEvents` script event through the script
/// host (0x0087F960); the vanilla handlers' condition is evaluated here directly, so a mod's own
/// `HistoricalEvents` handlers are ignored until the script host posts that event.
fn candidates(model: &mut CampaignModel) -> Vec<usize> {
    let rules = std::sync::Arc::clone(&model.rules);
    let events = &rules.historical_events;
    if !events.scripted {
        return Vec::new();
    }
    let calendar = model.calendar;
    let mut out = Vec::new();
    for (i, e) in events.events.iter().enumerate() {
        if event_date(e.year, e.turn_of_year, calendar.turns_per_year) != Some(calendar.date) {
            continue;
        }
        if e.once && !mark_triggered(&mut model.world.historical_events_triggered, &e.key) {
            continue;
        }
        out.push(i);
    }
    out
}

/// The original's round-start historical events (`0x008F5990`, module docs): the messages to post.
pub fn historical_events(model: &mut CampaignModel, _writes: &mut ModWrites) -> Vec<CampaignEvent> {
    let found = candidates(model);
    let mut out = Vec::new();
    if found.is_empty() {
        return out;
    }
    let rules = std::sync::Arc::clone(&model.rules);
    let events = &rules.historical_events.events;
    for faction in model.world.factions_in_turn_order() {
        if !model.turn.humans.contains(&faction) {
            continue;
        }
        let mut forced: [Vec<usize>; BUCKETS] = Default::default();
        let mut random: [Vec<usize>; BUCKETS] = Default::default();
        for &i in &found {
            let e = &events[i];
            let p = grouping_priority(model, faction, &e.effect_groups, e.once);
            if let Some(bucket) = forced.get_mut(usize::from(p)) {
                bucket.push(i);
                continue;
            }
            let p = grouping_priority(model, faction, &e.view_groups, e.once);
            if let Some(bucket) = random.get_mut(usize::from(p)) {
                bucket.push(i);
            }
        }
        let chosen: Vec<usize> = if forced.iter().any(|b| !b.is_empty()) {
            forced.iter().flatten().copied().take(MAX_FORCED).collect()
        } else if let Some(bucket) = random.iter().find(|b| !b.is_empty()) {
            // ORIGINAL BUG: the draw never ends for more than 0xFFFF events (inlined CaRng::uniform_below,
            // 0x008F5F58); ours draws among the first 0xFFFF (no such count exists in any data).
            let n = u32::try_from(bucket.len()).unwrap_or(u32::MAX).min(0xFFFF);
            vec![bucket[model.rng.uniform_below(n) as usize]]
        } else {
            Vec::new()
        };
        for i in chosen {
            let e = &events[i];
            if e.posts_message {
                out.push(CampaignEvent::Message { recipient: faction, message: CampaignMessage::HistoricalEvent { event: e.key.clone() } });
            }
        }
    }
    out
}

impl CampaignModel {
    /// The round start's historical events through the
    /// [`events.historical`](super::seams::HISTORICAL_EVENTS) seam: the messages go to `events`.
    pub fn historical_events_round_start(&mut self, events: &mut Vec<CampaignEvent>) {
        let mut writes = ModWrites::default();
        let rules = std::sync::Arc::clone(&self.rules);
        events.extend(rules.seams.historical_events.rule()(self, &mut writes));
        self.mod_state.apply(&mut writes);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::campaign::tests::test_model;

    const A: FactionId = FactionId(1); // test_model's faction A
    const A_KEY: &str = "test_faction_a";

    /// A MADE-UP event on 24 turns a year: the date of the test model's calendar.
    fn event(key: &str, effect: &[&str], view: &[&str], once: bool, model: &CampaignModel) -> HistoricalEventRule {
        let d = model.calendar.date;
        HistoricalEventRule {
            key: key.into(),
            once,
            year: d.year as i32,
            turn_of_year: (d.turn_in_year() * model.calendar.turns_per_year / 24) as i32 + 1,
            picture: "p.tga".into(),
            effect_groups: effect.iter().map(|s| (*s).to_owned()).collect(),
            view_groups: view.iter().map(|s| (*s).to_owned()).collect(),
            posts_message: true,
        }
    }

    fn grouping(factions: &[&str]) -> EventGrouping {
        EventGrouping { factions: factions.iter().map(|s| (*s).to_owned()).collect(), ..Default::default() }
    }

    /// test_model with faction A human, its calendar on a date event_date can give, and `events`.
    fn model_with(events: impl FnOnce(&CampaignModel) -> Vec<HistoricalEventRule>, groupings: Vec<(&str, EventGrouping)>) -> CampaignModel {
        let mut m = test_model();
        m.calendar.turns_per_year = 24;
        m.calendar.date = event_date(1806, 11, 24).expect("24 turns");
        m.turn.humans = vec![A];
        let mut rules = (*m.rules).clone();
        rules.historical_events = HistoricalEventRules {
            events: events(&m),
            groupings: groupings.into_iter().map(|(k, g)| (k.to_owned(), g)).collect(),
            region_continents: BTreeMap::new(),
            scripted: true,
        };
        m.rules = Arc::new(rules);
        m
    }

    fn messages(events: &[CampaignEvent]) -> Vec<(FactionId, &str)> {
        events
            .iter()
            .filter_map(|e| match e {
                CampaignEvent::Message { recipient, message: CampaignMessage::HistoricalEvent { event } } => Some((*recipient, event.as_str())),
                _ => None,
            })
            .collect()
    }

    /// `0x008A1DC0`'s date: `(turn − 1) · 48 / turns_per_year` quarter-months into the year.
    #[test]
    fn the_event_date_follows_the_turn_of_the_year() {
        let d = event_date(1812, 12, 24).unwrap();
        assert_eq!((d.year, d.month, d.half), (1812, 5, 2), "turn 12 of 24: late June");
        let d = event_date(1805, 24, 24).unwrap();
        assert_eq!((d.year, d.month, d.half), (1805, 11, 2), "turn 24: late December");
        let d = event_date(1806, 1, 12).unwrap();
        assert_eq!((d.month, d.half), (0, 0));
        // Turn 0: (−48 as u32) / 24 is past the year, so January's first quarter.
        let d = event_date(1806, 0, 24).unwrap();
        assert_eq!((d.month, d.half), (0, 0));
        assert_eq!(event_date(1806, 3, 0), None);
    }

    /// `0x008C98C0`: the best value of the groupings naming the faction, by kind and flag; regions set
    /// their value without comparing; empires never match.
    #[test]
    fn the_matcher_takes_the_best_grouping_and_regions_overwrite() {
        let mut m = model_with(|_| Vec::new(), Vec::new());
        let key = m.world.factions[&A].key.clone();
        let region = m.world.regions.values().find(|r| r.owner == A).expect("A owns a region").key.clone();
        let mut rules = (*m.rules).clone();
        rules.faction_cultures.insert(key.clone(), "c".into());
        rules.faction_subcultures.insert(key.clone(), "s".into());
        rules.historical_events.region_continents.insert(region.clone(), "cont".into());
        let g = |f: fn(&mut EventGrouping)| {
            let mut g = EventGrouping::default();
            f(&mut g);
            g
        };
        let gs = &mut rules.historical_events.groupings;
        gs.insert("cont".into(), g(|g| g.continents = vec!["cont".into()]));
        gs.insert("cult".into(), g(|g| g.cultures = vec!["c".into()]));
        gs.insert("emp".into(), g(|g| g.empires = vec!["anything".into()]));
        gs.insert("fac".into(), grouping(&[&key]));
        gs.insert("reg".into(), EventGrouping { regions: vec![region.clone()], ..EventGrouping::default() });
        gs.insert("sub".into(), g(|g| g.subcultures = vec!["s".into()]));
        gs.insert("other".into(), grouping(&["nobody"]));
        m.rules = Arc::new(rules);
        let p = |names: &[&str], flag: bool| grouping_priority(&m, A, &names.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(), flag);
        assert_eq!((p(&["cont"], true), p(&["cont"], false)), (4, 10));
        assert_eq!((p(&["cult"], true), p(&["cult"], false)), (1, 7));
        assert_eq!((p(&["fac"], true), p(&["fac"], false)), (3, 9));
        assert_eq!((p(&["reg"], true), p(&["reg"], false)), (5, 11));
        assert_eq!((p(&["sub"], true), p(&["sub"], false)), (2, 8));
        assert_eq!((p(&["emp"], true), p(&["emp"], false)), (NO_MATCH, NO_MATCH), "empires never match");
        assert_eq!(p(&["other", "missing"], false), NO_MATCH);
        assert_eq!(p(&["reg", "fac"], false), 9, "a better value replaces regions'");
        assert_eq!(p(&["cult", "reg"], false), 11, "regions overwrite a better value");
        assert_eq!(p(&["cult", "sub"], false), 7, "a worse value does not replace");
    }

    /// Forced events: the first 4 in bucket order, and no draw.
    #[test]
    fn forced_events_take_the_first_four_without_a_draw() {
        let mut m = model_with(
            |m| (0..6).map(|i| event(&format!("e{i}"), &["all"], &["all"], false, m)).collect(),
            vec![("all", grouping(&[A_KEY]))],
        );
        let rng = m.rng;
        let mut out = Vec::new();
        m.historical_events_round_start(&mut out);
        assert_eq!(messages(&out), [(A, "e0"), (A, "e1"), (A, "e2"), (A, "e3")]);
        assert_eq!(m.rng, rng, "no draw");
    }

    /// Random events: one draw per human faction from the first non-empty bucket; the AI factions get
    /// none and draw nothing; no candidate, no draw.
    #[test]
    fn a_random_event_is_drawn_once_per_human_faction() {
        let mut m = model_with(|m| (0..3).map(|i| event(&format!("e{i}"), &[], &["all"], false, m)).collect(), vec![("all", grouping(&[A_KEY]))]);
        let mut expected_rng = m.rng;
        let pick = expected_rng.uniform_below(3) as usize;
        let mut out = Vec::new();
        m.historical_events_round_start(&mut out);
        let picked = format!("e{pick}");
        assert_eq!(messages(&out), [(A, picked.as_str())]);
        assert_eq!(m.rng, expected_rng, "exactly one draw");
        // Next round's date matches no event: no candidate, no draw.
        m.calendar.advance_turn(None);
        let rng = m.rng;
        let mut out = Vec::new();
        m.historical_events_round_start(&mut out);
        assert!(out.is_empty());
        assert_eq!(m.rng, rng);
    }

    /// A once-only event enters the triggered list (UTF-16 sorted) and does not fire again; an
    /// event without text posts nothing but is still chosen; a campaign without the script gets none.
    #[test]
    fn once_only_events_fire_once_and_unscripted_campaigns_get_none() {
        let mut m = model_with(
            |m| {
                let mut quiet = event("b_quiet", &["all"], &[], true, m);
                quiet.posts_message = false;
                vec![event("a_once", &["all"], &[], true, m), quiet]
            },
            vec![("all", grouping(&[A_KEY]))],
        );
        let mut out = Vec::new();
        m.historical_events_round_start(&mut out);
        assert_eq!(messages(&out), [(A, "a_once")]);
        assert_eq!(m.world.historical_events_triggered, ["a_once", "b_quiet"]);
        let mut out = Vec::new();
        m.historical_events_round_start(&mut out);
        assert!(out.is_empty(), "listed events fail the condition");
        // Without the script's handlers no event is a candidate.
        let mut m = model_with(|m| vec![event("e", &["all"], &[], false, m)], vec![("all", grouping(&[A_KEY]))]);
        let mut rules = (*m.rules).clone();
        rules.historical_events.scripted = false;
        m.rules = Arc::new(rules);
        let mut out = Vec::new();
        m.historical_events_round_start(&mut out);
        assert!(out.is_empty());
    }

    /// The triggered list's order is `CompareUniStrings`' (UTF-16 code units), not UTF-8 bytes.
    #[test]
    fn the_triggered_list_sorts_by_utf16_code_unit() {
        let mut list = Vec::new();
        assert!(mark_triggered(&mut list, "\u{FF21}"));
        assert!(mark_triggered(&mut list, "\u{1F600}"));
        assert!(mark_triggered(&mut list, "B"));
        assert!(!mark_triggered(&mut list, "B"));
        assert_eq!(list, ["B", "\u{1F600}", "\u{FF21}"], "a surrogate pair sorts below U+FF21");
    }
}
