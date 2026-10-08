//! The campaign map (`GameMode::Campaign`): the original map of a campaign drawn from the
//! install, with settlements, armies and agents from the start position.
//!
//! Entry points:
//! - Command line harness: `--campaign <name>` (e.g. `eur_napoleon`), optionally with
//!   `--screenshot <file.png>`. `--campaign list` prints the campaigns.
//! - From other code (e.g. the front end's campaign-selection page): insert a [`CampaignStart`]
//!   resource and switch the state to `GameMode::Campaign`.
//!
//! Coordinates: Bevy `(x, y, z)` = logic map `(x, height, -z)`, 1 Bevy unit = 1 logic map unit
//! (x = east, logic z = north, so north is Bevy -Z, as in the battle view).
//!
//! What is from the files (see `analysis/campaign/CAMPAIGN_MAP.md`): heightmap, supertexture
//! colour, border/river/road splines, region data, settlement and character positions, the
//! camera limits (CampaignCamera tweaker defaults). PROVISIONAL / PLACEHOLDER: height scale,
//! line styling (plain lines instead of the original textured ribbons), lighting, the
//! settlement model choice, and the faction markers.

mod arrows;
mod camera;
mod detail;
mod hud;
mod play;
mod region_labels;
mod scene;

use bevy::prelude::*;

use crate::GameMode;

/// Which campaign to show. Insert this before entering `GameMode::Campaign`.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct CampaignStart {
    /// Campaign folder under `data\campaigns\`, e.g. `eur_napoleon`.
    pub campaign: String,
    /// The faction the player chose (a `factions` key). `None` = the start position's default
    /// (its `SAVE_GAME_HEADER` faction, e.g. france in eur_napoleon).
    pub faction: Option<String>,
    /// A save file to load instead of the start position (read-only; e.g. one of the original
    /// game's saves picked on the Load Game page). The campaign key then comes from the save.
    pub save: Option<std::path::PathBuf>,
}

impl Default for CampaignStart {
    fn default() -> Self {
        Self { campaign: "eur_napoleon".to_owned(), faction: None, save: None }
    }
}

/// `--campaign <name>` from the command line, if given. `--campaign list` prints the
/// campaign folders and exits.
pub fn start_from_args(args: &[String]) -> Option<CampaignStart> {
    let i = args.iter().position(|a| a == "--campaign")?;
    let name = args.get(i + 1).filter(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| CampaignStart::default().campaign);
    if name == "list" {
        match ntw_formats::pack::Vfs::open_install(crate::config::game_data_dir()) {
            Ok(vfs) => {
                let dir = crate::config::game_data_dir();
                let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(&dir) };
                let mut names: Vec<String> = files
                    .list("campaigns/")
                    .iter()
                    .filter(|p| p.ends_with("\\startpos.esf"))
                    .filter_map(|p| p.split('\\').nth(1).map(str::to_owned))
                    .collect();
                names.dedup();
                names.iter().for_each(|n| println!("{n}"));
            }
            Err(e) => eprintln!("cannot open the install: {e}"),
        }
        std::process::exit(0);
    }
    let faction = args.iter().position(|a| a == "--campaign-faction").and_then(|i| args.get(i + 1)).cloned();
    let save = args.iter().position(|a| a == "--campaign-save").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    Some(CampaignStart { campaign: name, faction, save })
}

/// Draws the campaign map while in `GameMode::Campaign`.
pub struct CampaignPlugin;

impl Plugin for CampaignPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameMode::Campaign), scene::enter)
            .add_systems(OnExit(GameMode::Campaign), play::leave)
            .add_systems(
                Update,
                (
                    hud::harness,
                    hud::pointer,
                    play::input,
                    camera::control,
                    play::sync_markers,
                    scene::sync_walls,
                    play::animate,
                    play::preview,
                    arrows::draw,
                    region_labels::update,
                    hud::update,
                    hud::labels,
                    hud::radar_outline,
                    hud::redraw,
                )
                    .chain()
                    .run_if(in_state(GameMode::Campaign)),
            )
            .add_systems(Update, (scene::fps_log, detail::update.after(camera::control)).run_if(in_state(GameMode::Campaign)));
    }
}

/// `--campaign-demo` (screenshot harness): selects the human player's first army and shows its
/// path towards a point 4 units south of the nearest settlement owned by a faction it is at war
/// with (a plain move, so `--campaign-demo-move` shows the walk animation).
fn demo(sim: &mut play::CampaignSim) {
    let Some(human) = sim.human_id() else { return };
    if std::env::args().any(|a| a == "--campaign-demo-zoc") {
        demo_past_enemy(sim, human);
        return;
    }
    let pick = {
        let m = sim.model();
        m.world.forces.values().filter(|f| f.faction == human && !f.is_navy).find_map(|f| {
            let c = m.world.characters.get(&f.commander?)?;
            let (x, z) = (c.position.0.to_f32(), c.position.1.to_f32());
            let target = m
                .world
                .regions
                .values()
                .filter(|r| m.world.stance(human, r.owner) == ntw_sim::campaign::Stance::War)
                .map(|r| (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32() - 4.0))
                .min_by(|a, b| ((a.0 - x).hypot(a.1 - z)).total_cmp(&(b.0 - x).hypot(b.1 - z)))?;
            Some((c.id, target))
        })
    };
    if let Some((id, target)) = pick {
        sim.selected = Some(id);
        sim.demo_target = Some(target);
        info!("Campaign demo: character {} -> ({:.1}, {:.1})", id.0, target.0, target.1);
    }
}

/// `--campaign-demo --campaign-demo-zoc` (screenshot harness for zones of control): selects the
/// human player's field army with an enemy field army 25..70 units away and targets a point 18
/// units beyond that enemy, so the shown path bends round the enemy's zone.
fn demo_past_enemy(sim: &mut play::CampaignSim, human: ntw_sim::campaign::FactionId) {
    let pick = {
        let m = sim.model();
        let pos = |f: &ntw_sim::campaign::MilitaryForce| {
            let c = m.world.characters.get(&f.commander?)?;
            c.garrisoned_in.is_none().then(|| (c.id, c.position.0.to_f32(), c.position.1.to_f32()))
        };
        m.world.forces.values().filter(|f| f.faction == human && !f.is_navy).filter_map(pos).find_map(|(id, x, z)| {
            m.world
                .forces
                .values()
                .filter(|e| !e.is_navy && m.world.stance(human, e.faction) == ntw_sim::campaign::Stance::War)
                .filter_map(pos)
                .map(|(_, ex, ez)| (ex, ez, (ex - x).hypot(ez - z)))
                .filter(|&(_, _, d)| (25.0..70.0).contains(&d))
                .min_by(|a, b| a.2.total_cmp(&b.2))
                .map(|(ex, ez, d)| (id, (ex + (ex - x) / d * 18.0, ez + (ez - z) / d * 18.0)))
        })
    };
    if let Some((id, target)) = pick {
        sim.selected = Some(id);
        sim.demo_target = Some(target);
        info!("Campaign demo (zones of control): character {} -> ({:.1}, {:.1})", id.0, target.0, target.1);
    }
}
