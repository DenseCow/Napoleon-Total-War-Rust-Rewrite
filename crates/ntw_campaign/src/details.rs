//! Reads the detail records into [`ntw_sim::campaign::details`]: `CHARACTER_DETAILS`, the
//! `GOVERNMENT` posts and governorships (with the taxes), the capital and religion, and the full
//! `DIPLOMACY_RELATIONSHIP` records. Layouts: `analysis/campaign/CAMPAIGN_DATA.md` §3.
//!
//! Lenient by design: a detail that does not have the expected shape is skipped (the core loader
//! already checked everything the rules depend on), so an odd modded file still loads.

use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_sim::calendar::Date;
use ntw_sim::campaign::details::{
    AlliedWar, AttitudeFactor, CharacterDetails, CharacterTrait, FactionDetails, GovernmentPost, Governorship,
    GovernorshipTaxes, Portrait, RegularPayment, Relationship,
};
use ntw_sim::campaign::{CharacterId, FactionId, RegionId, World, ZoomLimit};

fn date_of(r: Option<&EsfRecord>) -> Option<Date> {
    let r = r?;
    Some(Date { year: r.get_u32(0)?, season: r.get_u32(1)?, month: r.get_u32(2)?, half: r.get_u32(3)? })
}

fn loc(r: Option<&EsfNode>) -> String {
    r.and_then(EsfNode::as_record).and_then(|l| l.get_str(0)).unwrap_or_default().to_string()
}

fn items<'a>(r: &'a EsfRecord, name: &str) -> impl Iterator<Item = &'a [EsfNode]> + 'a {
    r.record_array(name).into_iter().flat_map(|a| a.items.iter().map(Vec::as_slice))
}

fn s(n: Option<&EsfNode>) -> String {
    n.and_then(EsfNode::as_str).unwrap_or_default().to_string()
}

