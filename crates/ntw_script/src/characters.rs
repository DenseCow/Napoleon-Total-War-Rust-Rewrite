//! The character conditions and the trait / ancillary actions the generated trigger scripts
//! (`export_triggers.lua`, `export_ancillaries.lua`) use (slot 0-G,
//! `analysis/fidelity/CHARACTERS_FIDELITY.md` §3–§4). Each condition's meaning is the exe's own
//! registered description (handler addresses in the notes); the rules behind the actions live in
//! [`ntw_sim::campaign::characters`].
//!
//! Conditions about a battle, a duel, a research or a building that just happened need event facts the
//! model does not carry yet; they stay logging stubs (false / 0, UNKNOWN) and are listed in the notes.

use mlua::{MultiValue, Value};
use ntw_sim::campaign::characters::{self as rules, AncillaryRefusal, TraitOutcome};
use ntw_sim::campaign::{CharacterId, Stance};

use crate::game::{Shared, arg_str};
use crate::state::{ScriptContext, ScriptState};

/// A numeric argument.
fn arg_num(args: &MultiValue, i: usize) -> Option<f64> {
    match args.get(i)? {
        Value::Number(n) => Some(*n),
        Value::Integer(n) => Some(*n as f64),
        Value::String(s) => s.to_string_lossy().parse().ok(),
        _ => None,
    }
}

fn num(v: i32) -> Value {
    Value::Number(f64::from(v as f32))
}

/// The context's character, if it is one the model knows.
fn character(st: &ScriptState, ctx: &ScriptContext) -> Option<CharacterId> {
    let id = CharacterId(ctx.character?);
    st.model.world.characters.contains_key(&id).then_some(id)
}

/// The post a character holds, if any.
fn post_of(st: &ScriptState, c: CharacterId) -> Option<&ntw_sim::campaign::GovernmentPost> {
    let w = &st.model.world;
    let ch = w.characters.get(&c)?;
    let d = w.character_details.get(&c)?;
    if d.post == 0 {
        return None;
    }
    w.faction_details.get(&ch.faction)?.posts.iter().find(|p| p.id as u32 == d.post)
}

