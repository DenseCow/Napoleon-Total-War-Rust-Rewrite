//! The campaign camera a save holds: `CAMPAIGN_ENV/CAMPAIGN_CAMERA_MANAGER` v4 (CONFIRMED from the
//! exe, `analysis/fidelity/UI_FIDELITY.md` §5 "The camera in the save"), kept as plain data so the
//! map display starts a loaded game where it was saved without reading `.esf` itself.
//!
//! The writer `0x0090FC90` (record version `0x004CDC70` = 4) writes a u32 count of named cameras
//! (manager +0x38, entries of 16 bytes from +0x3C), then per entry its utf16 key and its camera, and
//! last the current camera (`0x0092C250`). Each camera (`0x0090FB20`, `CAMPAIGN_CAMERA` version
//! `0x004CDC40` = 2) is: f32 tilt (+0x138), u32 (+0x13C), coord3d target (+0x14/+0x18/+0x1C:
//! east, height, north), bool (+0x140).
//!
//! The reader `0x0090E5A0` (called only from the `CAMPAIGN_ENV` loader `0x00987070`, and only when
//! that record's version is above 1) reads the named cameras (each through the read constructor
//! `0x00900C00`), then, when the manager's version is above 1, the current camera, copied in with
//! `0x0090E120`; a manager below version 4 has its current camera's u32 and bool cleared
//! (`0x0094F9D0`). `0x00900C00` starts u32 = 0 and bool = false, reads the bool only from a camera
//! of version 2 or later, and places the eye from the tilt (`0x009208B0`, the tilt/distance law of
//! `napoleon::campaign::camera`).
//!
//! The u32 (+0x13C) counts the camera's running controllers and the bool (+0x140) is the pre-battle
//! hold; either makes the camera ignore input ([`CameraView::holds_input`]). The current camera's
//! copy `0x0090E120` keeps both, so a loaded camera keeps them (0 and false in every vanilla save
//! seen); they are kept as read. No vanilla save or start position has a named camera.
//!
//! After the reader, the loader replaces the current camera only on a new multiplayer campaign
//! ([`opening_camera`]); a loaded game and a new single-player game open on the file's camera.

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::CampaignModel;
use serde::{Deserialize, Serialize};

/// One campaign camera as the save stores it (`CAMPAIGN_CAMERA`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraView {
    /// The tilt (+0x138), from which the exe places the eye (`0x009208B0`): 0.5 at the farthest
    /// zoom, 1.0 at the nearest.
    pub tilt: f32,
    /// +0x13C: the count of camera controllers running on this camera (CONFIRMED: one more when
    /// `0x00925E20` installs a controller, `0x00955800`; one less at `0x00925E33` (inside `0x00925E20`) and at `0x0096E560` when one ends
    /// (`0x0096F280`), when a named camera is restored (`0x00968300`) and when the turn's start
    /// restores the `start_of_turn` camera (`0x00A13BF0`)). Kept as read (0 in every vanilla save seen); while it is not 0 the camera
    /// takes no input ([`Self::holds_input`]).
    pub field_13c: u32,
    /// The point looked at (+0x14/+0x18/+0x1C), in logic units: east, height, north. The height is
    /// 0 in every vanilla save seen (the new-game view of `0x00987070` sets it to 0 too).
    pub target: [f32; 3],
    /// +0x140: the pre-battle hold (CONFIRMED: set only by the pre-battle panel `0x009994B0` through
    /// `0x00955810`, after it moves the camera to the battle; cleared on every camera of the
    /// manager by `0x0096E580`, which only the campaign UI's listener `0x00A13DA0`, vtable
    /// `0x0136B808` slot 1 at UI +0x9C4, calls; which event reaches that listener is UNKNOWN).
    /// Kept as read (false in every vanilla save seen); while it is set the camera takes no input
    /// ([`Self::holds_input`]).
    pub flag_140: bool,
}

