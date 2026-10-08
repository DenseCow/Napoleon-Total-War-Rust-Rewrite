//! The engine-provided `conditions.*` predicates and `effect.*` actions (W3 §6.3).
//!
//! Every condition takes the event `context` as its **last** argument (CONFIRMED from the call
//! sites). The few below that we can answer from the campaign model are implemented; any other
//! name resolves to a **logging stub (UNKNOWN)** that returns `false`, or `0` for the conditions the
//! shipped scripts compare as numbers (`conditions.TurnNumber(context) == 48`, ...). Returning
//! `false` there would make Lua raise "attempt to compare boolean with number".
//!
//! `effect.*` (trait, ancillary, historical_character, advice and mission actions) are all logging
//! stubs (UNKNOWN): the character traits, ancillaries and advice systems do not exist yet.

use mlua::{AnyUserData, Lua, MultiValue, Table, Value};
use ntw_sim::campaign::Stance;

use crate::game::{Shared, arg_str, describe, log};
use crate::state::ScriptContext;

/// Conditions that the shipped scripts compare with `==`, `<`, `>=` ... against numbers (found by
/// scanning every shipped `.lua` file, INFERRED to return numbers). Unimplemented ones return 0.
pub const NUMERIC_CONDITIONS: &[&str] = &[
    "BattleAllianceNumberOfShips",
    "BattleAllianceNumberOfUnits",
    "BattleEnemyAlliancePercentageOfSpecialAbility",
    "BattleEnemyAlliancePercentageOfUnitCategory",
    "BattleEnemyAlliancePercentageOfUnitClass",
    "BattlePlayerAlliancePercentageOfAmmoType",
    "BattlePlayerAlliancePercentageOfSpecialAbility",
    "BattlePlayerAlliancePercentageOfTechnology",
    "BattlePlayerAlliancePercentageOfUnitCategory",
    "BattlePlayerAlliancePercentageOfUnitClass",
    "BattlePlayerAllianceToEnemyAllianceRatio",
    "BattleShipSailsPercentageDamage",
    "BattlesFought",
    "CampaignPercentageOfOwnCaptured",
    "CampaignPercentageOfOwnKilled",
    "CampaignPercentageOfOwnRouted",
    "CampaignPercentageOfThemCaptured",
    "CampaignPercentageOfThemKilled",
    "CampaignPercentageOfThemRouted",
    "CampaignPercentageOfUnitCategory",
    "CharacterAttribute",
    "CharacterDuelsFought",
    "CharacterDuelsLost",
    "CharacterFactionAdmiralCount",
    "CharacterFactionGeneralCount",
    "CharacterMPPercentageRemaining",
    "CharacterTrait",
    "CharacterTurnsAtHome",
    "CharacterTurnsAtSea",
    "CharacterTurnsInEnemyLands",
    "DifficultyLevel",
    "FactionLeadersTrait",
    "FactionTaxLevel",
    "FactionTreasury",
    "FactionTreasuryWorldPercentage",
    "FactionwideAncillaryTypeExists",
    "PercentageUnspentIncome",
    "RegionTaxLevel",
    "SupportCostsPercentage",
    "TurnNumber",
    "TurnsSinceThreadLastAdvanced",
    "WorldwideAncillaryTypeExists",
];

/// The context (last argument) of a condition call.
fn context_of(args: &MultiValue) -> Option<ScriptContext> {
    let ud: &AnyUserData = match args.back()? {
        Value::UserData(ud) => ud,
        _ => return None,
    };
    ud.borrow::<ScriptContext>().ok().map(|c| c.clone())
}

/// An implemented condition, or `None` if `name` is not implemented.
/// Implemented meanings are INFERRED from the names and from how the scripts use them.
fn evaluate(s: &Shared, name: &str, args: &MultiValue) -> Option<Value> {
    let ctx = context_of(args).unwrap_or_default();
    let st = s.borrow();
    let eq = |a: &Option<String>, b: Option<String>| Value::Boolean(a.is_some() && *a == b);
    Some(match name {
        // 1-based turn number (Calendar::turn_number, W3 §3.1 CONFIRMED); returned as an f32 number.
        "TurnNumber" => Value::Number(f64::from(st.model.calendar.turn_number() as f32)),
        "CampaignName" => Value::Boolean(arg_str(args, 0).as_deref() == Some(st.campaign.as_str())),
        "FactionName" => eq(&ctx.faction, arg_str(args, 0)),
        "FactionIsLocal" => Value::Boolean(ctx.faction.as_deref() == Some(st.local_faction.as_str())),
        "MissionName" => eq(&ctx.mission, arg_str(args, 0)),
        "SettlementName" => eq(&ctx.settlement, arg_str(args, 0)),
        "BuildingLevelName" => eq(&ctx.building_level, arg_str(args, 0)),
        "IsComponentType" => eq(&ctx.component, arg_str(args, 0)),
        // Is the context faction (or the local player) at war with faction `arg0`?
        "FactionIsEnemyCampaign" => {
            let me = ctx.faction.clone().unwrap_or_else(|| st.local_faction.clone());
            let at_war = match (st.faction_id(&me), arg_str(args, 0).and_then(|k| st.faction_id(&k))) {
                (Some(a), Some(b)) => st.model.world.factions[&a].diplomacy.get(&b) == Some(&Stance::War),
                _ => false,
            };
            Value::Boolean(at_war)
        }
        "FactionTreasury" => {
            let t = ctx
                .faction
                .as_deref()
                .and_then(|k| st.faction_id(k))
                .map_or(0, |id| st.model.world.factions[&id].treasury);
            Value::Number(f64::from(t as f32))
        }
        // The context settlement belongs to the local player.
        "SettlementIsLocal" => {
            let local = st.faction_id(&st.local_faction);
            let owned = st.model.world.regions.values().any(|r| {
                Some(&r.settlement.key) == ctx.settlement.as_ref() && Some(r.owner) == local
            });
            Value::Boolean(owned)
        }
        _ => return crate::characters::evaluate(&st, name, args, &ctx),
    })
}

/// Builds the `conditions` table.
pub(crate) fn create_conditions(lua: &Lua, shared: &Shared) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let meta = lua.create_table()?;
    let s = shared.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (t, name): (Table, String)| {
            let s = s.clone();
            let n = name.clone();
            let f = lua.create_function(move |_, args: MultiValue| {
                if let Some(v) = evaluate(&s, &n, &args) {
                    return Ok(v);
                }
                log(&s, format!("UNKNOWN stub conditions.{n}({})", describe(&args, 0)));
                Ok(if NUMERIC_CONDITIONS.contains(&n.as_str()) {
                    Value::Number(0.0)
                } else {
                    Value::Boolean(false)
                })
            })?;
            t.raw_set(name, f.clone())?;
            Ok(f)
        })?,
    )?;
    t.set_metatable(Some(meta))?;
    Ok(t)
}

/// Builds the `effect` table: every action is a logging stub (UNKNOWN).
pub(crate) fn create_effect(lua: &Lua, shared: &Shared) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let meta = lua.create_table()?;
    let s = shared.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (t, name): (Table, String)| {
            let s = s.clone();
            let n = name.clone();
            let f = lua.create_function(move |_, args: MultiValue| {
                if crate::characters::effect(&s, &n, &args, context_of(&args)).is_some() {
                    return Ok(());
                }
                log(&s, format!("UNKNOWN stub effect.{n}({})", describe(&args, 0)));
                Ok(())
            })?;
            t.raw_set(name, f.clone())?;
            Ok(f)
        })?,
    )?;
    t.set_metatable(Some(meta))?;
    Ok(t)
}
