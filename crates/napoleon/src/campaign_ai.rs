//! The campaign AI hook: `ntw_ai` plays every non-human faction during End Turn.
//!
//! `--campaign-ai on|off` (default on). The AI is installed on the campaign's script host
//! (`ntw_ai::campaign::driver::install`), so it runs inside the model's turn loop at each AI
//! faction's `AiTurn` step and the original scripts see every event its orders cause. Its orders
//! go through the model's validated commands; everything is deterministic (campaign RNG).

use std::cell::RefCell;
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
}

/// True unless `--campaign-ai off`.
fn enabled() -> bool {
    let args: Vec<String> = std::env::args().collect();
    !args.windows(2).any(|w| w[0] == "--campaign-ai" && w[1] == "off")
}

/// Installs the campaign AI on `host`; `startpos` is the startpos or save the model was read from (called by the campaign scene before turn 1 starts, so AI
/// factions before the human in turn order play too).
pub fn attach(world: &mut World, host: &mut ScriptHost, vfs: &Vfs, startpos: &[u8]) {
    if !enabled() {
        info!("Campaign AI: off (--campaign-ai off)");
        return;
    }
    let data = match CampaignAiData::load(vfs, &world.resource::<GameData>().db) {
        Ok(d) => d,
        Err(e) => {
            warn!("Campaign AI: AI tables unavailable ({e}); AI factions will not act");
            return;
        }
    };
    // Each faction's manager and personality keys are stored in its FACTION record (CONFIRMED).
    let (keys, values) = match ntw_formats::esf::EsfFile::from_bytes(startpos) {
        Ok(esf) => (ntw_ai::campaign::keys::read_ai_keys(&esf.root), ntw_ai::campaign::keys::read_region_base_values(&esf.root)),
        Err(e) => {
            warn!("Campaign AI: cannot re-read the startpos for the AI keys ({e}); using the fallback rule");
            Default::default()
        }
    };
    let reports = driver::install_with(host, Arc::new(data), keys, values);
    world.insert_non_send(CampaignAiLog { reports, shown: 0 });
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
}

/// Registers the log system.
pub struct CampaignAiPlugin;

impl Plugin for CampaignAiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, log_reports.run_if(in_state(crate::GameMode::Campaign)));
    }
}