impl CameraView {
    /// Whether this camera ignores the player's input: the camera's busy test `0x00955820`, true
    /// while a controller runs (+0x13C not 0) or the pre-battle hold is set (+0x140) (CONFIRMED).
    /// It gates every camera key binding (`0x0095A510`, `0x0095A591`) and the wheel (`0x00A0EDF0`,
    /// `0x00A0EF7F`; a wheel turn while busy is deferred, `0x00A0EF86`, which ours drops), and the mouse update `0x00970A90` twice (`0x00970C0D`, `0x00970D52`).
    pub fn holds_input(&self) -> bool {
        self.field_13c != 0 || self.flag_140
    }

    /// A camera looking at logic (`east`, `north`) with `tilt`, its unknown fields as the read
    /// constructor `0x00900C00` starts them (0, false).
    pub fn looking_at(east: f32, north: f32, tilt: f32) -> Self {
        Self { tilt, field_13c: 0, target: [east, 0.0, north], flag_140: false }
    }
}

/// The camera block of a save: the current camera and the named ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraSave {
    /// The camera the game was saved with (the one a load opens on).
    pub current: CameraView,
    /// The named cameras (key, camera), in file order. What names them is UNKNOWN; none in any vanilla
    /// file seen. Kept so a save carries them on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub named: Vec<(String, CameraView)>,
}

/// The camera block of a save or start position, or `None` when the file has none (a `CAMPAIGN_ENV`
/// of version 1 or less, which the exe does not read it from, or a manager of version 1 or less,
/// which has no current camera; the game keeps its default camera then). A block whose fields do not
/// have the written types, or a `CAMPAIGN_ENV` of version 2 or later with no manager record, is
/// logged and read as `None`.
pub fn read(esf: &EsfFile) -> Option<CameraSave> {
    let env = esf.root.child("CAMPAIGN_ENV")?;
    if env.version <= 1 {
        return None;
    }
    let Some(manager) = env.child("CAMPAIGN_CAMERA_MANAGER") else {
        log::warn!("CAMPAIGN_ENV v{} has no CAMPAIGN_CAMERA_MANAGER record; the camera is not restored", env.version);
        return None;
    };
    let parsed = read_manager(manager);
    if parsed.is_none() && manager.version > 1 {
        log::warn!("CAMPAIGN_CAMERA_MANAGER v{} is not in the written layout; the camera is not restored", manager.version);
    }
    parsed
}

/// The camera a campaign opens on, as the `CAMPAIGN_ENV` loader `0x00987070` leaves it (CONFIRMED):
/// the file's own camera `read` ([`read`]), a start position's on a new game as a save's on a load,
/// except on a new multiplayer campaign. Only when the loader's new-game argument is set (`0x0098860F`;
/// set by the new-campaign caller `0x008B8930`, null from the loaded-game caller `0x008B8A60`) and
/// campaign +0x1034 is set (`0x00988620`; the multiplayer flag: the victory check `0x0096CD10` tests it
/// at `0x0096CEA9`, the loader `0x00872550` reads it as a bool at `0x00873247`) does it replace the
/// current camera (`0x0090E120` into `0x0092C250`), also when the file has no camera block (the
/// version test at `0x009885FA` skips only the reader), with one built by `0x00900E50`: looking at
/// a capital settlement (`0x008B3790`: faction +0x72C → region +0xFC, its position × 2^-20) at
/// height 0, its tilt the minimum tilt tweaker (`0x015C4078`, stored at `0x00900F23`) clamped to the
/// world's zoom limit by the tilt step `0x00920D00` (`ZoomLimit::clamp`), u32 0 and bool false; the
/// eye at height 100 the loader passes is replaced by the tilt law's
/// (`0x00920D00` → `0x009208B0`). The named cameras stay.
///
/// Which capital: the loader walks the campaign's faction list (+0xF5C), skips each faction whose
/// capital is null (`0x0098864D`), and takes the first whose key (`0x008E5060`) is the loader's
/// faction key ([EBX+0x34]; `human_key` here). A walk with no match (the human faction has no
/// capital, or no faction has its key) jumps past the camera code (`0x00988764`) and keeps the
/// file's camera (CONFIRMED). Faction keys are unique, so the walk's order does not change the match.
pub fn opening_camera(read: Option<CameraSave>, new_multiplayer_game: bool, model: &CampaignModel, human_key: Option<&str>) -> Option<CameraSave> {
    if !new_multiplayer_game {
        return read;
    }
    let capital = human_key.and_then(|key| {
        model.world.factions.values().find_map(|f| {
            let region = model.world.capital(f.id).and_then(|r| model.world.regions.get(&r))?;
            (f.key == key).then_some(region)
        })
    });
    let Some(region) = capital else {
        log::debug!("new multiplayer campaign: no faction with a capital has the key {human_key:?}; the camera stays on the file's");
        return read;
    };
    let (east, north) = (region.settlement.position.0.to_f32(), region.settlement.position.1.to_f32());
    // The minimum tilt tweaker, clamped by the tilt step `0x00920D00` to the zoom limit the model
    // loaded just before (`0x00872550` at `0x00987FFD`, ahead of the camera code).
    let tilt = model.world.zoom_limit.clamp(ntw_sim::campaign::zoom_limit::MIN_TILT);
    let current = CameraView::looking_at(east, north, tilt);
    Some(CameraSave { current, named: read.map(|c| c.named).unwrap_or_default() })
}