/// `CAMPAIGN_MODEL/HISTORICAL_CHARACTER_MANAGER/CREATED_CHARACTER_ARRAY[]` {utf16 key}: the
/// historical characters already made, kept sorted as the exe's list is (CHARACTERS_FIDELITY.md §7,
/// §8).
pub(crate) fn historical_created(model_rec: &EsfRecord) -> Vec<String> {
    let mut created: Vec<String> = model_rec
        .child("HISTORICAL_CHARACTER_MANAGER")
        .map(|m| items(m, "CREATED_CHARACTER_ARRAY").filter_map(|it| it.first()?.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    created.sort();
    created
}

/// The `CAMPAIGN_MODEL/EPISODIC_RESTRICTIONS` switches, in this order: #8 `display_turns` (+0xD1,
/// UI_FIDELITY.md §5.1); #20 / #22 `force_assassination_success_for_human` /
/// `force_garrison_infiltration_success_for_human` (CHARACTERS_FIDELITY.md §7, §8); #25
/// `disable_saving_game` (+0x115); #29 `set_technology_research_disabled` (+0x118); #31
/// `set_liberation_options_disabled` (+0x124); #32 `set_ui_notification_of_victory_disabled`
/// (+0x125). Each is (position, the record version the loader `0x009968A0` needs to exceed to read
/// it, `None` for any); positions and gates CONFIRMED from that loader and the writer `0x009A1070`.
/// An older record leaves the flag false.
///
/// #30 is the scripts' `LUA[]` slot array, which the loader reads only when present
/// (CAMPAIGN_DATA.md §4); every shipped start position and save has it. Without it the last two
/// switches move up to #30 / #31, so their positions follow whether #30 is that array.
fn switch_slots(restrictions: &EsfRecord) -> [(usize, Option<u8>); 7] {
    let has_lua = matches!(restrictions.get(30), Some(EsfNode::RecordArray(a)) if a.name == "LUA");
    let after_lua = if has_lua { 31 } else { 30 };
    [(8, Some(1)), (20, None), (22, None), (25, Some(0xB)), (29, Some(0xE)), (after_lua, Some(0xF)), (after_lua + 1, Some(0x10))]
}

/// The world's switches in [`switch_slots`] order.
fn switch_values(world: &World) -> [bool; 7] {
    [
        world.display_turns,
        world.force_success_for_human.0,
        world.force_success_for_human.1,
        world.saving_disabled,
        world.technology_research_disabled,
        world.liberation_options_disabled,
        world.victory_notification_disabled,
    ]
}

/// The positions of the zoom limit (episodic restrictions +0xC0 smallest tilt, +0xBC largest;
/// [`ntw_sim::campaign::zoom_limit`]) in an `EPISODIC_RESTRICTIONS` record: the loader `0x009968A0`
/// reads the smallest tilt after #8 (present above version 1) and the four map bounds, so at #13
/// (#12 in a version-1 record; `0x00997B9D`), and the largest right after it only above version 2
/// (`0x00997BBA`); the writer `0x009A1070` writes both (`0x009A1957`, `0x009A19A1`). CONFIRMED.
fn zoom_limit_slots(restrictions: &EsfRecord) -> (usize, Option<usize>) {
    let min_at = if restrictions.version > 1 { 13 } else { 12 };
    (min_at, (restrictions.version > 2).then_some(min_at + 1))
}

/// Reads the zoom limit ([`zoom_limit_slots`]); a bound the record does not have keeps the
/// constructor's tweaker value (`0x009968A0`), as in the exe.
fn read_zoom_limit(r: &EsfRecord) -> ZoomLimit {
    let mut z = ZoomLimit::default();
    let (min_at, max_at) = zoom_limit_slots(r);
    let read = |at: usize, out: &mut f32| match r.get(at) {
        Some(EsfNode::F32(v)) => *out = *v,
        _ => log::warn!("EPISODIC_RESTRICTIONS v{} #{at} is not an f32 zoom limit: the default {out} is kept", r.version),
    };
    read(min_at, &mut z.min_tilt);
    if let Some(at) = max_at {
        read(at, &mut z.max_tilt);
    }
    z
}

/// Reads the [`switch_slots`] and the zoom limit into `world`. The shipped start positions carry the
/// scripts' values (CONFIRMED: Egypt sets #31, Italy #29 and #31, the tutorial #31 and #32).
pub(crate) fn read_switches(model_rec: &EsfRecord, world: &mut World) {
    let mut v = [false; 7];
    world.zoom_limit = ZoomLimit::default();
    if let Some(r) = model_rec.child("EPISODIC_RESTRICTIONS") {
        for (out, (at, above)) in v.iter_mut().zip(switch_slots(r)) {
            *out = above.is_none_or(|a| r.version > a) && r.get_bool(at).unwrap_or(false);
        }
        world.zoom_limit = read_zoom_limit(r);
    }
    let [turns, assassination, infiltration, saving, research, liberation, victory] = v;
    world.display_turns = turns;
    world.force_success_for_human = (assassination, infiltration);
    world.saving_disabled = saving;
    world.technology_research_disabled = research;
    world.liberation_options_disabled = liberation;
    world.victory_notification_disabled = victory;
}

/// Writes the world's switches and zoom limit back into the [`switch_slots`] and
/// [`zoom_limit_slots`] the source record has (the writer `0x009A1070` saves every one). A slot the
/// record's version does not have, or one of another type, keeps the source's node, so the
/// record's layout never changes.
pub(crate) fn write_switches(model_rec: &mut EsfRecord, world: &World) {
    let Some(r) = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(b) if b.name == "EPISODIC_RESTRICTIONS" => Some(&mut **b),
        _ => None,
    }) else {
        return;
    };
    for ((at, above), value) in switch_slots(r).into_iter().zip(switch_values(world)) {
        if above.is_none_or(|a| r.version > a)
            && let Some(node @ EsfNode::Bool(_)) = r.children.get_mut(at)
        {
            *node = EsfNode::Bool(value);
        }
    }
    let (min_at, max_at) = zoom_limit_slots(r);
    for (at, value) in [(Some(min_at), world.zoom_limit.min_tilt), (max_at, world.zoom_limit.max_tilt)] {
        if let Some(node @ EsfNode::F32(_)) = at.and_then(|at| r.children.get_mut(at)) {
            *node = EsfNode::F32(value);
        }
    }
}

/// `CAMPAIGN_MODEL/HISTORICAL_EVENT_MANAGER` (v1) → `TRIGGERED_DYNAMIC_EVENT_ARRAY[]` {utf16 key}: the
/// once-only historical events already triggered (loader `0x0087F980`, writer `0x00895050`; CONFIRMED
/// shape, empty in every vanilla save), in the exe's list order
/// ([`ntw_sim::campaign::historical_events::sorted_triggered`]).
pub(crate) fn triggered_historical_events(model_rec: &EsfRecord) -> Vec<String> {
    let keys = model_rec
        .child("HISTORICAL_EVENT_MANAGER")
        .map(|m| items(m, "TRIGGERED_DYNAMIC_EVENT_ARRAY").filter_map(|it| it.first()?.as_str().map(str::to_string)).collect::<Vec<_>>())
        .unwrap_or_default();
    ntw_sim::campaign::historical_events::sorted_triggered(keys)
}