/// A character condition (character context), or `None` if `name` is not one of them.
pub(crate) fn evaluate(st: &ScriptState, name: &str, args: &MultiValue, ctx: &ScriptContext) -> Option<Value> {
    let w = &st.model.world;
    let cr = &st.model.rules.characters;
    let c = character(st, ctx);
    let ch = c.and_then(|c| w.characters.get(&c));
    let d = c.and_then(|c| w.character_details.get(&c));
    let faction = ch.and_then(|ch| w.factions.get(&ch.faction));
    let fkey = faction.map_or("", |f| f.key.as_str());
    let a0 = arg_str(args, 0);
    let is = |b: bool| Value::Boolean(b);
    let has_anc = |d: &ntw_sim::campaign::CharacterDetails, k: &str| d.ancillaries.iter().any(|a| a == k);
    Some(match name {
        // 0x0089E0D0: the agent type, compared case-sensitively (0x004F1F30, CONFIRMED).
        "CharacterType" => is(ch.is_some_and(|ch| Some(ch.kind.esf_name()) == a0.as_deref())),
        // 0x0089C240: the culture of the character's faction (factions #2 → cultures_subcultures).
        "CharacterCultureType" => is(ch.is_some() && a0.as_deref() == Some(cr.culture(fkey))),
        "CharacterFactionName" => is(ch.is_some() && a0.as_deref() == Some(fkey)),
        "CharacterHasAncillary" => is(d.zip(a0.as_deref()).is_some_and(|(d, k)| has_anc(d, k))),
        "CharacterHasTrait" => is(d.zip(a0.as_deref()).is_some_and(|(d, k)| d.traits.iter().any(|t| t.key == k))),
        // 0x0089DED0: the held trait's points, 0 if not held (CONFIRMED: entry +8).
        "CharacterTrait" => num(d.zip(a0.as_deref()).and_then(|(d, k)| d.traits.iter().find(|t| t.key == k)).map_or(0, |t| t.points)),
        // 0x0089FBA0: the same for the faction leader.
        "FactionLeadersTrait" => {
            let leader = ch.and_then(|ch| w.faction_details.get(&ch.faction)).and_then(|f| f.leader());
            let pts = leader
                .and_then(|l| w.character_details.get(&l))
                .zip(a0.as_deref())
                .and_then(|(d, k)| d.traits.iter().find(|t| t.key == k))
                .map_or(0, |t| t.points);
            num(pts)
        }
        // 0x0089EBA0: start <= year <= end.
        "DateInRange" => {
            let y = f64::from(st.model.calendar.date.year);
            is(arg_num(args, 0).is_some_and(|s| s <= y) && arg_num(args, 1).is_some_and(|e| y <= e))
        }
        "CharacterMinisterialPosition" => is(c.and_then(|c| post_of(st, c)).is_some_and(|p| Some(p.key.as_str()) == a0.as_deref())),
        // 0x0089D000: "Is this character a minister with a post?"
        "CharacterHoldsPost" => is(c.and_then(|c| post_of(st, c)).is_some_and(|p| p.governorship.is_none())),
        "IsTheatreGovernor" => is(c.and_then(|c| post_of(st, c)).is_some_and(|p| p.governorship.is_some())),
        "IsFactionLeader" => is(c.and_then(|c| post_of(st, c)).is_some_and(|p| p.key == "faction_leader")),
        // 0x008A18A0 (CONFIRMED): the character is his faction's leader (`0x008CF600`) and the
        // family's leader member (FAMILY +4, its +0x19 "male" byte = family +0x1D) is not male.
        "IsFactionLeaderFemale" => is(ch.is_some_and(|ch| {
            let d = w.faction_details.get(&ch.faction);
            d.and_then(|d| d.leader()) == Some(ch.id)
                && d.and_then(|d| d.family.as_ref()).and_then(|f| f.members.first()).is_some_and(|m| !m.male)
        })),
        "FactionGovernmentType" => is(faction.is_some_and(|f| Some(f.government_key.as_str()) == a0.as_deref())),
        "CharacterFactionHasTechType" => {
            let researched = ch
                .and_then(|ch| w.faction_details.get(&ch.faction))
                .zip(a0.as_deref())
                .is_some_and(|(f, k)| f.technologies.iter().any(|(t, s)| t == k && *s == ntw_sim::campaign::effects::TECH_RESEARCHED));
            is(researched)
        }
        "FactionwideAncillaryTypeExists" => {
            let k = a0.unwrap_or_default();
            is(ch.is_some_and(|ch| {
                w.characters.values().filter(|x| x.faction == ch.faction).any(|x| w.character_details.get(&x.id).is_some_and(|d| has_anc(d, &k)))
            }))
        }
        "WorldwideAncillaryTypeExists" => {
            let k = a0.unwrap_or_default();
            is(w.character_details.values().any(|d| has_anc(d, &k)))
        }
        "CharacterFactionGeneralCount" | "CharacterFactionAdmiralCount" => {
            let kind = if name == "CharacterFactionGeneralCount" { "General" } else { "admiral" };
            num(ch.map_or(0, |ch| w.characters.values().filter(|x| x.faction == ch.faction && x.kind.esf_name() == kind).count() as i32))
        }
        "CharacterAttribute" => {
            num(d.zip(a0.as_deref()).and_then(|(d, k)| d.attributes.iter().find(|(a, _)| a == k)).map_or(0, |(_, v)| (*v).max(0)))
        }
        "CharacterForename" => is(d.zip(a0.as_deref()).is_some_and(|(d, k)| d.forename.ends_with(k))),
        "CharacterSurname" => is(d.zip(a0.as_deref()).is_some_and(|(d, k)| d.surname.ends_with(k))),
        // The turn-end counters (`0x009DA210`, CHARACTERS_FIDELITY.md §6).
        "CharacterTurnsAtHome" => num(d.map_or(0, |d| d.turns_at_home as i32)),
        "CharacterTurnsAtSea" => num(d.map_or(0, |d| d.turns_at_sea as i32)),
        "CharacterTurnsInEnemyLands" => num(d.map_or(0, |d| d.turns_in_enemy_lands as i32)),
        "NoActionThisTurn" => is(d.is_some_and(|d| d.no_action)),
        // The duel counters (`CHARACTER` #36 / #37).
        "CharacterDuelsLost" => num(d.map_or(0, |d| d.duels_lost as i32)),
        "CharacterDuelsFought" => num(d.map_or(0, |d| (d.duels_lost + d.duels_won) as i32)),
        "InSettlement" => is(ch.is_some_and(|ch| ch.garrisoned_in.is_some())),
        // 0x008A22D0: the character's faction is at war with anyone.
        "OnAWarFooting" => is(faction.is_some_and(|f| f.diplomacy.values().any(|s| *s == Stance::War))),
        "IsGuerrillaGeneral" => is(ch.is_some_and(|ch| ch.kind.esf_name() == "guerilla")),
        _ => return None,
    })
}

