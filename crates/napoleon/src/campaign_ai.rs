//! The campaign AI hook: `ntw_ai` plays every non-human faction during End Turn.
//!
//! `--campaign-ai on|off` (default on). The AI is installed on the campaign's script host
//! (`ntw_ai::campaign::driver::install`), so it runs inside the model's turn loop at each AI
//! faction's `AiTurn` step and the original scripts see every event its orders cause. Its orders
//! go through the model's validated commands; everything is deterministic (campaign RNG).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use bevy::prelude::*;
use ntw_ai::campaign::CampaignAiData;
use ntw_ai::campaign::driver::{self, AiTurnReport};
use ntw_formats::pack::Vfs;
use ntw_script::ScriptHost;

use crate::data::GameData;

/// The reports of the AI turns played so far (non-`Send`: shared with the script host's hook).
pub struct CampaignAiLog {
    reports: Rc<RefCell<Vec<AiTurnReport>>>,
    shown: usize,
    /// The (faction, kind of refusal) pairs already logged ([`refusals_to_log`]).
    refusals_logged: RefusalKinds,
}

/// (faction, error variant) pairs: bounded by factions × variants, whatever numbers the errors carry.
type RefusalKinds = HashSet<(ntw_sim::campaign::FactionId, std::mem::Discriminant<ntw_sim::campaign::CommandError>)>;

/// True unless `--campaign-ai off`.
fn enabled() -> bool {
    let args: Vec<String> = std::env::args().collect();
    !args.windows(2).any(|w| w[0] == "--campaign-ai" && w[1] == "off")
}

/// Installs the campaign AI on `host` (called by the campaign scene before turn 1 starts, so AI
/// factions before the human in turn order play too). The factions' AI keys and the regions' base
/// values are in the model (`World::ai_keys`, `World::region_base_values`).
pub fn attach(world: &mut World, host: &mut ScriptHost, vfs: &Vfs) {
    if !enabled() {
        info!("Campaign AI: off (--campaign-ai off)");
        return;
    }
    let data = match CampaignAiData::load(vfs, &world.resource::<GameData>().db) {
        Ok(d) => {
            for w in &d.load_warnings {
                warn!("Campaign AI tables: {w}");
            }
            d
        }
        Err(e) => {
            warn!("Campaign AI: AI tables unavailable ({e}); AI factions will not act");
            return;
        }
    };
    let reports = driver::install(host, Arc::new(data));
    world.insert_non_send(CampaignAiLog { reports, shown: 0, refusals_logged: RefusalKinds::new() });
}

/// Logs a one-line summary of each End Turn's AI activity.
fn log_reports(log: Option<NonSendMut<CampaignAiLog>>) {
    let Some(mut log) = log else { return };
    let reports = log.reports.clone();
    let reports = reports.borrow();
    if reports.len() == log.shown {
        return;
    }
    let new = &reports[log.shown..];
    let orders: usize = new.iter().map(|r| r.orders.len()).sum();
    let accepted: usize = new.iter().map(|r| r.accepted).sum();
    info!("Campaign AI: {} faction turns, {orders} orders, {accepted} accepted by the model", new.len());
    log.shown = reports.len();
    for line in refusals_to_log(&mut log.refusals_logged, new) {
        warn!("{line}");
    }
}

/// The lines for the refused orders of `reports` not logged before: one per faction and kind of refusal
/// (the error variant, not its numbers, so a faction refused for money every turn logs once), recorded in
/// `logged`. An order the model refuses is an AI bug to see once, not a per-turn line.
fn refusals_to_log(logged: &mut RefusalKinds, reports: &[AiTurnReport]) -> Vec<String> {
    let mut out = Vec::new();
    for r in reports {
        for (order, err) in &r.rejected {
            if logged.insert((r.faction, std::mem::discriminant(err))) {
                out.push(format!("Campaign AI: faction {} order refused by the model ({err}): {order:?}", r.faction.raw()));
            }
        }
    }
    out
}

/// Registers the log system.
pub struct CampaignAiPlugin;

impl Plugin for CampaignAiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, log_reports.run_if(in_state(crate::GameMode::Campaign)));
    }
}

#[cfg(test)]
mod tests {
    use ntw_ai::campaign::AiOrder;
    use ntw_sim::campaign::{CommandError, FactionId, RegionId};

    use super::*;

    fn report(faction: i32, turn: u32, rejected: Vec<CommandError>) -> AiTurnReport {
        let order = AiOrder::Recruit { region: RegionId(1), unit_key: "u".into() };
        AiTurnReport { faction: FactionId(faction), turn, orders: vec![order.clone()], accepted: 0, rejected: rejected.into_iter().map(|e| (order.clone(), e)).collect() }
    }

    /// Review (0b-recruit): the "log once" key was the error's text, which carries numbers, so a faction
    /// refused for money every turn logged every turn. It is the faction and the error variant now.
    #[test]
    fn a_refusal_kind_is_logged_once_per_faction_whatever_its_numbers() {
        let mut logged = RefusalKinds::new();
        let poor = |needed, available| CommandError::InsufficientFunds { needed, available };
        let turns = [
            report(1, 1, vec![poor(400, 10), CommandError::NoRecruitmentCapacity]),
            report(1, 2, vec![poor(450, 3)]),
            report(2, 2, vec![poor(400, 10)]),
            report(1, 3, vec![poor(500, -20), CommandError::UnitCapReached("u".into())]),
        ];
        let lines: Vec<usize> = turns.iter().map(|r| refusals_to_log(&mut logged, std::slice::from_ref(r)).len()).collect();
        // Turn 1: money and queue; turn 2: nothing new for faction 1, money once for faction 2; turn 3: the cap.
        assert_eq!(lines, [2, 0, 1, 1]);
        assert_eq!(logged.len(), 4);
    }
}