/// `CHARACTER` → its details (`CHARACTER_DETAILS` v3 at #1, post id at #8).
pub(crate) fn character_details(ch: &EsfRecord) -> Option<CharacterDetails> {
    let d = ch.get(1)?.as_record().filter(|r| r.name == "CHARACTER_DETAILS")?;
    let rec = |i: usize| d.get(i).and_then(EsfNode::as_record);
    let traits = rec(0)
        .map(|t| {
            items(t, "TRAIT")
                .filter_map(|it| Some(CharacterTrait { key: it.first()?.as_str()?.to_string(), points: it.get(1)?.as_i32()?, level: 0 }))
                .collect()
        })
        .unwrap_or_default();
    let other = rec(3).map(|l| (s(l.get(0)), s(l.get(1)))).unwrap_or_default();
    let portrait = rec(8)
        .filter(|p| p.name == "PORTRAIT_DETAILS")
        .map(|p| Portrait { card: s(p.get(0)), alternative: s(p.get(1)), info: s(p.get(2)), index: p.get_i32(3).unwrap_or(0) })
        .unwrap_or_default();
    Some(CharacterDetails {
        forename: loc(d.get(1)),
        surname: loc(d.get(2)),
        other_names: other,
        regnal_numeral: s(d.get(4)),
        birth: date_of(rec(5)),
        date_2: date_of(rec(6)),
        portrait,
        faction_key: s(d.get(9)),
        traits,
        ancillaries: items(d, "AgentAncillaries").filter_map(|it| Some(it.first()?.as_str()?.to_string())).collect(),
        attributes: items(d, "AgentAttributes")
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_i32()?)))
            .collect(),
        abilities: items(d, "AgentAbilities")
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_i32()?, s(it.get(2)))))
            .collect(),
        attribute_bonuses: items(d, "AgentAttributeBonuses")
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_u32()?)))
            .collect(),
        onscreen_name: loc(d.get(16)),
        post: ch.get_u32(8).unwrap_or(0),
        returns_after_death: ch.get_bool(34).unwrap_or(false),
        duels_lost: ch.get_u32(36).unwrap_or(0),
        duels_won: ch.get_u32(37).unwrap_or(0),
        hidden: ch.get_bool(22).unwrap_or(false),
        turns_at_sea: ch.get_u32(28).unwrap_or(0),
        turns_in_enemy_lands: ch.get_u32(29).unwrap_or(0),
        turns_at_home: ch.get_u32(30).unwrap_or(0),
        no_action: ch.get_bool(14).unwrap_or(false),
        idle_turns: ch.get_u32(15).unwrap_or(0),
        fled: ch.get_bool(27).unwrap_or(false),
        wounded: false,
        historical_key: None,
    })
}

/// The index in `FACTION` of the faction-flag anchor: the first bool array after `EXPOSED_CHARACTERS`
/// (#45 in the vanilla saves), with the u32 +0x828 right before it and the out-of-game bool +0x824 before
/// that (the loader `0x0087A190`; anchored on the array, so the `PRESTIGE` record between them does not
/// shift it; checked on the eur_napoleon startpos). `None` when the record does not have that shape.
pub(crate) fn flags_anchor(f: &EsfRecord) -> Option<usize> {
    let exposed = f.children.iter().position(|c| c.record_name() == Some("EXPOSED_CHARACTERS"))?;
    let after = exposed + 1;
    let arr = f.children[after..].iter().position(|c| matches!(c, EsfNode::BoolArray(_))).map(|i| after + i)?;
    (arr >= after + 2 && f.children[arr - 1].as_u32().is_some()).then_some(arr)
}

/// A `FACTION`'s land and naval upkeep trackers (faction +0x738 / +0x744, the amounts they booked into the
/// running economics record's categories 19 / 20): the two i32 right after the flag anchor ([`flags_anchor`]),
/// which the loader reads at `0x0087B548` / `0x0087B551` (`0x0085EF40` / `0x0085EF60`, one int each) and sets
/// through `0x0088F950` / `0x0088F990` (CONFIRMED: France in `orig_fr_may1811` holds 12019 and 563, its
/// record's 19 and 20). `None` without them.
pub(crate) fn upkeep_trackers(f: &EsfRecord) -> Option<[i32; 2]> {
    let arr = flags_anchor(f)?;
    let land = f.children.get(arr + 1)?.as_i32()?;
    let naval = f.children.get(arr + 2)?.as_i32()?;
    Some([land, naval])
}

/// The position of a `FACTION`'s `WarStartedCharacterFaction` flag (faction +0x925,
/// `FactionDetails::war_started`): 8 after the second `CAMPAIGN_BONUS_VALUES`, when a bool sits there
/// (the loader `0x0087A190`; #63 on every vanilla `FACTION` v18). The bool before it (#62) is the
/// `FactionDestroyedByCharacterFaction` flag (faction +0x924, `FactionDetails::destroyed_faction`).
/// `None` when the record does not have that shape.
pub(crate) fn war_started_index(f: &EsfRecord) -> Option<usize> {
    let bonus = f.children.iter().enumerate().filter(|(_, c)| c.record_name() == Some("CAMPAIGN_BONUS_VALUES")).nth(1)?.0;
    let i = bonus + 8;
    (f.children.get(i)?.as_bool().is_some() && f.children[i - 1].as_bool().is_some()).then_some(i)
}