/// The trait / ancillary actions; `None` if `name` is not one of them.
pub(crate) fn effect(s: &Shared, name: &str, args: &MultiValue, ctx: Option<ScriptContext>) -> Option<()> {
    if !matches!(name, "trait" | "ancillary" | "remove_trait" | "remove_ancillary") {
        return None;
    }
    let mut st = s.borrow_mut();
    let key = arg_str(args, 0).unwrap_or_default();
    let Some(c) = ctx.as_ref().and_then(|ctx| character(&st, ctx)) else {
        // No character in the context: the exe's other context kinds (region, unit) are not used by
        // the shipped triggers; the roll is not made (UNKNOWN whether the exe rolls first).
        st.log.push(format!("effect.{name}({key:?}): no character in the context"));
        return Some(());
    };
    let line = match name {
        // effect.trait(key, scope, points, chance, context)
        "trait" => {
            let points = arg_num(args, 2).unwrap_or(0.0) as i32;
            let chance = arg_num(args, 3).unwrap_or(0.0) as i32;
            match rules::roll_trait(&mut st.model, c, &key, points, chance) {
                Some(TraitOutcome::Added { level, evicted }) => {
                    Some(format!("trait gained {key} ({points} pts, level {level}) by {}{}", c.raw(), evicted.map(|e| format!(", evicted {e}")).unwrap_or_default()))
                }
                Some(TraitOutcome::Raised { from_level, to_level }) => {
                    Some(format!("trait points {key} +{points} by {} (level {from_level} -> {to_level})", c.raw()))
                }
                Some(o) => Some(format!("trait {key} by {}: {o:?}", c.raw())),
                None => None,
            }
        }
        // effect.ancillary(key, chance, context)
        "ancillary" => {
            let chance = arg_num(args, 1).unwrap_or(0.0) as i32;
            match rules::roll_ancillary(&mut st.model, c, &key, chance) {
                Some(Ok(evicted)) => Some(format!("ancillary gained {key} by {}{}", c.raw(), evicted.map(|e| format!(", evicted {e}")).unwrap_or_default())),
                Some(Err(AncillaryRefusal::NoRoom | AncillaryRefusal::AlreadyHeld)) | None => None,
                Some(Err(e)) => Some(format!("ancillary {key} by {} refused: {e:?}", c.raw())),
            }
        }
        "remove_trait" => rules::remove_trait(&mut st.model, c, &key).then(|| format!("trait removed {key} from {}", c.raw())),
        _ => rules::remove_ancillary(&mut st.model, c, &key).then(|| format!("ancillary removed {key} from {}", c.raw())),
    };
    if let Some(l) = line {
        st.character_log.push(l);
    }
    Some(())
}