fn read_manager(manager: &EsfRecord) -> Option<CameraSave> {
    let mut nodes = manager.children.iter();
    let count = nodes.next()?.as_u32()?;
    let mut named = Vec::new();
    for _ in 0..count {
        let key = nodes.next()?.as_str()?.to_string();
        named.push((key, read_camera(nodes.next()?.as_record()?)?));
    }
    if manager.version <= 1 {
        // The exe keeps them (in the manager, beside its default current camera); ours has no camera
        // block without a current camera, so they are dropped, logged (none in any vanilla file).
        if !named.is_empty() {
            log::warn!("CAMPAIGN_CAMERA_MANAGER v{} has {} named cameras and no current camera; the named cameras are not kept", manager.version, named.len());
        }
        return None;
    }
    let mut current = read_camera(nodes.next()?.as_record()?)?;
    if manager.version < 4 {
        current.field_13c = 0;
        current.flag_140 = false;
    }
    Some(CameraSave { current, named })
}

fn read_camera(rec: &EsfRecord) -> Option<CameraView> {
    if rec.name != "CAMPAIGN_CAMERA" {
        return None;
    }
    let tilt = rec.get(0)?.as_f32()?;
    let field_13c = rec.get(1)?.as_u32()?;
    let EsfNode::Coord3d(x, y, z) = *rec.get(2)? else { return None };
    let flag_140 = if rec.version > 1 { rec.get(3)?.as_bool()? } else { false };
    Some(CameraView { tilt, field_13c, target: [x, y, z], flag_140 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(version: u8, tilt: f32, n: u32, t: (f32, f32, f32), flag: Option<bool>) -> EsfNode {
        let mut r = EsfRecord::new("CAMPAIGN_CAMERA", version);
        r.children = vec![EsfNode::F32(tilt), EsfNode::U32(n), EsfNode::Coord3d(t.0, t.1, t.2)];
        r.children.extend(flag.map(EsfNode::Bool));
        EsfNode::Record(Box::new(r))
    }

    fn file(env_version: u8, manager_version: u8, children: Vec<EsfNode>) -> EsfFile {
        let mut manager = EsfRecord::new("CAMPAIGN_CAMERA_MANAGER", manager_version);
        manager.children = children;
        let mut env = EsfRecord::new("CAMPAIGN_ENV", env_version);
        env.children = vec![EsfNode::Bool(false), EsfNode::Record(Box::new(manager))];
        let mut root = EsfRecord::new("CAMPAIGN_SAVE_GAME", 0);
        root.children = vec![EsfNode::Record(Box::new(env))];
        EsfFile::new(root)
    }

    #[test]
    fn reads_the_current_and_named_cameras() {
        let f = file(
            2,
            4,
            vec![
                EsfNode::U32(1),
                EsfNode::Utf16String("made_up_view".into()),
                camera(2, 0.9, 3, (1.0, 0.0, 2.0), Some(true)),
                camera(2, 0.65, 7, (-64.0, 0.0, -4.0), Some(true)),
            ],
        );
        let c = read(&f).expect("camera");
        assert_eq!(c.current, CameraView { tilt: 0.65, field_13c: 7, target: [-64.0, 0.0, -4.0], flag_140: true });
        assert_eq!(c.named, vec![("made_up_view".to_string(), CameraView { tilt: 0.9, field_13c: 3, target: [1.0, 0.0, 2.0], flag_140: true })]);
    }

    #[test]
    fn old_versions_follow_the_reader() {
        // A manager below v4 clears the current camera's u32 and bool (`0x0094F9D0`).
        let c = read(&file(2, 3, vec![EsfNode::U32(0), camera(2, 0.7, 5, (1.0, 0.0, 1.0), Some(true))])).unwrap();
        assert_eq!((c.current.field_13c, c.current.flag_140), (0, false));
        // A v1 camera has no bool.
        let c = read(&file(2, 4, vec![EsfNode::U32(0), camera(1, 0.7, 5, (1.0, 0.0, 1.0), None)])).unwrap();
        assert_eq!((c.current.field_13c, c.current.flag_140), (5, false));
        // A v1 manager has no current camera; a v1 CAMPAIGN_ENV is not read at all.
        assert_eq!(read(&file(2, 1, vec![EsfNode::U32(0)])), None);
        assert_eq!(read(&file(1, 4, vec![EsfNode::U32(0), camera(2, 0.7, 5, (1.0, 0.0, 1.0), Some(false))])), None);
    }

    #[test]
    fn only_a_new_multiplayer_game_replaces_the_files_camera() {
        let model = CampaignModel::new(
            ntw_sim::calendar::Calendar::new(ntw_sim::calendar::Date { year: 1805, season: 1, month: 0, half: 0 }, 0),
            ntw_sim::rng::CaRng::new(7),
            Default::default(),
        );
        let file_camera = CameraSave { current: CameraView::looking_at(-54.0, 151.12, 0.5), named: vec![("v".into(), CameraView::looking_at(1.0, 2.0, 0.7))] };
        // Single player (a new game or a load): the file's camera, named ones and all.
        assert_eq!(opening_camera(Some(file_camera.clone()), false, &model, Some("france")), Some(file_camera.clone()));
        assert_eq!(opening_camera(None, false, &model, Some("france")), None);
        // Multiplayer with no faction of that key holding a capital (`0x0098864D`): the file's camera.
        assert_eq!(opening_camera(Some(file_camera.clone()), true, &model, Some("france")), Some(file_camera));
        assert_eq!(opening_camera(None, true, &model, None), None);
    }

    #[test]
    fn a_misshapen_block_is_none() {
        assert_eq!(read(&file(2, 4, vec![EsfNode::U32(0), EsfNode::F32(1.0)])), None);
        // A v2 CAMPAIGN_ENV without the manager record (logged, not read).
        let mut env = EsfRecord::new("CAMPAIGN_ENV", 2);
        env.children = vec![EsfNode::Bool(false)];
        let mut root = EsfRecord::new("CAMPAIGN_SAVE_GAME", 0);
        root.children = vec![EsfNode::Record(Box::new(env))];
        assert_eq!(read(&EsfFile::new(root)), None);
    }

    /// The busy test `0x00955820`: a running controller (+0x13C not 0) or the pre-battle hold
    /// (+0x140) holds the input; a fresh camera holds none.
    #[test]
    fn a_controller_or_the_battle_hold_holds_the_input() {
        let free = CameraView::looking_at(0.0, 0.0, 0.7);
        assert!(!free.holds_input());
        assert!(CameraView { field_13c: 1, ..free }.holds_input());
        assert!(CameraView { flag_140: true, ..free }.holds_input());
    }
}