/// `FACTION` → its details, and the governorship taxes as `taxes_levels` keys (lower, upper).
pub(crate) fn faction_details(f: &EsfRecord) -> (FactionDetails, Option<(String, String)>) {
    let plain: Vec<&EsfNode> = f.values().collect();
    let mut d = FactionDetails { display_name: s(plain.get(2).copied()), ..Default::default() };
    d.exposed = items(f, "EXPOSED_CHARACTERS").filter_map(|it| it.first()?.as_i32()).map(CharacterId).collect();
    if let Some(g) = f.child("GOVERNMENT") {
        d.government_id = g.get_i32(0).unwrap_or(0);
        for it in items(g, "POSTS_ARRAY") {
            let Some(p) = it.first().and_then(EsfNode::as_record).filter(|r| r.name == "CHARACTER_POST") else { continue };
            let holder = p.get_u32(2).filter(|&h| h != 0).map(|h| CharacterId(h as i32));
            let governorship = p.child("GOVERNORSHIP").map(read_governorship);
            d.posts.push(GovernmentPost { id: p.get_i32(0).unwrap_or(0), key: s(p.get(1)), holder, governorship });
        }
    }
    // Capital and home region: the two i32 values right before EXPOSED_CHARACTERS, which the exe's
    // faction loader always opens next; the FORT_UPGRADE_MANAGER before them is optional (0x0087A190:
    // capital 0x0087B20C, home 0x0087B23C, EXPOSED_CHARACTERS 0x0087B254; see FactionDetails). The
    // loader's default is -1, so 0 and negative values mean none.
    // No vanilla faction lacks the FORT_UPGRADE_MANAGER block (checked on all 8 vanilla startpos: the
    // old and new reads give identical capitals; factions with no capital have the block with 0/0
    // values), so reading by position from EXPOSED_CHARACTERS only matters for records without it.
    if let Some(exposed) = f.children.iter().position(|c| c.record_name() == Some("EXPOSED_CHARACTERS")) {
        let region = |i: Option<usize>| {
            i.and_then(|i| f.get(i)).and_then(EsfNode::as_i32).filter(|&v| v > 0).map(|v| RegionId(v as u32))
        };
        d.capital = region(exposed.checked_sub(2));
        d.capital_2 = region(exposed.checked_sub(1));
        // The out-of-game flag (faction +0x824, read at 0x0087B4CC): the bool two before the anchor
        // ([`flags_anchor`]).
        if let Some(arr) = flags_anchor(f) {
            d.out_of_game = f.children[arr - 2].as_bool();
        }
    }
    d.religion = f.values().filter_map(EsfNode::as_str).find(|v| v.starts_with("rel_") || v.starts_with("align_")).unwrap_or_default().to_string();
    // Trade income of the last ECONOMICS_DATA record (category 7 = #1[2], CAMPAIGN_FIDELITY.md).
    d.stored_trade_income = f
        .child("FACTION_ECONOMICS")
        .and_then(|e| e.get(0))
        .and_then(EsfNode::as_record_array)
        .and_then(|h| h.items.last())
        .and_then(|it| it.first())
        .and_then(EsfNode::as_record)
        .and_then(|d| d.get(1))
        .and_then(EsfNode::as_i32_array)
        .and_then(|a| a.get(2).copied());
    // The bool right before CHARACTER_ARRAY: the major-power flag (faction +0x524, CAMPAIGN_FIDELITY.md).
    if let Some(i) = f.children.iter().position(|c| c.record_name() == Some("CHARACTER_ARRAY")) {
        d.major = i.checked_sub(1).and_then(|p| f.children[p].as_bool());
    }
    // The flags +0x924 / +0x925, which sit together: one warning for a record without them.
    (d.destroyed_faction, d.war_started) = match war_started_index(f) {
        Some(i) => (f.children[i - 1].as_bool().unwrap_or(false), f.children[i].as_bool().unwrap_or(false)),
        None => {
            log::warn!("load: a FACTION v{} has no FactionDestroyedByCharacterFaction / WarStartedCharacterFaction flags; read as not set", f.version);
            (false, false)
        }
    };
    d.family = f.child("FAMILY").and_then(read_family);
    // Technologies (EFFECTS_FIDELITY.md §3) and the campaign difficulty (slot 0-F).
    if let Some(tm) = f.child("FACTION_TECHNOLOGY_MANAGER") {
        d.technologies = items(tm, "techs").filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_u32()?))).collect();
        // #2 f32 progress, #3 u32 the researching school slot (CAMPAIGN_FIDELITY.md §Research).
        d.research = items(tm, "techs")
            .filter_map(|it| {
                let key = it.first()?.as_str()?.to_string();
                let progress = it.get(2).and_then(EsfNode::as_f32).unwrap_or(0.0);
                let researcher = it.get(3).and_then(EsfNode::as_u32).unwrap_or(0);
                // #5 the traded count (entry +0x28, saver 0x00894430).
                let traded = it.get(5).and_then(EsfNode::as_u32).unwrap_or(0);
                (progress != 0.0 || researcher != 0 || traded != 0)
                    .then_some((key, ntw_sim::campaign::details::TechResearch { progress, researcher, traded }))
            })
            .collect();
    }
    d.difficulty = f
        .child("CAMPAIGN_PLAYER_SETUP")
        .and_then(|s| s.child("CAMPAIGN_PLAYER_SETUP_INGAME_MODIFIABLES"))
        .and_then(|m| m.values().find_map(EsfNode::as_i32))
        .unwrap_or(0);
    // The recruitment pools (FACTION #75 `CHARACTER_RECRUITMENT_MANAGER`, slot 0-G).
    if let Some(m) = f.child("CHARACTER_RECRUITMENT_MANAGER") {
        let pool = |name: &str| {
            m.child(name)
                .map(|p| {
                    let ids = p.get(0).and_then(EsfNode::as_u32_array).map(|a| a.iter().map(|&x| CharacterId(x as i32)).collect()).unwrap_or_default();
                    (ids, p.get_u32(1).unwrap_or(0))
                })
                .unwrap_or_default()
        };
        d.general_pool = pool("GENERAL_RECRUITMENT");
        d.admiral_pool = pool("ADMIRAL_RECRUITMENT");
    }
    // The two saved effect containers (#54 base, #55 base + difficulty; EFFECTS_FIDELITY.md §1).
    let mut bonus = f.children.iter().filter_map(EsfNode::as_record).filter(|r| r.name == "CAMPAIGN_BONUS_VALUES").map(saved_bonuses);
    d.bonus_base = bonus.next().unwrap_or_default();
    d.bonus_with_difficulty = bonus.next().unwrap_or_default();
    let taxes = d.governorship().and_then(|g| {
        Some((
            GovernorshipTaxes::level_key(g.taxes.lower)?.to_string(),
            GovernorshipTaxes::level_key(g.taxes.upper)?.to_string(),
        ))
    });
    (d, taxes)
}

fn read_governorship(g: &EsfRecord) -> Governorship {
    let t = g.child("GOVERNORSHIP_TAXES");
    let u8_at = |i: usize| match t.and_then(|t| t.get(i)) {
        Some(EsfNode::U8(v)) => i32::from(*v),
        _ => 0,
    };
    Governorship {
        taxes: GovernorshipTaxes {
            lower: t.and_then(|t| t.get_u32(0)).unwrap_or(2),
            upper: t.and_then(|t| t.get_u32(1)).unwrap_or(2),
            lower_rate: u8_at(2),
            upper_rate: u8_at(3),
        },
        theatre_id: g.get_i32(1).unwrap_or(0),
        regions: g.get(2).and_then(EsfNode::as_u32_array).map(|a| a.iter().map(|&r| RegionId(r)).collect()).unwrap_or_default(),
        faction: FactionId(g.get_u32(3).unwrap_or(0) as i32),
        flags: (g.get_bool(4).unwrap_or(false), g.get_bool(5).unwrap_or(false)),
    }
}

/// `DIPLOMACY_RELATIONSHIP` v14 → (target, relationship). Field meanings:
/// `analysis/campaign/S1_LEFTOVERS.md` §1 (the exe's reader `0x00AE9480` and its payment-item reader
/// `0x00AE90E0`; writer `0x00AFC340`).
pub(crate) fn relationship(r: &EsfRecord) -> Option<(FactionId, Relationship)> {
    let target = FactionId(r.get_i32(0)?);
    // Every plain field is read by its index with whatever integer type it has, so a value of
    // another width (an odd modded file) still loads.
    let int = |i: usize| r.get(i).and_then(EsfNode::as_int).unwrap_or(0);
    let i32_at = |i: usize| int(i) as i32;
    let u32_at = |i: usize| int(i) as u32;
    let bool_at = |i: usize| r.get_bool(i).unwrap_or(false);
    let array = |i: usize| r.get(i).and_then(EsfNode::as_record_array).map(|a| a.items.as_slice()).unwrap_or_default();
    let item_int = |it: &[EsfNode], k: usize| it.get(k).and_then(EsfNode::as_int).unwrap_or(0);
    let item_bool = |it: &[EsfNode], k: usize| it.get(k).and_then(EsfNode::as_bool).unwrap_or(false);
    let attitudes = array(1)
        .iter()
        .map(|it| AttitudeFactor {
            drift: item_int(it, 0) as i32,
            value: item_int(it, 1) as i32,
            limit: item_int(it, 2) as i32,
            limited: item_bool(it, 3),
            cap: item_int(it, 4) as i32,
            capped: item_bool(it, 5),
        })
        .collect();
    let payments = array(14)
        .iter()
        .map(|it| RegularPayment { amount: item_int(it, 0) as i32, turns: item_int(it, 1) as u32 })
        .collect();
    let allied_in_war_against = array(17)
        .iter()
        .map(|it| AlliedWar { enemy: FactionId(item_int(it, 0) as i32), saved_access_turns: item_int(it, 1) as i32 })
        .collect();
    let mut diplomacy_options = [0u32; 14];
    if let Some(a) = r.get(18).and_then(EsfNode::as_u32_array) {
        for (o, v) in diplomacy_options.iter_mut().zip(a) {
            *o = *v;
        }
    }
    let war_ally = Some(i32_at(5)).filter(|&f| f != 0).map(FactionId);
    Some((
        target,
        Relationship {
            attitudes,
            trade_agreement: bool_at(2),
            military_access_turns: i32_at(3),
            war_ally,
            alliance_commitment_turns: u32_at(6),
            war_momentum: i32_at(7),
            protectorate_tribute: i32_at(8),
            protectorate_income: i32_at(9),
            war_region_balance: i32_at(10),
            war_wealth_balance: i32_at(11),
            war_turns: u32_at(12),
            turns_since_battle: u32_at(13),
            payments,
            friendship_turns: u32_at(15),
            unknown_16: u32_at(16),
            allied_in_war_against,
            diplomacy_options,
            military_access_streak: u32_at(19),
            previous_stance: s(r.get(20)),
            unknown_21: bool_at(21),
            unknown_22: bool_at(22),
            start_attitude: i32_at(23),
            military_access_granted: i32_at(24),
            military_access_elapsed: u32_at(25),
            access_cancel_grievance: u32_at(26),
            trade_embargo_turns: u32_at(27),
            // Version < 14 files have no #28; the exe's default is true (CONFIRMED in its reader).
            allows_region_return: r.get_bool(28).unwrap_or(true),
        },
    ))
}

/// `CAMPAIGN_BONUS_VALUES` → its entries (`CAMPAIGN_BONUS_VALUE_BLOCK[]` items each holding one
/// `CAMPAIGN_BONUS_VALUE`; reader `0x00DF14C0`, entry reader [`saved_bonus`]).
fn saved_bonuses(r: &EsfRecord) -> Vec<ntw_sim::campaign::effects::SavedBonus> {
    items(r, "CAMPAIGN_BONUS_VALUE_BLOCK").filter_map(|it| saved_bonus(it.first()?.as_record()?)).collect()
}

/// One `CAMPAIGN_BONUS_VALUE` {u32 type, i32 bonus, f32 value, then the type's qualifier strings:
/// type 7 has two, population class then religion, most types one, type 1 none} (CONFIRMED: entry
/// reader `0x00DF0D10`, writer `0x00E02F80`; `SavedBonus::qualifier_count`). A missing string reads "".
pub(crate) fn saved_bonus(v: &EsfRecord) -> Option<ntw_sim::campaign::effects::SavedBonus> {
    Some(ntw_sim::campaign::effects::SavedBonus {
        kind: v.get_u32(0)?,
        bonus: v.get_i32(1)?,
        value: v.get_f32(2)?,
        qualifier: v.get_str(3).unwrap_or_default().to_string(),
        qualifier2: v.get_str(4).unwrap_or_default().to_string(),
    })
}

/// The strings of a `CAMPAIGN_LOCALISATION` record (one or two).
fn loc_strings(r: Option<&EsfNode>) -> Vec<String> {
    r.and_then(EsfNode::as_record).map(|l| l.children.iter().filter_map(EsfNode::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// `FACTION` `FAMILY` v2: ten `FAMILY::MONARCHY_INFO_CHARACTER`, a u8 (the heir child's index),
/// `ORDINAL_PAIR[]` {loc, i32} (CONFIRMED: loader `0x0087D490`, member loader `0x008C2390`).
pub(crate) fn read_family(fam: &EsfRecord) -> Option<ntw_sim::campaign::family::Family> {
    use ntw_sim::campaign::family::{Family, FamilyMember, FIRST_CHILD};
    let members: Vec<FamilyMember> = fam
        .children_named("FAMILY::MONARCHY_INFO_CHARACTER")
        .map(|r| {
            let int = |i: usize| r.get(i).and_then(EsfNode::as_int).unwrap_or(0);
            let married_to = int(14) as u32;
            FamilyMember {
                names: loc_strings(r.get(0)),
                male: int(1) != 0,
                exists: int(2) != 0,
                children: int(3) as u8,
                trait_4: int(4) as i32,
                age: int(5) as i32,
                regnal: int(6) as i32,
                child_ages: [int(7) as i32, int(8) as i32, int(9) as i32, int(10) as i32],
                portrait: r
                    .get(11)
                    .and_then(EsfNode::as_record)
                    .map(|p| Portrait { card: s(p.get(0)), alternative: s(p.get(1)), info: s(p.get(2)), index: p.get_i32(3).unwrap_or(0) })
                    .unwrap_or_default(),
                religion: s(r.get(12)),
                owner: int(13) as u32,
                married_to,
                spouse_names: if married_to != 0 { loc_strings(r.get(15)) } else { Vec::new() },
                heir: false,
            }
        })
        .collect();
    if members.len() != 10 {
        return None;
    }
    let mut out = Family { members, ordinals: Vec::new(), female_heirs: false };
    let heir = fam.values().find_map(|v| match v {
        EsfNode::U8(x) => Some(*x),
        _ => None,
    });
    if let Some(h) = heir.filter(|&h| h < 4) {
        out.members[FIRST_CHILD + usize::from(h)].heir = true;
    }
    out.ordinals = items(fam, "ORDINAL_PAIR")
        .filter_map(|it| Some((loc_strings(it.first()).into_iter().next().unwrap_or_default(), it.get(1)?.as_i32()?)))
        .collect();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::esf::EsfRecordArray;

    /// A `CAMPAIGN_MODEL` whose `EPISODIC_RESTRICTIONS` has every switch set: 33 children with the
    /// `LUA[]` array at #30, or 32 booleans without it.
    fn model_with_restrictions(version: u8, lua: bool) -> EsfRecord {
        let mut children = vec![EsfNode::Bool(true); if lua { 33 } else { 32 }];
        if lua {
            children[30] = EsfNode::RecordArray(Box::new(EsfRecordArray { name: "LUA".into(), version: 0, items: Vec::new() }));
        }
        let restrictions = EsfRecord { name: "EPISODIC_RESTRICTIONS".into(), version, children };
        EsfRecord { name: "CAMPAIGN_MODEL".into(), version: 0, children: vec![EsfNode::Record(Box::new(restrictions))] }
    }

    fn switches(model_rec: &EsfRecord) -> [bool; 7] {
        let mut world = World::default();
        read_switches(model_rec, &mut world);
        switch_values(&world)
    }

    /// Each switch is read only from a record above the version the loader `0x009968A0` checks:
    /// #8 above 1, #25 above 0xB, #29 above 0xE, #31 above 0xF, #32 above 0x10; #20 / #22 at any
    /// version.
    #[test]
    fn each_switch_needs_a_record_above_its_version() {
        assert_eq!(switches(&model_with_restrictions(17, true)), [true; 7]);
        assert_eq!(switches(&model_with_restrictions(0x10, true)), [true, true, true, true, true, true, false]);
        assert_eq!(switches(&model_with_restrictions(0xF, true)), [true, true, true, true, true, false, false]);
        assert_eq!(switches(&model_with_restrictions(0xE, true)), [true, true, true, true, false, false, false]);
        assert_eq!(switches(&model_with_restrictions(0xB, true)), [true, true, true, false, false, false, false]);
        assert_eq!(switches(&model_with_restrictions(1, true)), [false, true, true, false, false, false, false]);
    }

    /// The liberation and victory-notification switches follow the `LUA[]` array at #30, and move
    /// up to #30 / #31 when a record has none (the loader reads that array only when present).
    #[test]
    fn the_last_two_switches_follow_the_lua_array() {
        let mut model = model_with_restrictions(17, true);
        let EsfNode::Record(r) = &mut model.children[0] else { unreachable!() };
        r.children[31] = EsfNode::Bool(false);
        assert_eq!(switches(&model)[5..], [false, true]);
        let mut model = model_with_restrictions(17, false);
        let EsfNode::Record(r) = &mut model.children[0] else { unreachable!() };
        r.children[31] = EsfNode::Bool(false);
        assert_eq!(switches(&model)[5..], [true, false]);
    }

    /// The writer puts each switch back where the reader takes it, and leaves the slots an older
    /// record does not have (and the `LUA[]` array) as they are.
    #[test]
    fn the_switches_are_written_where_they_are_read() {
        let world = World {
            technology_research_disabled: false,
            liberation_options_disabled: true,
            victory_notification_disabled: false,
            ..World::default()
        };
        let mut model = model_with_restrictions(17, true);
        write_switches(&mut model, &world);
        assert_eq!(switches(&model), switch_values(&world));
        let EsfNode::Record(r) = &model.children[0] else { unreachable!() };
        assert!(matches!(&r.children[30], EsfNode::RecordArray(a) if a.name == "LUA"));
        let mut old = model_with_restrictions(0xE, true);
        write_switches(&mut old, &world);
        let EsfNode::Record(r) = &old.children[0] else { unreachable!() };
        assert_eq!(r.get_bool(31), Some(true));
        assert_eq!(r.get_bool(32), Some(true));
        assert_eq!(r.get_bool(25), Some(false));
    }

    /// A record with the zoom limit: #13 the smallest tilt and #14 the largest as f32.
    fn model_with_zoom_limit(version: u8, min: f32, max: f32) -> EsfRecord {
        let mut model = model_with_restrictions(version, true);
        let EsfNode::Record(r) = &mut model.children[0] else { unreachable!() };
        r.children[13] = EsfNode::F32(min);
        r.children[14] = EsfNode::F32(max);
        model
    }

    fn zoom_limit(model_rec: &EsfRecord) -> ZoomLimit {
        let mut world = World::default();
        read_switches(model_rec, &mut world);
        world.zoom_limit
    }

    /// The loader `0x009968A0` reads the smallest tilt at #13 above version 1 and the largest at #14
    /// only above version 2; a bound it does not read keeps the tweaker default.
    #[test]
    fn the_zoom_limit_is_read_by_version() {
        assert_eq!(zoom_limit(&model_with_zoom_limit(17, 0.61, 0.95)), ZoomLimit { max_tilt: 0.95, min_tilt: 0.61 });
        assert_eq!(zoom_limit(&model_with_zoom_limit(3, 0.61, 0.95)), ZoomLimit { max_tilt: 0.95, min_tilt: 0.61 });
        assert_eq!(zoom_limit(&model_with_zoom_limit(2, 0.61, 0.95)), ZoomLimit { max_tilt: 1.0, min_tilt: 0.61 });
        // Version 1 has no #8, so the smallest tilt is at #12.
        let mut v1 = model_with_zoom_limit(1, 0.61, 0.95);
        let EsfNode::Record(r) = &mut v1.children[0] else { unreachable!() };
        r.children[12] = EsfNode::F32(0.7);
        assert_eq!(zoom_limit(&v1), ZoomLimit { max_tilt: 1.0, min_tilt: 0.7 });
        // No f32 there (or no record): the defaults.
        assert_eq!(zoom_limit(&model_with_restrictions(17, true)), ZoomLimit::default());
        assert_eq!(zoom_limit(&EsfRecord { name: "CAMPAIGN_MODEL".into(), version: 0, children: Vec::new() }), ZoomLimit::default());
    }

    /// The writer `0x009A1070` puts the zoom limit back where the loader reads it; a version-2 record
    /// keeps its #14.
    #[test]
    fn the_zoom_limit_is_written_where_it_is_read() {
        let world = World { zoom_limit: ZoomLimit::from_script(1.0, 0.61), ..World::default() };
        let mut model = model_with_zoom_limit(17, 0.5, 1.0);
        write_switches(&mut model, &world);
        assert_eq!(zoom_limit(&model), world.zoom_limit);
        let mut v2 = model_with_zoom_limit(2, 0.5, 0.9);
        write_switches(&mut v2, &world);
        let EsfNode::Record(r) = &v2.children[0] else { unreachable!() };
        assert_eq!((r.get_f32(13), r.get_f32(14)), (Some(0.61), Some(0.9)));
    }

    /// The triggered list is read in the exe's order (UTF-16 code units, each key once).
    #[test]
    fn the_triggered_events_are_read_sorted() {
        let item = |k: &str| vec![EsfNode::Utf16String(k.into())];
        let array = EsfRecordArray { name: "TRIGGERED_DYNAMIC_EVENT_ARRAY".into(), version: 0, items: vec![item("b"), item("{FF21}"), item("{1F600}"), item("b")] };
        let manager = EsfRecord { name: "HISTORICAL_EVENT_MANAGER".into(), version: 1, children: vec![EsfNode::RecordArray(Box::new(array))] };
        let model = EsfRecord { name: "CAMPAIGN_MODEL".into(), version: 0, children: vec![EsfNode::Record(Box::new(manager))] };
        assert_eq!(triggered_historical_events(&model), ["b", "{1F600}", "{FF21}"]);
        assert!(triggered_historical_events(&model_with_restrictions(2, true)).is_empty());
    }
}
