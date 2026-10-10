//! Sound: music, UI sounds, battle and campaign sounds, played from the original's own
//! sound tables (`sounds_packed\sound_events` + `sound_bank_database`, see
//! `analysis/audio/AUDIO_FORMAT.md`) and sound files, all through the Vfs.
//!
//! # Hooks for other systems (send these Bevy messages)
//! - [`PlaySound`]: play an event by built-in slot name, by event name, or by index, in 2D or
//!   at a world position.
//! - [`UiSound`]: a UI component was clicked (plays the `ui` event named after its id).
//! - [`ProjectileFired`]: a weapon fired (gun type + shot type from the `projectiles` table);
//!   the projectile-fire bank picks the sound by distance. Battle volleys are already bridged
//!   automatically from `battle::VolleyFx` (see [`battle`]).
//! - [`SetMusic`]: change the music state (`music_front_end`, `music_campaign` + subculture,
//!   `music_land_battle`, ...) or stop the music.
//! - [`CampaignAmbience`]: start/stop the campaign map's positional ambience emitters.
//! - [`AudioListener`]: put on the camera that hears 3D sounds (default: the first `Camera3d`).
//!
//! Volumes come from our own preferences copy (`%APPDATA%\NapoleonRust\scripts\
//! preferences.script.txt`, seeded read-only from the original's file), re-read when it changes.

mod battle;
mod clip;
pub mod mixer;
mod speakers;
pub mod feed;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bevy::audio::AddAudioSource;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ntw_formats::pack::Vfs;
use ntw_formats::sound::loop_points::LoopIndex;
use ntw_formats::sound::names::normalize_sound_path;
use ntw_formats::sound::{decode, decode_timed_i16, Param, Pcm, Pcm16, SharedBytes, SoundLibrary, SoundParams};

use crate::config;
use crate::GameMode;
use clip::{ClipData, Samples, VoiceClip, VoiceControl};
pub use feed::PcmFeed;

// ---------------------------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------------------------

/// Which event to play. (A hook: other systems construct these.)
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundRef {
    /// A built-in slot, e.g. `"DRUM_FIRE"`, `"THUNDER"` (`ntw_formats::sound::slots`).
    Slot(String),
    /// A named event: UI component ids, unit-voice names, or CSV event names.
    Named(String),
    /// An event index in `sound_events`.
    Event(usize),
}

/// Play one sound event. `position: None` plays it in 2D.
#[derive(Message, Debug, Clone)]
pub struct PlaySound {
    pub sound: SoundRef,
    pub position: Option<Vec3>,
}

/// A UI event on a component (by its layout id, e.g. `"grand_campaign"`); the sound follows the
/// exe's UI sound handler (MIDDLEWARE_VERIFY.md §2).
#[derive(Message, Debug, Clone)]
pub struct UiSound {
    pub component: String,
    pub kind: UiEvent,
}

impl UiSound {
    /// A left click (the usual case).
    pub fn click(component: impl Into<String>) -> Self {
        Self { component: component.into(), kind: UiEvent::LClickUp }
    }
}

/// The UI events that make a sound (the UI `EVENT_NAMES` the handler `0x0047F0C0` reacts to).
#[allow(dead_code)] // Shortcut and Move: no UI here reports shortcuts or slider drags yet
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiEvent {
    /// OnMouseLClickUp (3).
    #[default]
    LClickUp,
    /// OnShortcut (16): as a left click.
    Shortcut,
    /// OnMouseRClickUp (6).
    RClickUp,
    /// OnMove (13): a slider moved.
    Move,
    /// OnMouseOn (21): the pointer entered the component.
    MouseOn,
}

/// What a UI event plays: a built-in slot or a `ui` event name (CONFIRMED rules, §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiSoundChoice {
    Slot(&'static str),
    Named(String),
}

/// The sound candidates for a UI event, in the order the handler tries them (the first that exists
/// plays; `DEFAULT_UI_SOUND` is the last resort for clicks).
pub fn ui_sound_choices(kind: UiEvent, id: &str) -> Vec<UiSoundChoice> {
    use UiSoundChoice::{Named, Slot};
    match kind {
        UiEvent::LClickUp | UiEvent::Shortcut if id.starts_with("entry_") => vec![Slot("UNIT_CARD_SELECTED")],
        UiEvent::LClickUp | UiEvent::Shortcut => vec![Named(id.to_owned()), Slot("DEFAULT_UI_SOUND")],
        UiEvent::RClickUp if id.starts_with("entry_") => vec![Slot("UNIT_CARD_RIGHT_CLICK_SELECTED")],
        UiEvent::RClickUp => vec![Named(format!("right_click_{id}"))],
        UiEvent::Move => vec![Named(format!("slider_moved_{id}"))],
        UiEvent::MouseOn if id.starts_with("item") => vec![Named("mouse_over_Slot1".into())],
        UiEvent::MouseOn => {
            let mut v = vec![Named(format!("mouse_over_{id}"))];
            if ["entry_art_", "entry_inf_", "entry_cav_"].iter().any(|p| id.starts_with(p)) {
                v.push(Slot("mouse_over_unit_card"));
            }
            v
        }
    }
}

/// A weapon fired: what its sound depends on ([`ProjectileSound`], shared, so a volley allocates
/// nothing), where, and `shots` = number of men/guns firing together.
#[derive(Message, Debug, Clone)]
pub struct ProjectileFired {
    pub sound: Arc<ProjectileSound>,
    pub position: Vec3,
    pub shots: u32,
}

/// What a projectile's fire sound depends on, resolved once per `projectiles` row (the battle
/// bridge keeps them). `gun_type` = `projectiles.weapon_family` (e.g. `musket_flintlock`,
/// `cannon`), `shot_type` = `projectiles.shot_type` (e.g. `bullet`, `round_shot`); names as in
/// `sound_bank_projectile_fire.xml`. `kind` = [`ProjectileKind::of`] the row's category and
/// missile type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileSound {
    pub gun_type: String,
    pub shot_type: String,
    pub kind: Option<ProjectileKind>,
}

/// Change the music. `state: None` stops it. States are the `music_state` names of
/// `sound_bank_music_states.xml` (`music_front_end`, `music_campaign`, `music_land_deployment`,
/// `music_land_battle`, `music_land_battle_results`, `music_naval_battle`, ...); `subculture`
/// (e.g. `sc_european_west`) picks the campaign tracks.
#[derive(Message, Debug, Clone, Default)]
pub struct SetMusic {
    pub state: Option<String>,
    pub subculture: Option<String>,
}

/// Start (`Some(map)`, e.g. `"nap_europe"`) or stop (`None`) a campaign map's ambience emitters.
/// Positions are the original campaign-map coordinates, mapped to our world by
/// [`CampaignToWorld`].
#[derive(Message, Debug, Clone)]
pub struct CampaignAmbience {
    pub map: Option<String>,
}

/// Maps original campaign-map coordinates (x, height, z) to our world. Identity until the
/// campaign view sets it (PROVISIONAL).
#[derive(Resource, Debug, Clone, Copy)]
pub struct CampaignToWorld(pub Mat4);

impl Default for CampaignToWorld {
    fn default() -> Self {
        Self(Mat4::IDENTITY)
    }
}

/// Play a movie's sound track (sent by `crate::video`): the samples arrive live through `feed`;
/// `movie` is the movie's path or file name, for its volume multiplier in `sound_events`
/// (`movie_volumes.csv`). Stop it with [`PcmFeed::stop`].
#[derive(Message, Debug, Clone)]
pub struct PlayMovieAudio {
    pub feed: Arc<PcmFeed>,
    pub movie: String,
}

/// Marks the entity whose transform is the 3D listener.
#[derive(Component, Debug, Default)]
pub struct AudioListener;

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

/// The loaded sound data (absent if the install has none).
#[derive(Resource, Clone)]
pub struct SoundData {
    pub lib: Arc<SoundLibrary>,
    pub vfs: Arc<Vfs>,
    /// Each event's files, normalized ([`normalize_sound_path`]) once at load: the keys of the
    /// clip and music caches and of the failed files, so a pick allocates nothing.
    paths: Arc<[Box<[Arc<str>]>]>,
    bank_projectile_fire: Option<u32>,
    bank_music: Option<u32>,
    /// The (close, medium) audio-distance bands of each [`ProjectileKind`] (indexed by it), read
    /// once ([`ProjectileKind::bands`]).
    projectile_bands: [(f32, f32); ProjectileKind::BANDS.len()],
    /// Set once the "no file paths" error was logged: it is said once per build, not per process.
    missing_paths_logged: Arc<std::sync::atomic::AtomicBool>,
}

/// The land projectile-fire sound kinds of the `AUDIO_DISTANCE_LAND_PROJECTILES_{kind}_CLOSE` /
/// `_MEDIUM` settings. INFERRED: which kinds a land volley uses and how a projectile picks its
/// kind ([`ProjectileKind::of`]); the exe's reader of these settings is not found yet (no inline
/// read of their `sound_bank_database` slots 81..86 at the settings array, `+0x144..+0x158`, and
/// no bounds check against those indices as `0x00E31F40` does for the mixing settings).
/// PLACEHOLDER: the land `EXPLOSIONS` / `IMPACTS` bands (slots 87..90) and the whole naval set
/// (`AUDIO_DISTANCE_NAVAL_*`, slots 91..102) have no kind: explosion and impact sounds and naval
/// volleys are not played yet (AUDIO_FORMAT.md, projectile audio-distance settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileKind {
    SmallArms,
    Arrow,
    Artillery,
}

/// One [`ProjectileKind`]'s distance-band settings ([`ProjectileKind::BANDS`]).
struct KindBands {
    kind: ProjectileKind,
    stem: &'static str,
    slots: (usize, usize),
    shipped: (f32, f32),
}

impl ProjectileKind {
    /// Per kind, in discriminant order: its setting name stem, the packed `sound_bank_database`
    /// settings slots of its (close, medium) bands and their shipped values. Slots and values
    /// CONFIRMED shipped data (`sound_probe settings`: the slots in `sound_settings.xml` element
    /// order, the XML and packed values agree).
    const BANDS: [KindBands; 3] = [
        KindBands { kind: Self::SmallArms, stem: "SMALLARMS", slots: (83, 84), shipped: (350.0, 1000.0) },
        KindBands { kind: Self::Arrow, stem: "ARROW", slots: (81, 82), shipped: (200.0, 500.0) },
        KindBands { kind: Self::Artillery, stem: "ARTILLERY", slots: (85, 86), shipped: (50.0, 300.0) },
    ];

    /// The kind of a `projectiles` row by its category (column 1) and missile type (column 3),
    /// ignoring case: land guns (`artillery`, `fort_battery`, `rocket`) are artillery; a `missile`
    /// is small arms with `bullet`, arrow with `arrow`, artillery with `cannon_ball`. `None` for
    /// the rest: `naval` (the naval bands are a PLACEHOLDER) and, shipped, grenades, axes,
    /// javelins, chakram, grapple, grape and fragment shrapnel and fougasse; they play with the
    /// artillery bands (PROVISIONAL, noted once by the battle bridge). For every shipped
    /// `projectiles` row this gives the kind the old weapon-family rule gave (CONFIRMED by the
    /// install test `projectile_kinds_match_the_weapon_family_rule_on_every_shipped_row`); a row
    /// whose family is not a musket, pistol, airgun, camel gun or puckle but fires a `bullet` as a
    /// `missile` (a modded rifle) is now small arms, not artillery.
    pub fn of(category: &str, missile_type: &str) -> Option<Self> {
        let is = |s: &str, name: &str| s.eq_ignore_ascii_case(name);
        if ["artillery", "fort_battery", "rocket"].iter().any(|c| is(category, c)) {
            return Some(Self::Artillery);
        }
        if !is(category, "missile") {
            return None;
        }
        [("bullet", Self::SmallArms), ("arrow", Self::Arrow), ("cannon_ball", Self::Artillery)].into_iter().find(|(m, _)| is(missile_type, m)).map(|(_, k)| k)
    }

    /// Every kind's (close, medium) bands, indexed by kind: the packed slot (by slot, as the exe
    /// reads its mixing settings in `0x00E31F40`, so an edited packed table wins), else the
    /// shipped value, the fallbacks of this build logged in one warning.
    fn bands(lib: &SoundLibrary) -> [(f32, f32); Self::BANDS.len()] {
        let mut missing = Vec::new();
        let mut get = |stem: &str, band: &str, slot: usize, shipped: f32| {
            lib.banks.settings.get(slot).copied().unwrap_or_else(|| {
                missing.push(format!("AUDIO_DISTANCE_LAND_PROJECTILES_{stem}_{band} (slot {slot}, shipped {shipped} used)"));
                shipped
            })
        };
        let bands = Self::BANDS.map(|b| (get(b.stem, "CLOSE", b.slots.0, b.shipped.0), get(b.stem, "MEDIUM", b.slots.1, b.shipped.1)));
        if !missing.is_empty() {
            warn!("sound: the packed sound_bank_database has no {}", missing.join(", "));
        }
        bands
    }
}

// The band table is in kind order: `SoundData::projectile_bands` is indexed by `kind as usize`.
const _: () = {
    let mut i = 0;
    while i < ProjectileKind::BANDS.len() {
        assert!(ProjectileKind::BANDS[i].kind as usize == i);
        i += 1;
    }
};

impl SoundData {
    fn new(lib: SoundLibrary, vfs: Vfs) -> Self {
        let paths = lib.events.events.iter().map(|e| e.files.iter().map(|f| Arc::from(normalize_sound_path(f))).collect()).collect();
        let bank_projectile_fire = lib.vocabulary.bank_type_of("sound_bank_projectile_fire");
        if bank_projectile_fire.is_none() {
            // Every volley then plays nothing; said once per load.
            warn!("sound: no sound_bank_projectile_fire names (bank XML missing or unmatched): no projectile fire sounds");
        }
        let bank_music = lib.vocabulary.bank_type_of("sound_bank_music_states");
        let projectile_bands = ProjectileKind::bands(&lib);
        Self { lib: Arc::new(lib), vfs: Arc::new(vfs), paths, bank_projectile_fire, bank_music, projectile_bands, missing_paths_logged: Arc::default() }
    }
}

pub use mixer::VolumeGroup;

/// The sound manager's six volume groups (0 music, 1 sfx, 2 speech, 3 interface, 4 movie,
/// 5 master), each an enabled flag and an integer volume 0..=100, all constructed enabled at 100
/// (CONFIRMED `0x010009E0`). The preferences drive master / music / speech / sfx (`0x004837D0`);
/// interface stays 100; movie = `round(MOVIE_VOLUME × 100)` (MIDDLEWARE_VERIFY.md §1.2–1.3).
#[derive(Resource, Debug, Clone)]
pub struct Volumes {
    pub groups: [mixer::GroupVolume; 6],
    /// The original would open a headphones setup for these preferences ([`speakers`]): the
    /// headphones volume multiplier applies.
    headphones: bool,
    prefs_path: Option<PathBuf>,
    prefs_mtime: Option<std::time::SystemTime>,
    check_timer: f32,
}

impl Volumes {
    pub fn group(&self, g: VolumeGroup) -> mixer::GroupVolume {
        self.groups[g as usize]
    }

    fn load() -> Self {
        let user = config::user_dir();
        let prefs = ntw_script::ui::frontend::load_preferences(user.as_deref(), config::original_user_dir().as_deref());
        let group = |k: &str, en: &str| mixer::GroupVolume {
            enabled: prefs.get_bool(en).unwrap_or(true),
            volume: prefs.get_f64(k).unwrap_or(100.0).round().clamp(0.0, 100.0) as i32,
        };
        let path = user.map(|d| d.join("scripts").join("preferences.script.txt"));
        let mtime = path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        let mut groups = [mixer::GroupVolume::default(); 6];
        groups[VolumeGroup::Master as usize] = group("sound_master_volume", "sound_master_enabled");
        // `enable_sound` off silences everything (INFERRED: the master group is disabled).
        if !prefs.get_bool("enable_sound").unwrap_or(true) {
            groups[VolumeGroup::Master as usize].enabled = false;
        }
        groups[VolumeGroup::Music as usize] = group("sound_music_volume", "sound_music_enabled");
        groups[VolumeGroup::Speech as usize] = group("sound_speech_volume", "sound_speech_enabled");
        groups[VolumeGroup::Sfx as usize] = group("sound_sfx_volume", "sound_sfx_enabled");
        // `sound_provider` is an int preference, default 0 (registered at `0x00405140`).
        let provider = prefs.get_f64("sound_provider").map_or(0, |p| p.round() as i64);
        let headphones = speakers::opened_is_headphones(provider, speakers::system_speaker_config());
        Self { groups, headphones, prefs_path: path, prefs_mtime: mtime, check_timer: 0.0 }
    }

    /// Sets the movie group from `MOVIE_VOLUME` (CONFIRMED `round(MOVIE_VOLUME × 100)`).
    fn set_movie(&mut self, movie_volume: f32) {
        self.groups[VolumeGroup::Movie as usize].volume = (f64::from(movie_volume) * 100.0).round_ties_even() as i32;
    }
}

/// The mixing settings from `sound_bank_database` (MIDDLEWARE_VERIFY.md §1.2), read once.
#[derive(Resource, Debug, Clone, Copy)]
struct MixSettings {
    rolloff: f32,
    cutoff: f32,
    /// `SS_LOW_PASS_FILTER × 0.001`.
    low_pass_slope: f32,
    low_pass_min: f32,
    mult_2d: f32,
    mult_3d: f32,
    /// `SS_SPEAKERS_VOLUME_MULTIPLIER`: used unless the opened setup is headphones
    /// ([`speakers`], CONFIRMED `0x01004390`).
    speaker_mult: f32,
    /// `SS_HEADPHONES_VOLUME_MULTIPLIER`: used when it is headphones.
    headphones_mult: f32,
    battle_distance_mult: f32,
    campaign_distance_mult: f32,
    launch_min_distance: f32,
    launch_min_delay: f32,
    speed_of_sound: f32,
}

impl MixSettings {
    /// The shipped values (used when the install has no sound data).
    const SHIPPED: Self = Self {
        rolloff: 1.0,
        cutoff: 1.0,
        low_pass_slope: 0.00001,
        low_pass_min: 0.05,
        mult_2d: 2.0,
        mult_3d: 2.0,
        speaker_mult: 1.0,
        headphones_mult: 0.5,
        battle_distance_mult: 14.0,
        campaign_distance_mult: 20.0,
        launch_min_distance: 100.0,
        launch_min_delay: 0.1,
        speed_of_sound: 340.29,
    };

    fn from_library(lib: &SoundLibrary) -> Self {
        let s = Self::SHIPPED;
        let get = |k: &str, d: f32| lib.setting(k).unwrap_or(d);
        Self {
            rolloff: get("SS_VOLUME_ROLLOFF", s.rolloff),
            cutoff: get("SS_VOLUME_CUTOFF", s.cutoff),
            low_pass_slope: get("SS_LOW_PASS_FILTER", 0.01) * 0.001,
            low_pass_min: get("SS_LOW_PASS_FILTER_MIN", s.low_pass_min),
            mult_2d: get("SS_2D_VOLUME_MULTIPLIER", s.mult_2d),
            mult_3d: get("SS_3D_VOLUME_MULTIPLIER", s.mult_3d),
            speaker_mult: get("SS_SPEAKERS_VOLUME_MULTIPLIER", s.speaker_mult),
            headphones_mult: get("SS_HEADPHONES_VOLUME_MULTIPLIER", s.headphones_mult),
            battle_distance_mult: get("GLOBAL_RECORDED_DISTANCE_MULTIPLIER", s.battle_distance_mult),
            campaign_distance_mult: get("CAMPAIGN_GLOBAL_RECORDED_DISTANCE_MULTIPLIER", s.campaign_distance_mult),
            launch_min_distance: get("MIN_DIST_TO_APPLY_DISTANCE_FROM_LISTENER_TRIGGER_DELAY", s.launch_min_distance),
            launch_min_delay: get("MIN_DISTANCE_FROM_LISTENER_TRIGGER_DELAY", s.launch_min_delay),
            speed_of_sound: get("SPEED_OF_SOUND_IN_METRES_PER_SECOND", s.speed_of_sound),
        }
    }

    /// The manager's speaker multiplier for the opened setup (CONFIRMED `0x01004390`).
    fn speaker_mult(&self, headphones: bool) -> f32 {
        if headphones { self.headphones_mult } else { self.speaker_mult }
    }
}

/// The game speed the sound manager sees (1 = normal; 0 = paused). When it is not 1, sfx (group 1)
/// events do not start and playing sfx fade out over 1 s; while paused the fade clock stops
/// (CONFIRMED `0x01001860`: speed ≠ 1 → group 1 fade-out of 1.0 s; paused or speed ≠ 1 → frame
/// time 0). Set from the battle speed; 1 elsewhere.
#[derive(Resource, Debug, Clone, Copy)]
pub struct GameSpeed(pub f32);

impl Default for GameSpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

/// The sound manager's game mode as far as mixing cares: campaign (game mode 3: campaign
/// distance multiplier, no low-pass) or not.
#[derive(Resource, Debug, Clone, Copy, Default)]
struct AudioMode {
    campaign: bool,
}

/// A playing voice.
#[derive(Component)]
struct Voice {
    control: Arc<VoiceControl>,
    event: usize,
    /// World position (`None` = 2D).
    position: Option<Vec3>,
    min_dist: f32,
    max_dist: f32,
    /// The event's own `falloff` (param 5).
    falloff: f32,
    /// The event's `volume` (param 0).
    volume: f32,
    group: VolumeGroup,
    looped: bool,
    /// Linear fade level 0..=1 and its change per second.
    fade: f32,
    fade_rate: f32,
    /// `fade_type` 1: the level goes through the equal-power map.
    equal_power: bool,
}

/// Decoded clips, shared between voices. Short files only; long ones stream. Music (every
/// Music-group event, looping or not) is decoded whole in the background instead
/// ([`MusicCache`]). Filled once per session:
/// `SoundData` (and its Vfs) is loaded once at start-up. A future hot reload of sounds or mods
/// must replace this resource when it replaces `SoundData`.
#[derive(Resource, Default)]
struct ClipCache {
    clips: HashMap<String, Arc<Pcm>>,
    samples: usize,
    music: MusicCache,
    /// Files (normalized paths, as in [`SoundData::paths`]) that could not be read or decoded:
    /// logged once, not tried again this session (every sound, music or not).
    failed: HashSet<String>,
}

impl ClipCache {
    /// Marks `path` (normalized) failed; logs it the first time. True the first time.
    fn fail(&mut self, path: &str, error: &str) -> bool {
        let first = !self.failed.contains(path) && self.failed.insert(path.to_owned());
        if first {
            warn!("sound file {path}: {error}; not played again this session");
        }
        first
    }

    /// Whether `file` (a normalized path) has not failed.
    fn playable(&self, file: &str) -> bool {
        !self.failed.contains(file)
    }

    /// The first of `files` (an event's normalized paths) that has not failed, if one is left.
    fn first_playable(&self, files: &[Arc<str>]) -> Option<usize> {
        files.iter().position(|f| self.playable(f))
    }

    /// Takes in the finished music loads (every frame), marking the files that failed.
    fn poll_music(&mut self) {
        for (path, error) in self.music.poll() {
            self.fail(&path, &error);
        }
    }
}

/// A music file decoded whole as 16-bit samples, with its frame walk: every event's loop
/// block is a frame range into the same samples ([`loop_region`]).
struct MusicFile {
    pcm: Arc<Pcm16>,
    index: LoopIndex,
    /// When it was last asked for ([`MusicCache::clock`]).
    used: u64,
}

/// What a background music load gives back.
type MusicLoad = Result<(Pcm16, LoopIndex), String>;

/// Music, decoded whole on the async compute pool ([`load_music`]) and kept by file.
/// Main thread only. Files in use (a voice holds their samples) always stay, so the audio thread
/// never drops the last reference; unused ones are evicted least recently used first once the
/// total passes [`MUSIC_BYTES`], and freed on the pool.
#[derive(Default)]
struct MusicCache {
    files: HashMap<String, MusicFile>,
    /// Bytes of samples in `files`.
    bytes: usize,
    /// Use counter for the LRU order: bumped by each hit.
    clock: u64,
    /// Loads in flight, by file (a second request for the same file waits for the same load).
    loads: HashMap<String, Task<MusicLoad>>,
    /// (File, loop block bits) pairs whose block could not be used: logged once each.
    warned_blocks: HashSet<(String, (u32, u32))>,
}

impl MusicCache {
    /// Starts loading `path` unless it is decoded or loading (the caller checks it has not failed).
    fn request(&mut self, vfs: &Arc<Vfs>, path: &str) {
        if !self.files.contains_key(path) && !self.loads.contains_key(path) {
            let (vfs, p) = (vfs.clone(), path.to_owned());
            self.loads.insert(path.to_owned(), AsyncComputeTaskPool::get().spawn(async move { load_music(&vfs, &p) }));
        }
    }

    /// Takes in the finished loads (every frame); evicted files are freed on the pool. Returns
    /// the files that failed, with why (empty, without allocating, when none did).
    fn poll(&mut self) -> Vec<(String, String)> {
        let mut failed = Vec::new();
        let mut done = Vec::new();
        self.loads.retain(|path, task| match check_ready(task) {
            Some(r) => {
                done.push((path.clone(), r));
                false
            }
            None => true,
        });
        for (path, r) in done {
            match r {
                Ok((pcm, index)) => {
                    let freed = self.insert(path, pcm, index, MUSIC_BYTES);
                    if !freed.is_empty() {
                        AsyncComputeTaskPool::get().spawn(async move { drop(freed) }).detach();
                    }
                }
                Err(e) => failed.push((path, e)),
            }
        }
        failed
    }

    /// Adds a decoded file, then evicts unused files, least recently used first, until the total
    /// is within `budget` (or only files in use are left). Returns the evicted ones.
    fn insert(&mut self, path: String, pcm: Pcm16, index: LoopIndex, budget: usize) -> Vec<MusicFile> {
        self.clock += 1;
        self.bytes += size_of_pcm(&pcm);
        let mut freed: Vec<MusicFile> = self.files.insert(path, MusicFile { pcm: Arc::new(pcm), index, used: self.clock }).into_iter().collect();
        for old in &freed {
            self.bytes -= size_of_pcm(&old.pcm);
        }
        while self.bytes > budget {
            let oldest = self.files.iter().filter(|(_, f)| Arc::strong_count(&f.pcm) == 1).min_by_key(|(_, f)| f.used).map(|(k, _)| k.clone());
            let Some(f) = oldest.and_then(|k| self.files.remove(&k)) else { break };
            self.bytes -= size_of_pcm(&f.pcm);
            freed.push(f);
        }
        freed
    }

    /// A decoded file's samples and the loop region `block` maps to (`None`: not decoded). A block
    /// that cannot be used is logged once per (file, block); the whole file loops.
    fn clip(&mut self, path: &str, block: (f32, f32)) -> Option<(ClipData, (u64, u64))> {
        let f = self.files.get_mut(path)?;
        self.clock += 1;
        f.used = self.clock;
        let region = loop_region(&f.index, f.pcm.frames(), block).unwrap_or_else(|why| {
            if self.warned_blocks.insert((path.to_owned(), (block.0.to_bits(), block.1.to_bits()))) {
                warn!("music file {path}: {why}; the whole file loops");
            }
            (0, 0)
        });
        Some((ClipData::Decoded(Samples::I16(f.pcm.clone())), region))
    }
}

fn size_of_pcm(p: &Pcm16) -> usize {
    p.samples.len() * std::mem::size_of::<i16>()
}

/// Reads and decodes a music file whole, on the async compute pool, like the exe, which
/// reads every sound whole before Miles plays it. The frame walk sizes the buffer exactly.
fn load_music(vfs: &Vfs, path: &str) -> MusicLoad {
    decode_music(vfs.read(path).map_err(|e| e.to_string())?, path)
}

/// Walks and decodes a music file. A file that decodes to no audio fails: a voice of it
/// would end at once and the music would pick it again every frame.
fn decode_music(bytes: Vec<u8>, path: &str) -> MusicLoad {
    let index = LoopIndex::new(&bytes);
    let pcm = decode_timed_i16(bytes, path.rsplit('.').next(), index.decoded_frames()).map_err(|e| e.to_string())?;
    if pcm.frames() == 0 {
        return Err("decodes to no audio".into());
    }
    Ok((pcm, index))
}

/// The loop region, in sample frames of a file decoded to `frames` frames, for an event's loop
/// block (`loop_start_block`, `loop_end_block`): byte offsets into the file (Miles' loop block,
/// set by `0x01004430`; CONFIRMED units, AUDIO_FORMAT.md §8). `(0, 0)` = the whole file loops (no
/// block, or the exe sets none). `Err`: a block the exe would set that cannot be used (it cannot
/// be mapped, or its start has no decoded audio after it, e.g. damaged frames at the end); the
/// whole file loops then.
fn loop_region(index: &LoopIndex, frames: usize, (a, b): (f32, f32)) -> Result<(u64, u64), String> {
    match index.region(a, b) {
        Ok(None) => Ok((0, 0)),
        Ok(Some((s, _))) if s > 0 && s >= frames as u64 => Err(format!("loop start frame {s} is at or past the end of the decoded audio ({frames} frames)")),
        Ok(Some(r)) => Ok(r),
        Err(e) => Err(format!("loop block {a}..{b} cannot be mapped ({e})")),
    }
}

/// Bookkeeping for repeat limits and `random_cycle`.
#[derive(Resource, Default)]
struct EventHistory {
    last_played: HashMap<usize, f64>,
    last_file: HashMap<usize, usize>,
}
/// The current music: the state machine's state ([`music_step`]) and what the Bevy side holds for
/// it.
#[derive(Resource, Default)]
struct Music {
    state: MusicState,
    /// The prepared voice of `state.waiting` (dropped whenever `state.waiting` is `None`).
    pending: Option<Prepared>,
    /// The asked-for music state's name (for logs).
    name: String,
    /// States already logged (unknown, or no playable track).
    warned_states: HashSet<String>,
    /// One input chained more than [`MUSIC_MAX_STEPS`] steps (a bug): logged once, and ticks do
    /// nothing until the next other input.
    stalled: bool,
}

/// The music state machine's state. Pure data: [`music_step`] is the only thing that changes it.
#[derive(Debug, Clone, PartialEq, Default)]
struct MusicState {
    /// The asked-for state's candidate events (empty: no music).
    cands: Vec<usize>,
    /// The playing track: its voice and event.
    playing: Option<(Entity, usize)>,
    /// The track waiting for its file's decode ([`MusicCache`]): its event, and when the change
    /// that asked for it was made (real seconds).
    waiting: Option<(usize, f64)>,
    /// The event started last (a new pick avoids it when there is another choice).
    last: Option<usize>,
}

/// What happened, for [`music_step`].
#[derive(Debug, Clone, PartialEq)]
enum MusicInput {
    /// The music state changed; its candidate events (empty: stop, unknown state, no sound data).
    SetState(Vec<usize>),
    /// This voice ended (a non-looping track) or was stopped from elsewhere.
    Ended(Entity),
    /// Once per frame, after the other inputs.
    Tick,
    /// Reply to [`MusicAction::Begin`] / [`MusicAction::Retry`]: this event's voice started.
    Started { voice: Entity, event: usize },
    /// Reply: its file is still decoding.
    Loading,
    /// Reply: its file cannot be played (marked failed, so it is not picked again).
    Failed,
    /// Reply: the start rules refused it (probability, repeat limit, at-once limit; the last two
    /// are checked again when a decoded track starts; never for vanilla music, see the install
    /// test).
    Rejected,
}

/// What the Bevy side does for [`music_step`].
#[derive(Debug, Clone, Copy, PartialEq)]
enum MusicAction {
    /// Fade this voice out with its event's own `fade_out` (CONFIRMED `0x010089D0`).
    FadeOut(Entity, usize),
    /// Run the start rules for this event and start it, or its decode; reply with
    /// Started / Loading / Failed / Rejected.
    Begin(usize),
    /// Try the waiting track again; reply with Started / Loading / Failed / Rejected.
    Retry,
    /// No candidate of the state has a playable file: log it once for the state.
    NothingPlayable,
}

/// What [`music_step`] may look at besides its state and input.
struct MusicEnv<'a> {
    /// Real seconds.
    now: f64,
    /// Whether an event has a file left that can be played (not marked failed).
    playable: &'a dyn Fn(usize) -> bool,
    rng: &'a mut AudioRng,
}

/// How long a music change waits for the new track's decode before the old track fades out
/// anyway, so one state's music does not carry over into the next (front end into battle). The
/// new track still starts when its decode is done. PROVISIONAL: the exe does not wait (Miles
/// decodes while playing); our slowest music decode is about 0.23 s in release (AUDIO_FORMAT.md §8).
const MUSIC_SWITCH_TIMEOUT: f64 = 1.0;

/// The music state machine: one step from `s` on `input`. The playing track keeps playing until
/// the new one starts or [`MUSIC_SWITCH_TIMEOUT`] runs out, then fades out with its own
/// `fade_out` while the new one fades in with its own `fade_in` (CONFIRMED `0x010089D0` /
/// `0x010086A0`). Pure: the Bevy system ([`music`]) applies the actions and feeds back the
/// replies.
fn music_step(mut s: MusicState, input: MusicInput, env: &mut MusicEnv) -> (MusicState, Vec<MusicAction>) {
    let mut out = Vec::new();
    match input {
        MusicInput::SetState(cands) => {
            s.cands = cands;
            if s.playing.is_some_and(|(_, e)| s.cands.contains(&e)) {
                // The playing track is still a valid choice: it keeps playing.
                s.waiting = None;
            } else if !s.waiting.is_some_and(|(e, _)| s.cands.contains(&e) && (env.playable)(e)) {
                // The waiting track (if any) is not: pick before anything starts, so the old
                // state's track never starts in the new state.
                s.waiting = None;
                pick(&mut s, &mut out, env, env.now);
            }
        }
        MusicInput::Ended(voice) => {
            if s.playing.is_some_and(|(v, _)| v == voice) {
                s.playing = None;
                if s.waiting.is_none() {
                    pick(&mut s, &mut out, env, env.now);
                }
            }
        }
        MusicInput::Tick => {
            if s.waiting.is_some() {
                out.push(MusicAction::Retry);
            }
        }
        MusicInput::Started { voice, event } => match s.waiting {
            Some((e, _)) if e == event => {
                s.waiting = None;
                fade(&mut s, &mut out);
                s.playing = Some((voice, event));
                s.last = Some(event);
            }
            // Nobody waits for it (cannot happen through `music`): it does not play.
            _ => out.push(MusicAction::FadeOut(voice, event)),
        },
        MusicInput::Loading => {
            if s.waiting.is_some_and(|(_, since)| env.now - since >= MUSIC_SWITCH_TIMEOUT) {
                fade(&mut s, &mut out);
            }
        }
        MusicInput::Failed => {
            if let Some((_, since)) = s.waiting.take() {
                // Another track among the files left; the old track's hold still counts from the
                // change.
                pick(&mut s, &mut out, env, since);
            }
        }
        MusicInput::Rejected => {
            // The state's track does not play, so the old one does not carry over either.
            // PROVISIONAL: silence until the next change (vanilla music never hits this).
            if s.waiting.take().is_some() {
                fade(&mut s, &mut out);
            }
        }
    }
    (s, out)
}

/// Picks a playable candidate (avoiding the last track when there is another) as the waiting
/// track; with none, the music fades out.
fn pick(s: &mut MusicState, out: &mut Vec<MusicAction>, env: &mut MusicEnv, since: f64) {
    let ok: Vec<usize> = s.cands.iter().copied().filter(|&e| (env.playable)(e)).collect();
    if ok.is_empty() {
        if !s.cands.is_empty() {
            out.push(MusicAction::NothingPlayable);
        }
        fade(s, out);
        return;
    }
    let mut i = env.rng.below(ok.len());
    if ok.len() > 1 && Some(ok[i]) == s.last {
        i = (i + 1) % ok.len();
    }
    s.waiting = Some((ok[i], since));
    out.push(MusicAction::Begin(ok[i]));
}

/// Fades the playing track out.
fn fade(s: &mut MusicState, out: &mut Vec<MusicAction>) {
    if let Some((v, e)) = s.playing.take() {
        out.push(MusicAction::FadeOut(v, e));
    }
}

/// A play waiting for its file's decode (music asked for outside the music state machine).
struct WaitingPlay {
    play: Prepared,
    /// A campaign ambience emitter ([`CampaignEmitterVoice`]).
    emitter: bool,
    /// When it was asked for (real seconds).
    since: f64,
}

/// Plays waiting for their file's decode, started by [`poll_loads`] once it is done. At most one
/// per request (event, emitter or not, position): the same request asked for again while it
/// waits is dropped, so N requests during the decode start one voice (PROVISIONAL: the exe has no
/// wait, Miles plays at once, so it could start them all there); two emitters of one event, or a
/// `PlaySound` and an emitter, are different requests. The start rules that depend on what plays
/// (repeat limit, at-once limit, voice limit: [`Player::admit`]) are checked again when it starts.
/// Dropped when its file fails, when it is still decoding after [`WAITING_PLAY_TIMEOUT`], and on
/// every game-mode change, so a front-end request never starts in a battle.
#[derive(Resource, Default)]
struct WaitingPlays(Vec<WaitingPlay>);

/// How long a play waits for its decode before it is dropped (a decode that has finished by
/// then still starts). PROVISIONAL (the exe does not wait). The slowest music walk + decode is
/// 230 ms in release and 161 ms in a debug build (dependencies are optimized; AUDIO_FORMAT.md §8),
/// so 10 s leaves room for a pool busy with loading.
const WAITING_PLAY_TIMEOUT: f64 = 10.0;

/// The voices [`Player::start`] spawned this frame. The voice query shows a voice only once its
/// deferred spawn is applied, which happens between the chained audio systems, so the tally only
/// matters within one system: every start rule ([`Player::live`]) counts these too, for normal
/// and waiting plays alike (two plays of one event in one system cannot both pass
/// `max_number_playing_at_once` = 1). An entry the query already shows is not counted twice.
/// Emptied at the start of each frame ([`poll_loads`]).
#[derive(Resource, Default)]
struct NewVoices(Vec<NewVoice>);

/// What [`Player::live`] reads of a tallied voice: a copy of its [`Voice`]'s event, position and
/// control, as the `Voice` itself moves into the deferred spawn.
struct NewVoice {
    entity: Entity,
    event: usize,
    position: Option<Vec3>,
    control: Arc<VoiceControl>,
}

/// Drops every waiting play (run on each game-mode change, in `Update` before the systems that
/// read play requests). The new mode's `OnEnter` systems run earlier, in `StateTransition`, but
/// ask for sounds only through messages ([`SetMusic`], [`PlaySound`], [`CampaignAmbience`]...),
/// which those later systems read, so nothing they ask for is dropped.
fn drop_waiting_plays(mut waiting: ResMut<WaitingPlays>) {
    waiting.0.clear();
}

/// Asks [`update_voices`] to fade a voice out over this many seconds, then stop it.
#[derive(Component)]
struct FadeOut(f32);

/// Running campaign emitters.
#[derive(Component)]
struct CampaignEmitterVoice;

/// A small display-only random generator (sound variation is not part of the simulation).
#[derive(Resource)]
struct AudioRng(u64);

impl AudioRng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.next_f32() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

/// Files shorter than this are decoded fully and cached; longer ones stream.
const STREAM_BYTES: usize = 512 * 1024;
/// Cache limit in samples (~256 MB of f32).
const CACHE_SAMPLES: usize = 64 * 1024 * 1024;
/// Budget of the decoded music ([`MusicCache`]): unused files past it are evicted. Not a
/// count cap (any number of files in use stay).
const MUSIC_BYTES: usize = 256 * 1024 * 1024;
/// Total voices at once (the preference `sound_channels` default is 128).
const MAX_VOICES: usize = 128;

// ---------------------------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------------------------

/// Registers the sound system.
pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<VoiceClip>()
            .add_message::<PlaySound>()
            .add_message::<UiSound>()
            .add_message::<ProjectileFired>()
            .add_message::<SetMusic>()
            .add_message::<CampaignAmbience>()
            .add_message::<PlayMovieAudio>()
            .init_resource::<ClipCache>()
            .init_resource::<EventHistory>()
            .init_resource::<Music>()
            .init_resource::<WaitingPlays>()
            .init_resource::<NewVoices>()
            .init_resource::<CampaignToWorld>()
            .init_resource::<AudioMode>()
            .init_resource::<GameSpeed>()
            .insert_resource(MixSettings::SHIPPED)
            .insert_resource(AudioRng(0x9E37_79B9_7F4A_7C15 ^ std::process::id() as u64))
            .insert_resource(Volumes::load())
            .add_systems(PreStartup, load_sound_data)
            .add_systems(OnEnter(GameMode::FrontEnd), |mut m: MessageWriter<SetMusic>, mut mode: ResMut<AudioMode>, mut speed: ResMut<GameSpeed>| {
                mode.campaign = false;
                speed.0 = 1.0;
                m.write(SetMusic { state: Some("music_front_end".into()), subculture: None });
            })
            .add_systems(OnEnter(GameMode::Campaign), enter_campaign)
            .add_systems(OnEnter(GameMode::Battle), enter_battle)
            .add_systems(
                Update,
                (
                    watch_preferences,
                    drop_waiting_plays.run_if(state_changed::<GameMode>),
                    poll_loads,
                    battle::bridge_volleys.run_if(in_state(GameMode::Battle)),
                    battle::track_speed.run_if(in_state(GameMode::Battle)),
                    ui_sounds,
                    projectile_sounds,
                    play_sounds,
                    movie_audio,
                    campaign_ambience,
                    music,
                    update_voices,
                )
                    .chain(),
            );
    }
}

fn load_sound_data(mut commands: Commands, mut volumes: ResMut<Volumes>) {
    let dir = config::game_data_dir();
    let vfs = match Vfs::open_install(&dir) {
        Ok(v) => v,
        Err(e) => {
            warn!("No sound: the install at {} could not be opened: {e}", dir.display());
            return;
        }
    };
    match SoundLibrary::load(&vfs) {
        Ok(lib) => {
            info!(
                "Sound: {} events, {} banks, {} named events",
                lib.events.events.len(),
                lib.banks.banks.len(),
                lib.names.named_count()
            );
            commands.insert_resource(MixSettings::from_library(&lib));
            volumes.set_movie(lib.setting("MOVIE_VOLUME").unwrap_or(1.0));
            commands.insert_resource(SoundData::new(lib, vfs));
        }
        Err(e) => warn!("No sound: {e}"),
    }
}

fn enter_campaign(mut speed: ResMut<GameSpeed>, mut mode: ResMut<AudioMode>, mut music: MessageWriter<SetMusic>) {
    speed.0 = 1.0;
    mode.campaign = true;
    // PROVISIONAL: the campaign view should send SetMusic with the player's subculture; until
    // then any campaign track plays.
    music.write(SetMusic { state: Some("music_campaign".into()), subculture: None });
}

fn enter_battle(mut mode: ResMut<AudioMode>) {
    mode.campaign = false;
    // The battle sends SetMusic itself for each phase (deployment, battle, results) with the
    // player's subculture (`battle::hud`). PROVISIONAL: the original starts deployment music after
    // LAND_BATTLE_TIME_UNTIL_DEPLOYMENT_MUSIC_PLAYS (40 s) and switches to battle music when
    // PERCENTAGE_OF_LAND_UNITS_FIGHTING_FOR_BATTLE_MUSIC_TO_PLAY (10 %) of units fight; we switch
    // at Start Battle.
}

/// Re-reads the volumes when our preferences file changes (the Options page saves it).
fn watch_preferences(time: Res<Time>, mut vol: ResMut<Volumes>, data: Option<Res<SoundData>>) {
    vol.check_timer -= time.delta_secs();
    if vol.check_timer > 0.0 {
        return;
    }
    vol.check_timer = 1.0;
    let mtime = vol.prefs_path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
    if mtime.is_some() && mtime != vol.prefs_mtime {
        *vol = Volumes::load();
        vol.set_movie(data.and_then(|d| d.lib.setting("MOVIE_VOLUME")).unwrap_or(1.0));
    }
}

// ---------------------------------------------------------------------------------------------
// Playing events
// ---------------------------------------------------------------------------------------------

/// Everything a system needs to start voices.
#[derive(bevy::ecs::system::SystemParam)]
struct Player<'w, 's> {
    commands: Commands<'w, 's>,
    data: Option<Res<'w, SoundData>>,
    clips: ResMut<'w, Assets<VoiceClip>>,
    cache: ResMut<'w, ClipCache>,
    waiting: ResMut<'w, WaitingPlays>,
    new_voices: ResMut<'w, NewVoices>,
    history: ResMut<'w, EventHistory>,
    rng: ResMut<'w, AudioRng>,
    time: Res<'w, Time<Real>>,
    settings: Res<'w, MixSettings>,
    mode: Res<'w, AudioMode>,
    speed: Res<'w, GameSpeed>,
    listener: ListenerQuery<'w, 's>,
    voices: Query<'w, 's, (Entity, &'static Voice)>,
}

impl Player<'_, '_> {
    /// Starts event `event` (index) with the sound manager's start rules
    /// (MIDDLEWARE_VERIFY.md §1.4–1.6). Returns the voice entity if it started now.
    fn play(&mut self, event: usize, position: Option<Vec3>, extra_delay: f32) -> Option<Entity> {
        self.play_as(event, position, extra_delay, false)
    }

    /// [`Player::play`]; `emitter` marks the voice as a campaign ambience emitter. Music not
    /// decoded yet starts its background load and waits for it ([`WaitingPlays`], started by
    /// [`poll_loads`]).
    fn play_as(&mut self, event: usize, position: Option<Vec3>, extra_delay: f32, emitter: bool) -> Option<Entity> {
        let p = self.prepare(event, position, extra_delay)?;
        match self.begin(p, Ask::New) {
            Begin::Started(e) => {
                if emitter {
                    self.commands.entity(e).insert(CampaignEmitterVoice);
                }
                Some(e)
            }
            Begin::Wait(play) => {
                // Exact position equality: a request asked for again computes its position the
                // same way, bit for bit; a tolerance would merge two nearby emitters, which must
                // both start.
                let same = |w: &WaitingPlay| w.play.event == play.event && w.emitter == emitter && w.play.voice.position == play.voice.position;
                if !self.waiting.0.iter().any(same) {
                    let since = self.time.elapsed_secs_f64();
                    self.waiting.0.push(WaitingPlay { play, emitter, since });
                }
                None
            }
            Begin::Failed | Begin::Rejected => None,
        }
    }

    /// Starts the waiting plays whose decode is done (also one that finished on the timeout
    /// frame), if the start rules let them then; drops those whose file failed (logged where it
    /// failed), those the rules refuse, and those still decoding after [`WAITING_PLAY_TIMEOUT`]
    /// (without asking for a new decode).
    fn start_waiting(&mut self) {
        if self.waiting.0.is_empty() {
            return;
        }
        let now = self.time.elapsed_secs_f64();
        for WaitingPlay { play, emitter, since } in std::mem::take(&mut self.waiting.0) {
            let timed_out = now - since >= WAITING_PLAY_TIMEOUT;
            match self.begin(play, if timed_out { Ask::TimedOut } else { Ask::Waited }) {
                Begin::Started(e) => {
                    if emitter {
                        self.commands.entity(e).insert(CampaignEmitterVoice);
                    }
                }
                Begin::Wait(play) if timed_out => {
                    warn!("sound: event {} dropped, its file {} still decoding after {WAITING_PLAY_TIMEOUT} s", play.event, play.path);
                }
                Begin::Wait(play) => self.waiting.0.push(WaitingPlay { play, emitter, since }),
                Begin::Failed | Begin::Rejected => {}
            }
        }
    }

    /// The start rules: whether event `event` plays now (game speed, files left, distance, repeat
    /// limit, probability, at-once and voice limits), which file, and with what gains, pitch and
    /// delay. Changes nothing but the RNG; the voice it replaces under
    /// `max_number_playing_at_once` is stopped, and the play recorded, only when it starts
    /// ([`Player::start`]).
    fn prepare(&mut self, event: usize, position: Option<Vec3>, extra_delay: f32) -> Option<Prepared> {
        let data = self.data.as_deref()?;
        let ev = data.lib.events.events.get(event)?;
        let Some(paths) = data.paths.get(event) else {
            // Cannot happen: `SoundData::new` builds one path list per event, and neither changes
            // after. Logged once per build (prepare runs on the main thread, never the audio thread).
            if !data.missing_paths_logged.swap(true, std::sync::atomic::Ordering::Relaxed) {
                error!("sound: event {event} has no file paths (sound data out of step)");
            }
            return None;
        };
        if paths.is_empty() {
            return None;
        }
        let p = data.lib.events.params_of(ev);
        let s = *self.settings;
        let group = VolumeGroup::from_param(p.get(Param::Group));
        if group == VolumeGroup::Sfx && self.speed.0 != 1.0 {
            return None;
        }
        let looped = p.flag(Param::Looped);
        // Music (looping or not) is decoded whole in the background before it plays
        // ([`MusicCache`]), so a file that cannot be read or decodes to no audio is marked failed
        // before any voice of it starts. A looping one plays its loop block:
        // `loop_start/end_block` are byte offsets into the file (Miles' loop block, set by
        // `0x01004430`), mapped to sample frames there (CONFIRMED units, AUDIO_FORMAT.md §8).
        // Every other sound plays from the clip cache or a stream. PROVISIONAL: other looped
        // events with a loop block (no shipped one has a usable block, see the install test)
        // loop the whole file.
        let music_block = (group == VolumeGroup::Music).then(|| if looped { (p.get(Param::LoopStartBlock), p.get(Param::LoopEndBlock)) } else { (0.0, 0.0) });
        // Files that failed before are skipped; with none left nothing plays.
        let first_playable = self.cache.first_playable(paths)?;
        let n = paths.len();
        let is_2d = position.is_none() || p.flag(Param::Is2d);
        let position = if is_2d { None } else { position };
        let ear = listener_transform(&self.listener);
        let d = ear_distance(ear, position);
        // Distances (§1.5) and the past-max rule: a 3D voice that starts past max is not started.
        let mult = if self.mode.campaign { s.campaign_distance_mult } else { s.battle_distance_mult };
        let (min_dist, max_dist) = mixer::event_distances(p.get(Param::MinDist), p.get(Param::MaxDist), mult, s.cutoff, s.rolloff);
        if !is_2d && mixer::past_max(d, max_dist) {
            return None;
        }
        // The rules run in this order, each before anything is drawn or allocated for the voice:
        // repeat limit, probability, at-once and voice limits (a play that waits for its decode
        // meets the first and last again when it starts: [`Player::admit`]).
        let limits = Limits::of(p);
        if !self.may_repeat(event, &limits) {
            return None;
        }
        // One pass over what plays serves the probability and the at-once and voice limits; the
        // latter refuse only after the probability draw.
        let tally = self.tally(event, &limits, ear);
        let room = Self::room_for(&tally, &limits, d, looped);
        // Probability minus the reductions per playing instance (§1.6).
        let prob = p.get(Param::Probability)
            - p.get(Param::ProbabilityReductionSameEvents) * tally.same as f32
            - p.get(Param::ProbabilityReductionAnyEvents) * tally.any as f32;
        if prob <= 0.0 || (prob < 1.0 && self.rng.next_f32() > prob) {
            return None;
        }
        let steal = room?;
        // Which file: `random_cycle` (1) avoids repeating the last one; `random` (0) any; a file
        // that failed before is skipped for the first one left (found above). A silent
        // placeholder plays like any other file: the exe has no name rule for it (CONFIRMED: no
        // string of Napoleon.exe names a placeholder; its only `silent`, `0x013EEAF4`, is a
        // category name of the text event reader `0x01011EA0`).
        let mut file = self.rng.below(n);
        if p.get(Param::Playback) == 1.0 && n > 1 && self.history.last_file.get(&event) == Some(&file) {
            file = (file + 1) % n;
        }
        if !self.cache.playable(&paths[file]) {
            file = first_playable;
        }
        let path = paths[file].clone();

        // Pitch: semitones, uniform in [min, max] (CONFIRMED unit).
        let (lo, hi) = (p.get(Param::MinPitch), p.get(Param::MaxPitch));
        let semis = lo + (hi - lo) * self.rng.next_f32();
        let speed = 2f32.powf(semis / 12.0);
        // Launch delay (§1.6): speed of sound for 3D events that ask for it; otherwise the random
        // trigger delay in 0.01 s steps; then the start delay.
        let mut delay = if is_2d {
            0.0
        } else {
            mixer::launch_delay(d, p.flag(Param::ApplyLaunchDelayRelativeToDistance), s.launch_min_distance, s.speed_of_sound, s.launch_min_delay)
        };
        if delay < 0.01 {
            delay = mixer::random_trigger_delay(p.get(Param::RandomTriggerDelay), self.rng.next_f32());
        }
        let delay = extra_delay + delay + p.get(Param::StartDelay).max(0.0);
        let fade_in = p.get(Param::FadeIn);
        let voice = Voice {
            control: VoiceControl::new(0.0, 0.0),
            event,
            position,
            min_dist,
            max_dist,
            falloff: p.get(Param::Falloff),
            volume: p.get(Param::Volume),
            group,
            looped,
            fade: if fade_in > 0.0 { 0.0 } else { 1.0 },
            fade_rate: if fade_in > 0.0 { 1.0 / fade_in } else { 0.0 },
            equal_power: p.get(Param::FadeType) == 1.0,
        };
        Some(Prepared { event, limits, file, path, voice, speed, delay, music_block, prefer_stream: p.flag(Param::Streamed) || looped, steal })
    }

    /// The repeat limit (our own bookkeeping of `delay_before_can_play_again`): whether `event`
    /// may play again now.
    fn may_repeat(&self, event: usize, l: &Limits) -> bool {
        let again = f64::from(l.again);
        let now = self.time.elapsed_secs_f64();
        !(again > 0.0 && self.history.last_played.get(&event).is_some_and(|t| now - t < again))
    }

    /// The at-once and voice limits for a new voice of an event (limits `l`) at distance `d`,
    /// given what plays (`t`, [`Player::tally`]): `None` when it may not start, else the voice it
    /// replaces. At most `max_number_playing_at_once` (1000 = unlimited): the lowest-priority
    /// instance is replaced when the new one has a higher [`Limits::priority`] (§1.6). Then at most
    /// [`MAX_VOICES`] voices, looped ones exempt. Allocates nothing.
    fn room_for(t: &Tally, l: &Limits, d: f32, looped: bool) -> Option<Option<Arc<VoiceControl>>> {
        let mut steal = None;
        if l.capped() && t.same as f32 >= l.max_at_once {
            match t.lowest {
                Some((c, pr)) if pr < l.priority(d) => steal = Some(c.clone()),
                _ => return None,
            }
        }
        (looped || t.any < MAX_VOICES).then_some(steal)
    }

    /// One pass over [`Player::live`] for the start rules of a new voice of `event` (limits `l`):
    /// how many voices of it and in all play, and, under an at-once limit, its lowest-priority
    /// instance (the first of equals).
    fn tally(&self, event: usize, l: &Limits, ear: Option<GlobalTransform>) -> Tally<'_> {
        let mut t = Tally { same: 0, any: 0, lowest: None };
        for (e, pos, c) in self.live() {
            t.any += 1;
            if e == event {
                t.same += 1;
                if !l.capped() {
                    continue;
                }
                let pr = l.priority(ear_distance(ear, pos));
                if t.lowest.is_none_or(|(_, low)| pr.total_cmp(&low).is_lt()) {
                    t.lowest = Some((c, pr));
                }
            }
        }
        t
    }

    /// The voices playing now, for every start rule: those in the voice query plus those started
    /// since the last command flush ([`NewVoices`]; their spawn is deferred, so the query does not
    /// show them yet), without the ones already stopped (a replaced instance). As (event,
    /// position, control).
    fn live(&self) -> impl Iterator<Item = (usize, Option<Vec3>, &Arc<VoiceControl>)> + '_ {
        let unseen = self.new_voices.0.iter().filter(|n| self.voices.get(n.entity).is_err()).map(|n| (n.event, n.position, &n.control));
        self.voices.iter().map(|(_, v)| (v.event, v.position, &v.control)).chain(unseen).filter(|v| !v.2.is_stopped())
    }

    /// For a play that waited for its decode: the repeat, at-once and voice limits again, as they
    /// stand when it starts (no draw: probability and file were decided when it was asked for).
    /// Sets `p.steal`; stops nothing.
    fn admit(&self, p: &mut Prepared) -> bool {
        if !self.may_repeat(p.event, &p.limits) {
            return false;
        }
        let ear = listener_transform(&self.listener);
        let d = ear_distance(ear, p.voice.position);
        match Self::room_for(&self.tally(p.event, &p.limits, ear), &p.limits, d, p.voice.looped) {
            Some(steal) => {
                p.steal = steal;
                true
            }
            None => false,
        }
    }

    /// Starts a prepared voice if its samples are at hand. Music not decoded yet starts its
    /// background load (unless `ask` is [`Ask::TimedOut`]) and comes back as [`Begin::Wait`]; a
    /// play that waited for its decode runs [`Player::admit`] first. A file that failed before
    /// (or fails now) comes back as [`Begin::Failed`].
    fn begin(&mut self, mut p: Prepared, ask: Ask) -> Begin {
        let Some(vfs) = self.data.as_deref().map(|d| d.vfs.clone()) else { return Begin::Failed };
        if self.cache.failed.contains(&*p.path) {
            return Begin::Failed;
        }
        let Some(block) = p.music_block else {
            return match self.clip_data(&vfs, &p.path, p.prefer_stream) {
                Some(data) => Begin::Started(self.start(p, data, (0, 0))),
                None => Begin::Failed,
            };
        };
        if let Some((data, region)) = self.cache.music.clip(&p.path, block) {
            if ask != Ask::New && !self.admit(&mut p) {
                return Begin::Rejected;
            }
            return Begin::Started(self.start(p, data, region));
        }
        if ask != Ask::TimedOut {
            self.cache.music.request(&vfs, &p.path);
        }
        Begin::Wait(p)
    }

    /// Spawns the voice, stops the voice it steals, and records the play (repeat limit,
    /// `random_cycle`).
    fn start(&mut self, mut p: Prepared, data: ClipData, (loop_start, loop_end): (u64, u64)) -> Entity {
        if let Some(d) = self.data.as_deref() {
            debug!("sound: event {} ({}) {}", p.event, d.lib.names.name(p.event).unwrap_or("?"), p.path);
        }
        if let Some(c) = p.steal.take() {
            c.stop();
        }
        self.history.last_file.insert(p.event, p.file);
        self.history.last_played.insert(p.event, self.time.elapsed_secs_f64());
        let clip = VoiceClip { data, control: p.voice.control.clone(), speed: p.speed, delay: p.delay, looped: p.voice.looped, loop_start, loop_end };
        let (event, position, control) = (p.event, p.voice.position, p.voice.control.clone());
        let entity = self.commands.spawn((AudioPlayer::<VoiceClip>(self.clips.add(clip)), PlaybackSettings::ONCE, p.voice)).id();
        self.new_voices.0.push(NewVoice { entity, event, position, control });
        entity
    }

    /// The event's own fade-out time in seconds (param 15), for stops "with the event's fade".
    fn event_fade_out(&self, event: usize) -> f32 {
        self.data
            .as_deref()
            .and_then(|d| d.lib.events.events.get(event).map(|ev| d.lib.events.params_of(ev).get(Param::FadeOut)))
            .unwrap_or(0.0)
            .max(0.0)
    }

    /// The samples of a file: cached decoded PCM for short files, a stream for long ones.
    fn clip_data(&mut self, vfs: &Vfs, path: &str, prefer_stream: bool) -> Option<ClipData> {
        if let Some(p) = self.cache.clips.get(path) {
            return Some(ClipData::Decoded(Samples::F32(p.clone())));
        }
        let bytes = match vfs.read(path) {
            Ok(b) => b,
            Err(e) => {
                self.cache.fail(path, &e.to_string());
                return None;
            }
        };
        let ext = path.rsplit('.').next().map(str::to_owned);
        if prefer_stream || bytes.len() > STREAM_BYTES {
            return Some(ClipData::Streamed { bytes: SharedBytes(bytes.into()), ext });
        }
        match decode(bytes, ext.as_deref()) {
            Ok(pcm) => {
                if self.cache.samples + pcm.samples.len() > CACHE_SAMPLES {
                    self.cache.clips.clear();
                    self.cache.samples = 0;
                }
                self.cache.samples += pcm.samples.len();
                let pcm = Arc::new(pcm);
                self.cache.clips.insert(path.to_owned(), pcm.clone());
                Some(ClipData::Decoded(Samples::F32(pcm)))
            }
            Err(e) => {
                self.cache.fail(path, &e.to_string());
                None
            }
        }
    }
}

/// A voice the start rules let through ([`Player::prepare`]), waiting for its samples.
struct Prepared {
    event: usize,
    /// The limits [`Player::admit`] checks again.
    limits: Limits,
    /// Which of the event's files, and its normalized path ([`SoundData::paths`]).
    file: usize,
    path: Arc<str>,
    voice: Voice,
    speed: f32,
    delay: f32,
    /// Music: decoded whole in the background ([`MusicCache`]), with this loop block
    /// (`loop_start_block`, `loop_end_block`; `(0, 0)` when not looped).
    music_block: Option<(f32, f32)>,
    /// Stream rather than decode whole (the `streamed` flag, or looped).
    prefer_stream: bool,
    /// The voice it replaces under `max_number_playing_at_once` (chosen by [`Player::room_for`],
    /// again by [`Player::admit`] for a play that waited), stopped when it starts.
    steal: Option<Arc<VoiceControl>>,
}

/// Who asks [`Player::begin`] to start a prepared voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ask {
    /// A new play.
    New,
    /// A play that waited for its decode: [`Player::admit`] runs first.
    Waited,
    /// A waiting play past [`WAITING_PLAY_TIMEOUT`]: as `Waited`, but it asks for no new decode
    /// (it is dropped when its file is not decoded).
    TimedOut,
}

/// What plays, as the start rules of a new voice of one event see it ([`Player::tally`]).
struct Tally<'a> {
    /// Voices of the event.
    same: usize,
    /// Voices of any event.
    any: usize,
    /// The event's lowest-priority voice, with its priority; `None` when the event has no at-once limit
    /// ([`Limits::capped`] false: nothing is ever stolen for it) or no voice of it plays.
    lowest: Option<(&'a Arc<VoiceControl>, f32)>,
}

/// An event's parameters for the start rules that depend on what plays (repeat, at-once and
/// voice limits), which a play that waited for its decode meets again ([`Player::admit`]).
#[derive(Debug, Clone, Copy)]
struct Limits {
    /// `delay_before_can_play_again`, seconds.
    again: f32,
    /// `max_number_playing_at_once` (1000 = unlimited).
    max_at_once: f32,
    /// Param 7 and `priority_reduction_by_distance`.
    priority: f32,
    priority_by_distance: f32,
}

impl Limits {
    fn of(p: &SoundParams) -> Self {
        Self {
            again: p.get(Param::DelayBeforeCanPlayAgain),
            max_at_once: p.get(Param::MaxNumberPlayingAtOnce),
            priority: p.get(Param::Priority),
            priority_by_distance: p.get(Param::PriorityReductionByDistance),
        }
    }

    /// Whether `max_number_playing_at_once` limits the event (0 or less, or 1000 or more: no limit).
    fn capped(&self) -> bool {
        self.max_at_once > 0.0 && self.max_at_once < 1000.0
    }

    /// A voice's priority at distance `d` from the listener: param 7 −
    /// `priority_reduction_by_distance` × d (§1.6; the × 0.95 in front of the listener is not
    /// done: PROVISIONAL).
    fn priority(&self, d: f32) -> f32 {
        self.priority - self.priority_by_distance * d
    }
}

/// The distance from the listener to `pos`; 0 for a 2D sound (`None`) or with no listener.
fn ear_distance(ear: Option<GlobalTransform>, pos: Option<Vec3>) -> f32 {
    match (pos, ear) {
        (Some(p), Some(t)) => p.distance(t.translation()),
        _ => 0.0,
    }
}

/// What [`Player::begin`] did with a prepared voice.
enum Begin {
    Started(Entity),
    /// Its file is decoding.
    Wait(Prepared),
    /// Its file cannot be played (logged where it failed).
    Failed,
    /// [`Player::admit`] refused it.
    Rejected,
}

/// The movie volume multiplier for a movie path or file name (`sound_events` movie table, names
/// lower case). CONFIRMED table; the name match (file name, with or without `.bik`) is INFERRED.
pub fn movie_volume(movies: &[(String, f32)], movie: &str) -> Option<f32> {
    let lower = movie.replace('/', "\\").to_ascii_lowercase();
    let file = lower.rsplit('\\').next().unwrap_or(&lower);
    let stem = file.strip_suffix(".bik").unwrap_or(file);
    movies.iter().find(|(n, _)| {
        let n = n.replace('/', "\\");
        let nf = n.rsplit('\\').next().unwrap_or(&n);
        nf == file || nf == stem || nf.strip_suffix(".bik") == Some(stem)
    }).map(|(_, v)| *v)
}

/// Starts movie sound tracks (from `crate::video`) as 2D voices.
fn movie_audio(mut msgs: MessageReader<PlayMovieAudio>, mut player: Player) {
    for m in msgs.read() {
        // The movie table entry is the voice's volume (movies not in the table play at 1.0);
        // `MOVIE_VOLUME` is the movie group's volume (CONFIRMED, §1.2) and the master group applies
        // too. INFERRED: no 2D multiplier for movie audio.
        let mult = player.data.as_deref().and_then(|d| movie_volume(&d.lib.events.movies, &m.movie));
        debug!("movie audio: {} table volume {mult:?}", m.movie);
        let control = VoiceControl::new(0.0, 0.0);
        let handle = player.clips.add(VoiceClip {
            data: ClipData::Live(m.feed.clone()),
            control: control.clone(),
            speed: 1.0,
            delay: 0.0,
            looped: false,
            loop_start: 0,
            loop_end: 0,
        });
        // Not tallied in `NewVoices`: no start rule runs later in this system, and the next
        // system sees the voice in its query.
        let voice = Voice {
            control,
            event: usize::MAX,
            position: None,
            min_dist: 0.0,
            max_dist: 0.0,
            falloff: 0.0,
            volume: mult.unwrap_or(1.0),
            group: VolumeGroup::Movie,
            looped: false,
            fade: 1.0,
            fade_rate: 0.0,
            equal_power: false,
        };
        player.commands.spawn((AudioPlayer::<VoiceClip>(handle), PlaybackSettings::ONCE, voice));
    }
}

fn play_sounds(mut msgs: MessageReader<PlaySound>, mut player: Player) {
    for m in msgs.read() {
        let Some(data) = player.data.as_ref().map(|d| d.lib.clone()) else { continue };
        let event = match &m.sound {
            SoundRef::Slot(s) => data.slot_event(s),
            SoundRef::Named(n) => data.event_by_name(n),
            SoundRef::Event(i) => Some(*i),
        };
        match event {
            Some(e) => {
                player.play(e, m.position, 0.0);
            }
            None => debug!("no sound event for {:?}", m.sound),
        }
    }
}

/// UI sounds (§2): the first existing candidate of [`ui_sound_choices`], always 2D. Named candidates
/// are `ui` events that carry their own name in the packed file (named categories).
fn ui_sounds(mut msgs: MessageReader<UiSound>, mut player: Player) {
    for m in msgs.read() {
        let Some(lib) = player.data.as_ref().map(|d| d.lib.clone()) else { continue };
        let event = ui_sound_choices(m.kind, &m.component).into_iter().find_map(|c| match c {
            UiSoundChoice::Slot(s) => lib.slot_event(s),
            UiSoundChoice::Named(n) => lib.names.find(&n).iter().copied().find(|&i| lib.events.events[i].name.is_some()),
        });
        match event {
            Some(e) => {
                player.play(e, None, 0.0);
            }
            // Most components have no hover / right-click sound: only clicks are worth a log line.
            None if matches!(m.kind, UiEvent::LClickUp | UiEvent::Shortcut) => debug!("no UI sound for {:?} {}", m.kind, m.component),
            None => {}
        }
    }
}

/// Projectile fire through the projectile-fire bank, with `audio_distance` from the
/// listener distance and the `AUDIO_DISTANCE_LAND_PROJECTILES_*` settings (INFERRED,
/// [`ProjectileKind`]). Allocates nothing per volley: the bank query is written into `query`,
/// kept across frames.
fn projectile_sounds(mut msgs: MessageReader<ProjectileFired>, mut player: Player, mut query: Local<Vec<Option<u32>>>) {
    let ear = listener_transform(&player.listener);
    for m in msgs.read() {
        let d = ear_distance(ear, Some(m.position));
        let Some((event, dist)) = projectile_event(player.data.as_deref(), m, d, &mut query) else { continue };
        // A volley: several shots spread over a short time (PROVISIONAL; the original plays one
        // sound per firing soldier from the animation cue).
        let n = m.shots.clamp(1, 6);
        let mut started = 0;
        for i in 0..n {
            let delay = if i == 0 { 0.0 } else { player.rng.next_f32() * 0.35 };
            started += usize::from(player.play(event, Some(m.position), delay).is_some());
        }
        debug!(
            "volley {} {} from ({:.0}, {:.0}) at {d:.0} m ({dist}): event {event}, {started}/{n} voices started",
            m.sound.gun_type, m.sound.shot_type, m.position.x, m.position.z
        );
    }
}

/// The projectile-fire bank's event for volley `m` heard at distance `d`, with its audio
/// distance name; `None` without sound data, a projectile-fire bank or a matching entry, or when
/// the entry names no event. Unknown names are left out of the bank query (they match nothing in
/// the original either). `query` is the reused query buffer.
fn projectile_event(data: Option<&SoundData>, m: &ProjectileFired, d: f32, query: &mut Vec<Option<u32>>) -> Option<(usize, &'static str)> {
    let data = data?;
    let bank_type = data.bank_projectile_fire?;
    let s = &*m.sound;
    // A projectile of no kind plays with the artillery bands (PROVISIONAL, see [`ProjectileKind::of`]).
    let (close, medium) = data.projectile_bands[s.kind.unwrap_or(ProjectileKind::Artillery) as usize];
    let dist = if d <= close { "close" } else if d <= medium { "medium" } else { "far" };
    let conditions = [("gun_type", s.gun_type.as_str()), ("shot_type", s.shot_type.as_str()), ("audio_distance", dist)];
    if !data.lib.vocabulary.query_known_into(bank_type, &conditions, query) {
        return None;
    }
    let entry = data.lib.banks.bank(bank_type)?.best_match(query)?;
    (entry.event != u32::MAX).then_some((entry.event as usize, dist))
}

/// Empties [`NewVoices`] (last frame's spawns are applied by now), takes in finished music decodes
/// and starts the plays that waited for them (every frame, before anything asks for a sound).
fn poll_loads(mut player: Player) {
    player.new_voices.0.clear();
    player.cache.poll_music();
    player.start_waiting();
}

/// The music system: feeds the music state machine ([`music_step`]) a state change, the end of
/// the playing track, and a tick, and applies what it asks for.
fn music(mut msgs: MessageReader<SetMusic>, mut music: ResMut<Music>, mut player: Player) {
    // Checked before anything starts this frame (a voice spawned now is not in the query yet).
    let gone = music.state.playing.map(|(v, _)| v).filter(|&v| player.voices.get(v).is_err());
    if let Some(c) = msgs.read().last() {
        music.name = c.state.clone().unwrap_or_default();
        let cands = music_candidates(c, &mut music.warned_states, &player);
        feed_music(&mut music, &mut player, MusicInput::SetState(cands));
    }
    if let Some(v) = gone {
        feed_music(&mut music, &mut player, MusicInput::Ended(v));
    }
    feed_music(&mut music, &mut player, MusicInput::Tick);
}

/// The candidate tracks of a music state: its bank entries (and the subculture's); empty for no
/// music, an unknown state (logged once) or no sound data. Only entries that name a state count
/// (not catch-alls). A silent placeholder (e.g. deployment music for subcultures without any) is
/// a track like any other: its silent file plays, as in the exe (see [`Player::prepare`]).
/// `random_number_selection` picks one at random (INFERRED).
fn music_candidates(c: &SetMusic, warned: &mut HashSet<String>, player: &Player) -> Vec<usize> {
    let Some(state_name) = c.state.as_deref() else { return Vec::new() };
    let Some(data) = player.data.as_deref() else { return Vec::new() };
    let lib = &data.lib;
    let voc = &lib.vocabulary;
    let Some(bank_type) = data.bank_music else { return Vec::new() };
    let Some(bank) = lib.banks.bank(bank_type) else { return Vec::new() };
    let mut conds: Vec<(&str, &str)> = vec![("music_state", state_name)];
    if let Some(s) = &c.subculture
        && voc.value(bank_type, "subculture", s).is_some()
    {
        conds.push(("subculture", s));
    }
    let Some(query) = voc.query(bank_type, &conds) else {
        if warned.insert(state_name.to_owned()) {
            warn!("unknown music state {state_name}: no music");
        }
        return Vec::new();
    };
    let k_state = voc.condition_index(bank_type, "music_state");
    bank.matching(&query)
        .filter(|e| k_state.is_some_and(|k| !e.conditions[k].is_empty()))
        .filter(|e| e.event != u32::MAX)
        .map(|e| e.event as usize)
        .collect()
}

/// Upper bound on the steps one input can chain. A chain only continues on a reply to a begin or
/// retry, and each Failed reply comes from a file just marked failed, which no later pick takes,
/// so the chain ends by itself; this only guards against a bug. (With no sound data a state
/// change has no candidates and takes the stop path, so nothing begins.)
const MUSIC_MAX_STEPS: usize = 1000;

/// Runs `input`, then each reply `one` gives back, for at most [`MUSIC_MAX_STEPS`] steps. True
/// when the chain was cut there.
fn run_music_chain(input: MusicInput, mut one: impl FnMut(MusicInput) -> Option<MusicInput>) -> bool {
    let mut next = Some(input);
    for _ in 0..MUSIC_MAX_STEPS {
        match next.take() {
            Some(i) => next = one(i),
            None => return false,
        }
    }
    next.is_some()
}

/// Whether `input` runs: once a chain was cut ([`Music::stalled`]) ticks do nothing, and the next
/// other input clears the stall and runs.
fn music_gate(stalled: &mut bool, input: &MusicInput) -> bool {
    if *stalled && *input == MusicInput::Tick {
        return false;
    }
    *stalled = false;
    true
}

/// Runs `input` through [`music_step`] and applies the actions, feeding each Begin/Retry reply
/// back in.
fn feed_music(music: &mut Music, player: &mut Player, input: MusicInput) {
    if !music_gate(&mut music.stalled, &input) {
        return;
    }
    let cut = run_music_chain(input, |input| music_once(music, player, input));
    if cut {
        music.stalled = true;
        error!("music state {}: more than {MUSIC_MAX_STEPS} steps for one input; music stopped until the next change", music.name);
    }
}

/// One step of [`music_step`] with its actions applied; returns the reply to feed back.
fn music_once(music: &mut Music, player: &mut Player, input: MusicInput) -> Option<MusicInput> {
    let now = player.time.elapsed_secs_f64();
    let (data, cache) = (player.data.as_deref(), &player.cache);
    let playable = |e: usize| data.and_then(|d| d.paths.get(e)).is_some_and(|files| cache.first_playable(files).is_some());
    let mut env = MusicEnv { now, playable: &playable, rng: &mut player.rng };
    let (state, actions) = music_step(std::mem::take(&mut music.state), input, &mut env);
    music.state = state;
    if music.state.waiting.is_none() {
        music.pending = None;
    }
    let mut next = None;
    for action in actions {
        match action {
            MusicAction::FadeOut(voice, event) => {
                let secs = player.event_fade_out(event);
                player.commands.entity(voice).try_insert(FadeOut(secs));
            }
            MusicAction::NothingPlayable => {
                if music.warned_states.insert(music.name.clone()) {
                    warn!("music state {}: no track has a file that can be played; no music", music.name);
                }
            }
            MusicAction::Begin(event) => {
                music.pending = None;
                next = Some(match player.prepare(event, None, 0.0) {
                    Some(p) => begin_music(music, player, p, false),
                    None => MusicInput::Rejected,
                });
            }
            MusicAction::Retry => {
                next = Some(match music.pending.take() {
                    Some(p) => begin_music(music, player, p, true),
                    None => MusicInput::Failed,
                });
            }
        }
    }
    next
}

/// Begins a music track (`waited`: the pending one, retried after its decode) and turns the
/// result into the state machine's reply.
fn begin_music(music: &mut Music, player: &mut Player, p: Prepared, waited: bool) -> MusicInput {
    let event = p.event;
    match player.begin(p, if waited { Ask::Waited } else { Ask::New }) {
        Begin::Started(voice) => MusicInput::Started { voice, event },
        Begin::Wait(p) => {
            music.pending = Some(p);
            MusicInput::Loading
        }
        Begin::Failed => MusicInput::Failed,
        Begin::Rejected => MusicInput::Rejected,
    }
}

/// Campaign ambience emitters (positional, looping).
fn campaign_ambience(
    mut msgs: MessageReader<CampaignAmbience>,
    running: Query<(Entity, &Voice), With<CampaignEmitterVoice>>,
    to_world: Res<CampaignToWorld>,
    mut player: Player,
) {
    for m in msgs.read() {
        for (_, v) in &running {
            v.control.stop();
        }
        player.waiting.0.retain(|w| !w.emitter);
        let Some(map) = &m.map else { continue };
        let Some(data) = player.data.as_deref().cloned() else { continue };
        let Some((_, list)) = data.lib.events.emitters.iter().find(|(k, _)| k.eq_ignore_ascii_case(map)) else { continue };
        for em in list {
            let pos = to_world.0.transform_point3(Vec3::from(em.pos));
            player.play_as(em.event as usize, Some(pos), 0.0, true);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Mixing
// ---------------------------------------------------------------------------------------------

type ListenerQuery<'w, 's> = (
    Query<'w, 's, &'static GlobalTransform, With<AudioListener>>,
    Query<'w, 's, &'static GlobalTransform, With<Camera3d>>,
);

fn listener_transform(q: &ListenerQuery) -> Option<GlobalTransform> {
    q.0.iter().next().or_else(|| q.1.iter().next()).copied()
}

/// Every frame: the original's mixing (MIDDLEWARE_VERIFY.md §1.3–1.9), fades, the past-max stop,
/// the low-pass, and cleanup of finished voices.
fn update_voices(
    mut commands: Commands,
    time: Res<Time<Real>>,
    vol: Res<Volumes>,
    settings: Res<MixSettings>,
    (mode, speed): (Res<AudioMode>, Res<GameSpeed>),
    listener: ListenerQuery,
    mut voices: Query<(Entity, &mut Voice, Option<&FadeOut>)>,
) {
    let ear = listener_transform(&listener);
    let dt = time.delta_secs();
    let master = vol.group(VolumeGroup::Master);
    for (e, mut v, fade_out) in &mut voices {
        if v.control.is_finished() {
            commands.entity(e).despawn();
            continue;
        }
        // Game speed ≠ 1: playing sfx fade out over 1 s (CONFIRMED `0x01001860`).
        if speed.0 != 1.0 && v.group == VolumeGroup::Sfx && v.fade_rate >= 0.0 {
            v.fade_rate = -1.0;
        }
        if let Some(f) = fade_out {
            commands.entity(e).remove::<FadeOut>();
            if f.0 <= 0.0 {
                v.control.stop();
                continue;
            }
            v.fade_rate = -1.0 / f.0;
        }
        // Fades are linear in time; a fade-out ends below 0.001 (§1.8).
        if v.fade_rate != 0.0 {
            v.fade = (v.fade + v.fade_rate * dt).clamp(0.0, 1.0);
            if v.fade_rate < 0.0 && v.fade < mixer::FADE_END {
                v.control.stop();
                continue;
            } else if v.fade_rate > 0.0 && v.fade >= 1.0 {
                v.fade_rate = 0.0;
            }
        }
        let fade = if v.equal_power { mixer::equal_power_fade(v.fade) } else { v.fade };
        let is_2d = v.position.is_none();
        let dim = if v.group == VolumeGroup::Movie {
            1.0
        } else if is_2d {
            settings.mult_2d
        } else {
            settings.mult_3d
        };
        // Movie sound goes out through Bink's own driver at `group(4) × 0.01 × group(5)`, without
        // the manager's speaker multiplier (CONFIRMED `0x004831D0`, BINK.md §7).
        let speaker = if v.group == VolumeGroup::Movie { 1.0 } else { settings.speaker_mult(vol.headphones) };
        let g = mixer::manager_gain(master, vol.group(v.group), v.volume, speaker, dim);
        // The voice volume Miles gets (pan always 0.5), then Miles' curve and centre pan.
        let side = mixer::miles_curve((g * fade).clamp(0.0, 1.0)) * mixer::CENTRE_PAN;
        let (l, r) = match (v.position, ear) {
            (Some(p), Some(t)) => {
                let to = p - t.translation();
                let d = to.length();
                // Past max: the exe stops the voice (§1.4). Looped voices are muted instead
                // (deviation: the original's emitter manager restarts them; INFERRED).
                if mixer::past_max(d, v.max_dist) {
                    if !v.looped {
                        v.control.stop();
                        continue;
                    }
                    v.control.set(0.0, 0.0);
                    continue;
                }
                let att = mixer::distance_gain(d, v.min_dist, v.max_dist, v.falloff, settings.rolloff);
                let dir = if d > 0.0001 {
                    let n = to / d;
                    [n.dot(*t.right()), n.dot(*t.up()), n.dot(*t.forward())]
                } else {
                    [0.0, 0.0, 1.0]
                };
                let (sl, sr) = mixer::stereo_3d_gains(dir, d, att);
                // Low-pass by distance, battle only (§1.7).
                let cutoff = if mode.campaign { 1.0 } else { mixer::low_pass_cutoff(d, settings.battle_distance_mult, settings.low_pass_slope, settings.low_pass_min) };
                v.control.set_cutoff(cutoff);
                (
                    mixer::channel_gain(side * sl * mixer::MILES_MASTER_LEVEL),
                    mixer::channel_gain(side * sr * mixer::MILES_MASTER_LEVEL),
                )
            }
            _ => {
                let c = mixer::voice_2d_gain((g * fade).clamp(0.0, 1.0));
                (c, c)
            }
        };
        v.control.set(l, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_cache_holds_one_copy_per_file_and_evicts_unused_files_lru() {
        use ntw_formats::sound::loop_points::mp3_frames;
        use ntw_formats::sound::test_files::mp3;
        let pcm = |n: usize| Pcm16 { channels: 1, sample_rate: 100, samples: vec![0; n] };
        let mut c = MusicCache::default();
        // Two loop blocks on one file, one ending at -1: one decoded copy, two frame ranges.
        let bytes = mp3(60, false, None, None);
        let f = mp3_frames(&bytes);
        let (a, b) = (f[10].offset as f32, f[40].offset as f32);
        assert!(c.insert("m".into(), pcm(60 * 1152), LoopIndex::new(&bytes), usize::MAX).is_empty());
        let Some((ClipData::Decoded(Samples::I16(x)), r1)) = c.clip("m", (a, b)) else { panic!("not cached") };
        let Some((ClipData::Decoded(Samples::I16(y)), r2)) = c.clip("m", (a, -1.0)) else { panic!("not cached") };
        assert!(Arc::ptr_eq(&x, &y));
        assert_eq!((r1, r2, c.files.len()), ((10 * 1152, 40 * 1152), (10 * 1152, 0), 1));
        drop((x, y));
        // A miss does not count as a use.
        let clock = c.clock;
        assert!(c.clip("absent", (0.0, 0.0)).is_none());
        assert_eq!(c.clock, clock);
        // 10 + 10 + 10 samples (60 bytes) over a 40-byte budget: `m` (in use) stays, the least
        // recently used unused file goes.
        let mut c = MusicCache::default();
        let none = || LoopIndex::new(&[]);
        c.insert("a".into(), pcm(10), none(), 40);
        c.insert("b".into(), pcm(10), none(), 40);
        let held = c.clip("a", (0.0, 0.0));
        let freed = c.insert("c".into(), pcm(10), none(), 40);
        assert_eq!(freed.len(), 1);
        assert!(c.files.contains_key("a") && !c.files.contains_key("b") && c.files.contains_key("c"));
        assert_eq!(c.bytes, 40);
        // Once nothing holds `a`, it is the oldest unused file.
        drop(held);
        c.clip("c", (0.0, 0.0));
        let freed = c.insert("d".into(), pcm(10), none(), 40);
        assert_eq!(freed.iter().map(|f| f.pcm.samples.len()).sum::<usize>(), 10);
        assert!(!c.files.contains_key("a") && c.files.contains_key("c"));
        assert_eq!(c.bytes, 40);
    }

    #[test]
    fn loop_regions_map_or_fall_back_to_the_whole_file() {
        use ntw_formats::sound::loop_points::mp3_frames;
        use ntw_formats::sound::test_files::mp3;
        let bytes = mp3(60, false, None, None);
        let f = mp3_frames(&bytes);
        let index = LoopIndex::new(&bytes);
        assert_eq!(loop_region(&index, 60 * 1152, (f[10].offset as f32, f[40].offset as f32)), Ok((10 * 1152, 40 * 1152)));
        // No block, or an end at or past the data: the exe sets none.
        assert_eq!(loop_region(&index, 60 * 1152, (0.0, 0.0)), Ok((0, 0)));
        assert_eq!(loop_region(&index, 60 * 1152, (418.0, bytes.len() as f32)), Ok((0, 0)));
        // An end past the last whole frame (a trailing tag) cannot be mapped.
        let mut tagged = mp3(20, false, None, None);
        let walked = tagged.len();
        tagged.extend_from_slice(&[0u8; 128]);
        assert!(loop_region(&LoopIndex::new(&tagged), 20 * 1152, (418.0, (walked + 60) as f32)).is_err());
        // A start on a last frame the decoder dropped (damaged): no audio after the start.
        let start = f[59].offset as f32;
        assert!(loop_region(&index, 59 * 1152, (start, -1.0)).is_err());
    }

    #[test]
    fn load_failures_are_logged_apart_from_loop_block_warnings() {
        let mut c = ClipCache::default();
        c.music.insert("m".into(), Pcm16 { channels: 1, sample_rate: 100, samples: vec![0; 4] }, LoopIndex::new(&[]), usize::MAX);
        // An unmappable block: warned once, the whole file plays.
        assert_eq!(c.music.clip("m", (1.0, 2.0)).map(|(_, r)| r), Some((0, 0)));
        assert!(c.music.warned_blocks.contains(&("m".to_owned(), (1f32.to_bits(), 2f32.to_bits()))));
        // A later load failure of the same file is still logged (its own set), once.
        assert!(c.fail("m", "read error"));
        assert!(!c.fail("m", "read error"));
    }

    #[test]
    fn a_music_file_that_decodes_to_no_audio_fails() {
        use ntw_formats::sound::test_files::wav;
        // A voice of it would end at once and be picked again every frame.
        assert!(decode_music(wav(&[], 22_050, 1), "empty.wav").is_err());
        assert_eq!(decode_music(wav(&[1, 2, 3], 22_050, 1), "short.wav").map(|(p, _)| p.frames()), Ok(3));
    }

    fn rng() -> AudioRng {
        AudioRng(0x9E37_79B9_7F4A_7C15)
    }

    fn voice(n: u32) -> Entity {
        Entity::from_raw_u32(n).expect("valid index")
    }

    /// One step with the events in `dead` (and 9) unplayable.
    fn step(s: &MusicState, input: MusicInput, now: f64, dead: &[usize]) -> (MusicState, Vec<MusicAction>) {
        let playable = |e: usize| e != 9 && !dead.contains(&e);
        let mut r = rng();
        music_step(s.clone(), input, &mut MusicEnv { now, playable: &playable, rng: &mut r })
    }

    fn st(cands: &[usize], playing: Option<(Entity, usize)>, waiting: Option<(usize, f64)>, last: Option<usize>) -> MusicState {
        MusicState { cands: cands.to_vec(), playing, waiting, last }
    }

    /// Every (state x input) pair of the music state machine. States: idle; a track playing; a
    /// track waiting for its decode with nothing playing; a track playing while the next one
    /// waits. Event 9 (and those listed as dead) has no playable file.
    #[test]
    fn music_state_machine_table() {
        use MusicAction::{Begin, FadeOut, NothingPlayable, Retry};
        use MusicInput::{Ended, Failed, Loading, Rejected, SetState, Started, Tick};
        const T0: f64 = 10.0;
        let now = T0 + 0.5;
        let late = T0 + MUSIC_SWITCH_TIMEOUT;
        let (v1, v2, v3, v9) = (voice(1), voice(2), voice(3), voice(9));
        let p1 = Some((v1, 1));
        let w2 = Some((2, T0));
        let idle = MusicState::default();
        let playing = st(&[1], p1, None, Some(1));
        let waiting = st(&[2], None, w2, None);
        let switching = st(&[2, 3], p1, w2, Some(1));
        type Row = (&'static str, MusicState, MusicInput, f64, &'static [usize], MusicState, Vec<MusicAction>);
        let rows: Vec<Row> = vec![
            // Idle.
            ("idle stop", idle.clone(), SetState(vec![]), now, &[], idle.clone(), vec![]),
            ("idle set", idle.clone(), SetState(vec![1]), now, &[], st(&[1], None, Some((1, now)), None), vec![Begin(1)]),
            ("idle set unplayable", idle.clone(), SetState(vec![9]), now, &[], st(&[9], None, None, None), vec![NothingPlayable]),
            ("idle ended", idle.clone(), Ended(v1), now, &[], idle.clone(), vec![]),
            ("idle tick", idle.clone(), Tick, now, &[], idle.clone(), vec![]),
            ("idle stray start", idle.clone(), Started { voice: v2, event: 2 }, now, &[], idle.clone(), vec![FadeOut(v2, 2)]),
            ("idle loading", idle.clone(), Loading, late, &[], idle.clone(), vec![]),
            ("idle failed", idle.clone(), Failed, now, &[], idle.clone(), vec![]),
            ("idle rejected", idle.clone(), Rejected, now, &[], idle.clone(), vec![]),
            // Playing.
            ("playing stop", playing.clone(), SetState(vec![]), now, &[], st(&[], None, None, Some(1)), vec![FadeOut(v1, 1)]),
            ("playing same", playing.clone(), SetState(vec![1]), now, &[], playing.clone(), vec![]),
            ("playing change", playing.clone(), SetState(vec![3]), now, &[], st(&[3], p1, Some((3, now)), Some(1)), vec![Begin(3)]),
            // Nothing of the new state plays: the old state's track does not carry over.
            ("playing change unplayable", playing.clone(), SetState(vec![9]), now, &[], st(&[9], None, None, Some(1)), vec![NothingPlayable, FadeOut(v1, 1)]),
            ("playing ended", playing.clone(), Ended(v1), now, &[], st(&[1], None, Some((1, now)), Some(1)), vec![Begin(1)]),
            ("playing other ended", playing.clone(), Ended(v9), now, &[], playing.clone(), vec![]),
            ("playing tick", playing.clone(), Tick, now, &[], playing.clone(), vec![]),
            ("playing stray start", playing.clone(), Started { voice: v2, event: 2 }, now, &[], playing.clone(), vec![FadeOut(v2, 2)]),
            ("playing loading", playing.clone(), Loading, late, &[], playing.clone(), vec![]),
            ("playing failed", playing.clone(), Failed, now, &[], playing.clone(), vec![]),
            ("playing rejected", playing.clone(), Rejected, now, &[], playing.clone(), vec![]),
            // Waiting, nothing playing.
            ("waiting stop", waiting.clone(), SetState(vec![]), now, &[], st(&[], None, None, None), vec![]),
            ("waiting same", waiting.clone(), SetState(vec![2]), now, &[], waiting.clone(), vec![]),
            // The old state's waiting track is replaced before anything retries it.
            ("waiting change", waiting.clone(), SetState(vec![3]), now, &[], st(&[3], None, Some((3, now)), None), vec![Begin(3)]),
            ("waiting same failed", waiting.clone(), SetState(vec![2]), now, &[2], st(&[2], None, None, None), vec![NothingPlayable]),
            ("waiting ended", waiting.clone(), Ended(v1), now, &[], waiting.clone(), vec![]),
            ("waiting tick", waiting.clone(), Tick, now, &[], waiting.clone(), vec![Retry]),
            ("waiting started", waiting.clone(), Started { voice: v2, event: 2 }, now, &[], st(&[2], Some((v2, 2)), None, Some(2)), vec![]),
            ("waiting stray start", waiting.clone(), Started { voice: v3, event: 3 }, now, &[], waiting.clone(), vec![FadeOut(v3, 3)]),
            ("waiting loading", waiting.clone(), Loading, now, &[], waiting.clone(), vec![]),
            ("waiting loading late", waiting.clone(), Loading, late, &[], waiting.clone(), vec![]),
            ("waiting failed", waiting.clone(), Failed, now, &[2], st(&[2], None, None, None), vec![NothingPlayable]),
            ("waiting rejected", waiting.clone(), Rejected, now, &[], st(&[2], None, None, None), vec![]),
            // Playing while the next one waits.
            ("switching stop", switching.clone(), SetState(vec![]), now, &[], st(&[], None, None, Some(1)), vec![FadeOut(v1, 1)]),
            ("switching back", switching.clone(), SetState(vec![1]), now, &[], st(&[1], p1, None, Some(1)), vec![]),
            ("switching same", switching.clone(), SetState(vec![2]), now, &[], st(&[2], p1, w2, Some(1)), vec![]),
            ("switching change", switching.clone(), SetState(vec![3]), now, &[], st(&[3], p1, Some((3, now)), Some(1)), vec![Begin(3)]),
            ("switching change unplayable", switching.clone(), SetState(vec![9]), now, &[], st(&[9], None, None, Some(1)), vec![NothingPlayable, FadeOut(v1, 1)]),
            ("switching ended", switching.clone(), Ended(v1), now, &[], st(&[2, 3], None, w2, Some(1)), vec![]),
            ("switching tick", switching.clone(), Tick, now, &[], switching.clone(), vec![Retry]),
            ("switching started", switching.clone(), Started { voice: v2, event: 2 }, now, &[], st(&[2, 3], Some((v2, 2)), None, Some(2)), vec![FadeOut(v1, 1)]),
            ("switching stray start", switching.clone(), Started { voice: v3, event: 3 }, now, &[], switching.clone(), vec![FadeOut(v3, 3)]),
            ("switching loading", switching.clone(), Loading, now, &[], switching.clone(), vec![]),
            // A slow decode releases the old track after the timeout.
            ("switching loading late", switching.clone(), Loading, late, &[], st(&[2, 3], None, w2, Some(1)), vec![FadeOut(v1, 1)]),
            // A failed file: another track at once, its hold still counted from the change.
            ("switching failed", switching.clone(), Failed, now, &[2], st(&[2, 3], p1, Some((3, T0)), Some(1)), vec![Begin(3)]),
            ("switching failed all", switching.clone(), Failed, now, &[2, 3], st(&[2, 3], None, None, Some(1)), vec![NothingPlayable, FadeOut(v1, 1)]),
            // Refused by the start rules: the old state's track does not carry over.
            ("switching rejected", switching.clone(), Rejected, now, &[], st(&[2, 3], None, None, Some(1)), vec![FadeOut(v1, 1)]),
        ];
        for (name, s, input, now, dead, want_state, want_actions) in rows {
            let (got_state, got_actions) = step(&s, input, now, dead);
            assert_eq!((got_state, got_actions), (want_state, want_actions), "{name}");
        }
    }

    #[test]
    fn music_picks_avoid_the_last_track_when_there_is_another() {
        let s = st(&[], None, None, Some(1));
        for seed in 1..50u64 {
            let playable = |_: usize| true;
            let mut r = AudioRng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let (n, a) = music_step(s.clone(), MusicInput::SetState(vec![1, 2]), &mut MusicEnv { now: 0.0, playable: &playable, rng: &mut r });
            assert_eq!((n.waiting, a), (Some((2, 0.0)), vec![MusicAction::Begin(2)]));
        }
    }

    /// A track whose file fails (e.g. it decodes to no audio) is not picked again: no new voice or
    /// retry every frame.
    #[test]
    fn a_failed_track_is_not_retried_every_frame() {
        let mut dead = vec![];
        let (s, a) = step(&MusicState::default(), MusicInput::SetState(vec![2]), 0.0, &dead);
        assert_eq!(a, vec![MusicAction::Begin(2)]);
        let (s, a) = step(&s, MusicInput::Loading, 0.0, &dead);
        assert!(a.is_empty());
        let (s, a) = step(&s, MusicInput::Tick, 0.1, &dead);
        assert_eq!(a, vec![MusicAction::Retry]);
        // Its load failed: the file is marked failed, the event has nothing left.
        dead.push(2);
        let (mut s, a) = step(&s, MusicInput::Failed, 0.1, &dead);
        assert_eq!(a, vec![MusicAction::NothingPlayable]);
        for frame in 0..100 {
            let (n, a) = step(&s, MusicInput::Tick, 0.2 + f64::from(frame) / 60.0, &dead);
            assert!(a.is_empty(), "frame {frame}: {a:?}");
            s = n;
        }
    }

    /// A test event: its files and the params that differ from volume 1, probability 1, no cap,
    /// group 1 (sfx).
    type TestEvent<'a> = (&'a [&'a str], &'a [(Param, f32)]);

    /// Params of a looping music event (group 0).
    const LOOPED_MUSIC: &[(Param, f32)] = &[(Param::Group, 0.0), (Param::Looped, 1.0)];

    /// A world holding what [`Player`] needs: sound data with `events` (files, params), an empty
    /// Vfs, a listener at the origin.
    fn player_world(events: &[TestEvent]) -> World {
        use ntw_formats::sound::{SoundBankDatabase, SoundEvent, SoundEvents};
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let mut params = Vec::new();
        let mut evs = Vec::new();
        for (i, (files, ps)) in events.iter().enumerate() {
            let mut p = SoundParams([0; ntw_formats::sound::events::PARAM_COUNT]);
            for &(k, v) in [(Param::Volume, 1.0), (Param::Probability, 1.0), (Param::MaxNumberPlayingAtOnce, 1000.0), (Param::Group, 1.0)].iter().chain(ps.iter()) {
                p.0[k as usize] = v.to_bits();
            }
            params.push(p);
            evs.push(SoundEvent { category: 0, name: None, params: i as u32, files: files.iter().map(|f| f.to_string()).collect() });
        }
        let events = SoundEvents { header: 0, categories: vec![], special_categories: [0; 6], params, events: evs, emitters: vec![], slots: vec![], movies: vec![] };
        let banks = SoundBankDatabase { header: 0, settings: vec![], banks: vec![] };
        let lib = SoundLibrary { events, banks, names: Default::default(), vocabulary: Default::default(), settings: HashMap::new() };
        let mut world = World::new();
        world.insert_resource(SoundData::new(lib, Vfs::new()));
        world.init_resource::<Assets<VoiceClip>>();
        world.init_resource::<ClipCache>();
        world.init_resource::<WaitingPlays>();
        world.init_resource::<NewVoices>();
        world.init_resource::<EventHistory>();
        world.init_resource::<AudioMode>();
        world.init_resource::<GameSpeed>();
        world.insert_resource(rng());
        world.insert_resource(Time::<Real>::default());
        world.insert_resource(MixSettings::SHIPPED);
        world.spawn((AudioListener, GlobalTransform::IDENTITY));
        world
    }

    /// A sound library with no events, the packed settings values `packed`, the given banks, and
    /// a vocabulary of the projectile-fire bank (type 1: gun_type, shot_type, audio_distance).
    fn projectile_lib(packed: Vec<f32>, banks: Vec<ntw_formats::sound::banks::SoundBank>) -> SoundLibrary {
        use ntw_formats::sound::bank_xml::{BankNames, BankVocabulary, ConditionNames};
        use ntw_formats::sound::{SoundBankDatabase, SoundEvents};
        let events = SoundEvents { header: 0, categories: vec![], special_categories: [0; 6], params: vec![], events: vec![], emitters: vec![], slots: vec![], movies: vec![] };
        let names = |tag: &str, v: &[(&str, u32)]| Some(ConditionNames { tag: tag.into(), values: v.iter().map(|&(n, k)| (n.into(), k)).collect() });
        let conditions = vec![
            names("gun_type", &[("musket_flintlock", 0), ("cannon", 1)]),
            names("shot_type", &[("bullet", 0), ("round_shot", 1)]),
            names("audio_distance", &[("close", 0), ("medium", 1), ("far", 2)]),
        ];
        let vocabulary = BankVocabulary { banks: HashMap::from([(1, BankNames { source: "sound_bank_projectile_fire".into(), conditions })]) };
        SoundLibrary { events, banks: SoundBankDatabase { header: 0, settings: packed, banks }, names: Default::default(), vocabulary, settings: HashMap::new() }
    }

    /// The projectile distance bands are read once at load from their packed slots, so an edited
    /// packed table wins; with no such slot, the shipped value of that kind (regression: 200 /
    /// 500 for every kind).
    #[test]
    fn projectile_distance_bands_are_read_at_load() {
        let bands = SoundData::new(projectile_lib(vec![], vec![]), Vfs::new()).projectile_bands;
        assert_eq!(bands[ProjectileKind::SmallArms as usize], (350.0, 1000.0));
        assert_eq!(bands[ProjectileKind::Arrow as usize], (200.0, 500.0));
        assert_eq!(bands[ProjectileKind::Artillery as usize], (50.0, 300.0));
        // A modded packed table: slot i holds 1000 + i; a table ending before slot 86.
        let bands = SoundData::new(projectile_lib((0..154).map(|i| 1000.0 + i as f32).collect(), vec![]), Vfs::new()).projectile_bands;
        assert_eq!(bands, [(1083.0, 1084.0), (1081.0, 1082.0), (1085.0, 1086.0)]);
        let bands = SoundData::new(projectile_lib((0..86).map(|i| 1000.0 + i as f32).collect(), vec![]), Vfs::new()).projectile_bands;
        assert_eq!(bands[ProjectileKind::Artillery as usize], (1085.0, 300.0));
    }

    /// The kind comes from the projectile row's category and missile type, in any case: a rifle
    /// or carbine (a `missile` firing a `bullet`, whatever its weapon family) is small arms, not
    /// artillery; a naval gun and a projectile no kind covers are `None`.
    #[test]
    fn projectile_kinds_by_category_and_missile_type() {
        assert_eq!(ProjectileKind::of("missile", "bullet"), Some(ProjectileKind::SmallArms));
        assert_eq!(ProjectileKind::of("Missile", "BULLET"), Some(ProjectileKind::SmallArms));
        assert_eq!(ProjectileKind::of("missile", "arrow"), Some(ProjectileKind::Arrow));
        assert_eq!(ProjectileKind::of("missile", "cannon_ball"), Some(ProjectileKind::Artillery));
        for gun in ["artillery", "Fort_Battery", "rocket"] {
            assert_eq!(ProjectileKind::of(gun, "bullet"), Some(ProjectileKind::Artillery), "{gun}");
        }
        assert_eq!(ProjectileKind::of("naval", "cannon_ball"), None);
        assert_eq!(ProjectileKind::of("missile", "grenade"), None);
        assert_eq!(ProjectileKind::of("special", "shrapnel"), None);
    }

    /// Every shipped `projectiles` row gets the bands the old weapon-family rule gave it (a musket,
    /// pistol, airgun, camel gun or puckle family is small arms, an `arrow` shot an arrow, the rest
    /// artillery; a row of no kind plays with the artillery bands). Needs the install; skipped
    /// (passes) otherwise.
    #[test]
    fn projectile_kinds_match_the_weapon_family_rule_on_every_shipped_row() {
        let dir = crate::config::game_data_dir();
        let Ok(db) = ntw_data::GameDatabase::from_install(&dir) else {
            eprintln!("skipped: no install at {}", dir.display());
            return;
        };
        let old = |p: &ntw_data::Projectile| {
            let g = p.weapon_family.as_deref().unwrap_or("none");
            if g.starts_with("musket") || ["pistol", "airgun", "camel_gun", "puckle"].contains(&g) {
                ProjectileKind::SmallArms
            } else if p.shot_type == "arrow" {
                ProjectileKind::Arrow
            } else {
                ProjectileKind::Artillery
            }
        };
        assert!(!db.projectiles.is_empty());
        for p in db.projectiles.iter() {
            let kind = ProjectileKind::of(&p.category, &p.missile_type).unwrap_or(ProjectileKind::Artillery);
            assert_eq!(kind, old(p), "{} (category {}, missile type {}, family {:?})", p.key, p.category, p.missile_type, p.weapon_family);
        }
    }

    /// A volley's bank entry: the most specific match for its gun, shot and audio distance (from
    /// its kind's bands; no kind: the artillery bands); an unknown name is left out of the query;
    /// an entry naming no event and a missing bank play nothing. The reused query buffer gives the
    /// same answers.
    #[test]
    fn projectile_event_picks_the_entry_for_gun_shot_and_distance() {
        use ntw_formats::sound::banks::{BankEntry, SoundBank};
        let e = |event: u32, c: [&[u32]; 3]| BankEntry { event, conditions: c.iter().map(|l| l.to_vec()).collect() };
        // Catch-all, musket close, musket medium/far, cannon (any distance), a cannon round shot
        // entry naming no event.
        let bank = SoundBank { bank_type: 1, entries: vec![e(9, [&[], &[], &[]]), e(10, [&[0], &[0], &[0]]), e(11, [&[0], &[0], &[1, 2]]), e(12, [&[1], &[], &[]]), e(u32::MAX, [&[1], &[1], &[]])] };
        let data = SoundData::new(projectile_lib(vec![], vec![bank]), Vfs::new());
        let fired = |gun: &str, shot: &str, kind: Option<ProjectileKind>| ProjectileFired {
            sound: Arc::new(ProjectileSound { gun_type: gun.into(), shot_type: shot.into(), kind }),
            position: Vec3::ZERO,
            shots: 1,
        };
        let (small_arms, artillery) = (Some(ProjectileKind::SmallArms), Some(ProjectileKind::Artillery));
        let mut q = Vec::new();
        let mut ev = |m: &ProjectileFired, d: f32| projectile_event(Some(&data), m, d, &mut q);
        // Small arms: close up to 350 m, medium up to 1000.
        assert_eq!(ev(&fired("musket_flintlock", "bullet", small_arms), 350.0), Some((10, "close")));
        assert_eq!(ev(&fired("musket_flintlock", "bullet", small_arms), 351.0), Some((11, "medium")));
        assert_eq!(ev(&fired("musket_flintlock", "bullet", small_arms), 1001.0), Some((11, "far")));
        // An unknown shot type is left out: the gun and distance still pick the musket entry.
        assert_eq!(ev(&fired("musket_flintlock", "buckshot", small_arms), 10.0), Some((10, "close")));
        // Artillery: close up to 50 m; a projectile of no kind uses the same bands.
        assert_eq!(ev(&fired("cannon", "canister", artillery), 60.0), Some((12, "medium")));
        assert_eq!(ev(&fired("cannon", "canister", None), 60.0), Some((12, "medium")));
        assert_eq!(ev(&fired("cannon", "round_shot", artillery), 10.0), None, "the best entry names no event");
        assert_eq!(projectile_event(None, &fired("cannon", "canister", artillery), 10.0, &mut Vec::new()), None);
        let no_bank = SoundData::new(projectile_lib(vec![], vec![]), Vfs::new());
        assert_eq!(projectile_event(Some(&no_bank), &fired("cannon", "canister", artillery), 10.0, &mut Vec::new()), None);
    }

    /// Runs `f` on a [`Player`] of `world`, then applies its commands.
    fn with_player<R>(world: &mut World, f: impl FnOnce(&mut Player) -> R) -> R {
        let mut state = bevy::ecs::system::SystemState::<Player>::new(world);
        let r = f(&mut state.get_mut(world).expect("player params"));
        state.apply(world);
        r
    }

    /// A playing voice of `event` at `pos`.
    fn spawn_voice(world: &mut World, event: usize, pos: Vec3) -> Arc<VoiceControl> {
        let control = VoiceControl::new(0.0, 0.0);
        let v = Voice { control: control.clone(), event, position: Some(pos), min_dist: 1.0, max_dist: 1e9, falloff: 0.0, volume: 1.0, group: VolumeGroup::Sfx, looped: false, fade: 1.0, fade_rate: 0.0, equal_power: false };
        world.spawn(v);
        control
    }


    /// The "no file paths" error is logged once per sound-data build, not once per process.
    #[test]
    fn the_missing_paths_error_is_logged_once_per_build() {
        let mut world = player_world(&[(&["a.wav"], &[])]);
        world.resource_mut::<SoundData>().paths = Arc::from(Vec::new());
        let logged = |w: &World| w.resource::<SoundData>().missing_paths_logged.load(std::sync::atomic::Ordering::Relaxed);
        assert!(!logged(&world));
        assert_eq!(with_player(&mut world, |p| p.play(0, None, 0.0)), None);
        assert!(logged(&world), "the first miss logs");
        assert_eq!(with_player(&mut world, |p| p.play(0, None, 0.0)), None);
        let fresh = SoundData::new(world.resource::<SoundData>().lib.as_ref().clone(), Vfs::new());
        assert!(!fresh.missing_paths_logged.load(std::sync::atomic::Ordering::Relaxed), "a new build logs again");
    }
    /// Under `max_number_playing_at_once`, the voice to replace is stopped only when the new one
    /// starts: not when its file fails or is still decoding.
    #[test]
    fn a_capped_voice_is_stolen_only_when_the_new_one_starts() {
        let capped: &[(Param, f32)] = &[(Param::MaxNumberPlayingAtOnce, 1.0), (Param::Priority, 10.0), (Param::PriorityReductionByDistance, 1.0)];
        let music: Vec<(Param, f32)> = LOOPED_MUSIC.iter().chain(capped).copied().collect();
        let mut world = player_world(&[(&["missing.wav"], capped), (&["m.mp3"], &music)]);
        // A far instance of event 0 (lower priority): a new one would replace it. Its file cannot
        // be read: the far one keeps playing.
        let far = spawn_voice(&mut world, 0, Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(with_player(&mut world, |p| p.play(0, None, 0.0)), None);
        assert!(!far.is_stopped(), "stopped for a file that failed");
        // Looping music (event 1) still decoding: the far one keeps playing while it waits.
        let far = spawn_voice(&mut world, 1, Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(with_player(&mut world, |p| p.play(1, None, 0.0)), None);
        assert!(!far.is_stopped(), "stopped while the new one decodes");
        assert_eq!(world.resource::<WaitingPlays>().0.len(), 1);
        // Decoded: it starts and only now replaces the far one.
        world.resource_mut::<ClipCache>().music.insert("m.mp3".into(), Pcm16 { channels: 1, sample_rate: 100, samples: vec![1; 10] }, LoopIndex::new(&[]), usize::MAX);
        with_player(&mut world, |p| p.start_waiting());
        assert!(far.is_stopped());
        assert!(world.resource::<WaitingPlays>().0.is_empty());
    }

    /// Looping music asked for outside the music state machine (a `PlaySound`, a UI sound, a
    /// campaign emitter) waits for its decode and starts then, instead of being dropped.
    #[test]
    fn looping_music_from_other_callers_starts_when_decoded() {
        let mut world = player_world(&[(&["m.mp3"], LOOPED_MUSIC), (&["n.mp3"], LOOPED_MUSIC)]);
        assert_eq!(with_player(&mut world, |p| p.play_as(0, None, 0.0, true)), None);
        assert_eq!(with_player(&mut world, |p| p.play(1, None, 0.0)), None);
        with_player(&mut world, |p| p.start_waiting());
        assert_eq!(world.resource::<WaitingPlays>().0.len(), 2, "still decoding");
        world.resource_mut::<ClipCache>().music.insert("m.mp3".into(), Pcm16 { channels: 1, sample_rate: 100, samples: vec![1; 10] }, LoopIndex::new(&[]), usize::MAX);
        with_player(&mut world, |p| p.start_waiting());
        let started: Vec<(usize, bool)> = world.query::<(&Voice, Has<CampaignEmitterVoice>)>().iter(&world).map(|(v, e)| (v.event, e)).collect();
        assert_eq!(started, vec![(0, true)]);
        // The other one's file failed: dropped, not retried.
        world.resource_mut::<ClipCache>().fail("n.mp3", "test");
        with_player(&mut world, |p| p.start_waiting());
        assert!(world.resource::<WaitingPlays>().0.is_empty());
    }

    /// A file that failed is skipped for the event's next file; with none left nothing plays.
    #[test]
    fn failed_files_are_skipped() {
        let mut world = player_world(&[(&["a.wav", "b.wav"], &[])]);
        world.resource_mut::<ClipCache>().fail("a.wav", "test");
        for _ in 0..20 {
            assert_eq!(with_player(&mut world, |p| p.prepare(0, None, 0.0).map(|p| p.path.to_string())), Some("b.wav".to_owned()));
        }
        world.resource_mut::<ClipCache>().fail("b.wav", "test");
        assert!(with_player(&mut world, |p| p.prepare(0, None, 0.0).is_none()));
        // Nothing failed: both files are picked.
        let mut world = player_world(&[(&["a.wav", "b.wav"], &[])]);
        let picks: HashSet<String> = (0..50).filter_map(|_| with_player(&mut world, |p| p.prepare(0, None, 0.0).map(|p| p.path.to_string()))).collect();
        assert_eq!(picks.len(), 2);
    }

    /// Decoded samples for `path` in the music cache (as if its background load finished).
    fn decoded(world: &mut World, path: &str) {
        world.resource_mut::<ClipCache>().music.insert(path.into(), Pcm16 { channels: 1, sample_rate: 100, samples: vec![1; 10] }, LoopIndex::new(&[]), usize::MAX);
    }

    fn voices_of(world: &mut World, event: usize) -> usize {
        world.query::<&Voice>().iter(world).filter(|v| v.event == event).count()
    }

    /// Round 14 item 1: the same looping event asked for on several frames waits once, and the
    /// start rules that depend on what plays (at-once limit, repeat limit) are applied when it
    /// starts, not when it was asked for.
    #[test]
    fn a_waiting_play_waits_once_per_event_and_meets_the_start_rules_when_it_starts() {
        let capped: Vec<(Param, f32)> = LOOPED_MUSIC.iter().chain(&[(Param::MaxNumberPlayingAtOnce, 1.0), (Param::Priority, 10.0)]).copied().collect();
        let again: Vec<(Param, f32)> = LOOPED_MUSIC.iter().chain(&[(Param::DelayBeforeCanPlayAgain, 60.0)]).copied().collect();
        let mut world = player_world(&[(&["m.mp3"], LOOPED_MUSIC), (&["c.mp3"], &capped), (&["r.mp3"], &again)]);
        for _ in 0..5 {
            assert_eq!(with_player(&mut world, |p| p.play(0, None, 0.0)), None);
        }
        assert_eq!(world.resource::<WaitingPlays>().0.len(), 1);
        decoded(&mut world, "m.mp3");
        with_player(&mut world, |p| p.start_waiting());
        assert_eq!(voices_of(&mut world, 0), 1, "one voice, not one per request");
        // At most one at once: an instance of the same priority started while it waited.
        assert_eq!(with_player(&mut world, |p| p.play(1, None, 0.0)), None);
        let other = spawn_voice(&mut world, 1, Vec3::ZERO);
        decoded(&mut world, "c.mp3");
        with_player(&mut world, |p| p.start_waiting());
        assert_eq!(voices_of(&mut world, 1), 1);
        assert!(!other.is_stopped());
        // Repeat limit: it played while the request waited.
        assert_eq!(with_player(&mut world, |p| p.play(2, None, 0.0)), None);
        world.resource_mut::<EventHistory>().last_played.insert(2, 0.0);
        decoded(&mut world, "r.mp3");
        with_player(&mut world, |p| p.start_waiting());
        assert_eq!(voices_of(&mut world, 2), 0);
        assert!(world.resource::<WaitingPlays>().0.is_empty());
    }

    /// Round 15 item 1: waiting plays are told apart by event, emitter and position: two emitters
    /// of one event both start, and a `PlaySound` of it does not take an emitter's place (which
    /// would leave a looping voice no ambience change stops).
    #[test]
    fn waiting_plays_of_one_event_from_different_requests_all_start() {
        let far: Vec<(Param, f32)> = LOOPED_MUSIC.iter().chain(&[(Param::MinDist, 1.0), (Param::MaxDist, 1000.0)]).copied().collect();
        let mut world = player_world(&[(&["m.mp3"], &far)]);
        let (a, b) = (Some(Vec3::X), Some(Vec3::Y));
        with_player(&mut world, |p| {
            p.play_as(0, a, 0.0, true);
            p.play_as(0, b, 0.0, true);
            p.play_as(0, a, 0.0, true);
            p.play(0, a, 0.0);
        });
        assert_eq!(world.resource::<WaitingPlays>().0.len(), 3);
        decoded(&mut world, "m.mp3");
        with_player(&mut world, |p| p.start_waiting());
        let mut started: Vec<(bool, [i32; 3])> =
            world.query::<(&Voice, Has<CampaignEmitterVoice>)>().iter(&world).map(|(v, e)| (e, v.position.expect("3D").as_ivec3().to_array())).collect();
        started.sort();
        assert_eq!(started, vec![(false, [1, 0, 0]), (true, [0, 1, 0]), (true, [1, 0, 0])]);
    }

    /// Round 16 item 1: voices started earlier in the same system (their spawn not applied yet)
    /// count for every start rule, for waiting and normal plays alike: with
    /// `max_number_playing_at_once` = 1, two emitters whose decode finishes in one frame start
    /// one voice, and two plays asked for in one system start one.
    #[test]
    fn voices_started_in_the_same_frame_count_for_the_at_once_cap() {
        let capped: Vec<(Param, f32)> = LOOPED_MUSIC.iter().chain(&[(Param::MinDist, 1.0), (Param::MaxDist, 1000.0), (Param::MaxNumberPlayingAtOnce, 1.0)]).copied().collect();
        let sfx: &[(Param, f32)] = &[(Param::MaxNumberPlayingAtOnce, 1.0)];
        let mut world = player_world(&[(&["m.mp3"], &capped), (&["a.wav"], sfx)]);
        with_player(&mut world, |p| {
            p.play_as(0, Some(Vec3::X), 0.0, true);
            p.play_as(0, Some(Vec3::Y), 0.0, true);
        });
        assert_eq!(world.resource::<WaitingPlays>().0.len(), 2);
        decoded(&mut world, "m.mp3");
        with_player(&mut world, |p| p.start_waiting());
        assert_eq!(voices_of(&mut world, 0), 1);
        // Normal plays (the clip cache holds the file, so both would start at once).
        world.resource_mut::<ClipCache>().clips.insert("a.wav".into(), Arc::new(Pcm { channels: 1, sample_rate: 100, samples: vec![0.5; 10] }));
        let started = with_player(&mut world, |p| (p.play(1, None, 0.0).is_some(), p.play(1, None, 0.0).is_some()));
        assert_eq!(started, (true, false));
        assert_eq!(voices_of(&mut world, 1), 1);
        // Next frame: the tally is emptied and the query shows the voice; still one.
        bevy::ecs::system::RunSystemOnce::run_system_once(&mut world, poll_loads).expect("poll_loads");
        assert!(with_player(&mut world, |p| p.play(1, None, 0.0)).is_none());
    }

    /// Round 17: a stopped voice (e.g. the instance a new one replaced) does not count, whether
    /// the query shows it or it was started this frame.
    #[test]
    fn stopped_voices_do_not_count_for_the_start_rules() {
        let sfx: &[(Param, f32)] = &[(Param::MaxNumberPlayingAtOnce, 1.0)];
        let mut world = player_world(&[(&["a.wav"], sfx)]);
        world.resource_mut::<ClipCache>().clips.insert("a.wav".into(), Arc::new(Pcm { channels: 1, sample_rate: 100, samples: vec![0.5; 10] }));
        let old = spawn_voice(&mut world, 0, Vec3::ZERO);
        assert!(with_player(&mut world, |p| p.play(0, None, 0.0)).is_none(), "the cap holds while it plays");
        old.stop();
        // In the query, stopped: the new one starts; then, in the same system, stop that one
        // (still only in the tally): another starts.
        let started = with_player(&mut world, |p| {
            let a = p.play(0, None, 0.0);
            let tallied = p.new_voices.0.last().expect("tallied");
            assert_eq!(Some(tallied.entity), a, "the tally holds the voice play() started");
            tallied.control.stop();
            (a.is_some(), p.play(0, None, 0.0).is_some(), p.play(0, None, 0.0).is_some())
        });
        assert_eq!(started, (true, true, false));
    }

    /// Round 15 items 2 and 3: the repeat limit refuses a sound before anything is drawn, and the
    /// at-once limit right after the probability draw, before the file, pitch and delay draws
    /// and before the path or voice control is allocated (the order before round 14).
    #[test]
    fn limits_refuse_before_the_file_pitch_and_delay_draws() {
        let half = |extra: (Param, f32)| -> Vec<(Param, f32)> { vec![(Param::Probability, 0.5), extra] };
        let again = half((Param::DelayBeforeCanPlayAgain, 60.0));
        let capped = half((Param::MaxNumberPlayingAtOnce, 1.0));
        let mut world = player_world(&[(&["a.wav", "b.wav"], &again), (&["a.wav", "b.wav"], &capped)]);
        world.resource_mut::<EventHistory>().last_played.insert(0, 0.0);
        spawn_voice(&mut world, 1, Vec3::ZERO);
        let before = world.resource::<AudioRng>().0;
        assert!(with_player(&mut world, |p| p.prepare(0, None, 0.0)).is_none());
        assert_eq!(world.resource::<AudioRng>().0, before, "repeat limit: nothing drawn");
        for _ in 0..20 {
            let before = world.resource::<AudioRng>().0;
            let mut one_draw = AudioRng(before);
            one_draw.next_f32();
            assert!(with_player(&mut world, |p| p.prepare(1, None, 0.0)).is_none());
            assert_eq!(world.resource::<AudioRng>().0, one_draw.0, "at-once limit: only the probability drawn");
        }
    }

    /// Round 14 item 2: a waiting play is dropped after [`WAITING_PLAY_TIMEOUT`] and on every
    /// game-mode change (a front-end request never starts in a battle).
    #[test]
    fn waiting_plays_time_out_and_are_dropped_on_a_mode_change() {
        // Round 15 item 4: a decode that has finished on the timeout frame still starts; one still
        // decoding then is dropped.
        let mut world = player_world(&[(&["m.mp3"], LOOPED_MUSIC), (&["slow.mp3"], LOOPED_MUSIC)]);
        world.resource_mut::<Time<Real>>().update_with_duration(std::time::Duration::ZERO);
        assert_eq!(with_player(&mut world, |p| p.play(0, None, 0.0)), None);
        assert_eq!(with_player(&mut world, |p| p.play(1, None, 0.0)), None);
        world.resource_mut::<Time<Real>>().update_with_duration(std::time::Duration::from_secs_f64(WAITING_PLAY_TIMEOUT));
        decoded(&mut world, "m.mp3");
        with_player(&mut world, |p| p.start_waiting());
        assert_eq!((voices_of(&mut world, 0), voices_of(&mut world, 1)), (1, 0));
        assert!(world.resource::<WaitingPlays>().0.is_empty());

        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin).init_state::<GameMode>().init_resource::<WaitingPlays>();
        app.add_systems(Update, drop_waiting_plays.run_if(state_changed::<GameMode>));
        let mut world = player_world(&[(&["m.mp3"], LOOPED_MUSIC)]);
        with_player(&mut world, |p| p.play(0, None, 0.0));
        let waiting = world.remove_resource::<WaitingPlays>().expect("waiting plays");
        app.update();
        app.insert_resource(waiting);
        app.update();
        assert_eq!(app.world().resource::<WaitingPlays>().0.len(), 1, "kept while the mode stays");
        app.world_mut().resource_mut::<NextState<GameMode>>().set(GameMode::Battle);
        app.update();
        assert!(app.world().resource::<WaitingPlays>().0.is_empty());
    }

    /// Round 14 item 3: non-looping music is decoded whole before a voice starts (no stream), so
    /// a file that fails is marked failed and not picked again; one that decodes to no audio fails
    /// too ([`a_music_file_that_decodes_to_no_audio_fails`]).
    #[test]
    fn non_looping_music_is_decoded_first_and_a_failed_file_is_not_picked_again() {
        let mut world = player_world(&[(&["missing.mp3"], &[(Param::Group, 0.0), (Param::Streamed, 1.0)])]);
        let p = with_player(&mut world, |p| p.prepare(0, None, 0.0)).expect("prepared");
        assert_eq!(p.music_block, Some((0.0, 0.0)));
        assert_eq!(with_player(&mut world, |p| p.play(0, None, 0.0)), None);
        assert_eq!(world.resource::<WaitingPlays>().0.len(), 1, "waits for its decode, no stream voice");
        let start = std::time::Instant::now();
        while !world.resource::<ClipCache>().failed.contains("missing.mp3") {
            assert!(start.elapsed().as_secs() < 10, "load never finished");
            std::thread::sleep(std::time::Duration::from_millis(5));
            world.resource_mut::<ClipCache>().poll_music();
        }
        with_player(&mut world, |p| p.start_waiting());
        assert!(world.resource::<WaitingPlays>().0.is_empty());
        assert_eq!(voices_of(&mut world, 0), 0);
        assert!(with_player(&mut world, |p| p.prepare(0, None, 0.0)).is_none(), "picked again");
    }

    /// Round 14 item 4: a silent placeholder file plays like any other (the exe has no name rule
    /// for it), so the music it stands for replaces the old track instead of leaving it playing.
    #[test]
    fn a_silent_placeholder_file_plays_like_any_other() {
        let mut world = player_world(&[(&[r"music\Silent Placeholder.wav"], LOOPED_MUSIC)]);
        let p = with_player(&mut world, |p| p.prepare(0, None, 0.0)).expect("a placeholder pick plays");
        assert_eq!(&*p.path, r"music\silent placeholder.wav");
    }

    /// Round 14 item 6: event files are normalized once at load, so a failure under one spelling
    /// covers every spelling of the path, and a check or pick builds no string.
    #[test]
    fn failed_file_check_matches_any_spelling_of_the_path() {
        let mut world = player_world(&[(&["A/B.WAV"], &[]), (&[r" a\\b.wav "], &[]), (&["a/b.wa"], &[])]);
        let paths = world.resource::<SoundData>().paths.clone();
        assert_eq!((&*paths[0][0], &*paths[1][0], &*paths[2][0]), (r"a\b.wav", r"a\b.wav", r"a\b.wa"));
        assert!(with_player(&mut world, |p| p.prepare(0, None, 0.0)).is_some());
        world.resource_mut::<ClipCache>().fail(r"a\b.wav", "test");
        assert!(with_player(&mut world, |p| p.prepare(0, None, 0.0)).is_none());
        assert!(with_player(&mut world, |p| p.prepare(1, None, 0.0)).is_none());
        assert!(with_player(&mut world, |p| p.prepare(2, None, 0.0)).is_some());
        let c = world.resource::<ClipCache>();
        assert_eq!((c.first_playable(&[paths[0][0].clone(), paths[2][0].clone()]), c.first_playable(&paths[1])), (Some(1), None));
    }

    /// A waiting play still decoding at its timeout is dropped without asking for a new decode;
    /// before the timeout, one whose load is gone (e.g. finished and evicted) asks again.
    #[test]
    fn a_timed_out_play_asks_for_no_new_decode() {
        use std::time::Duration;
        let mut world = player_world(&[(&["m.mp3"], LOOPED_MUSIC)]);
        world.resource_mut::<Time<Real>>().update_with_duration(Duration::ZERO);
        assert_eq!(with_player(&mut world, |p| p.play(0, None, 0.0)), None);
        world.resource_mut::<ClipCache>().music.loads.clear();
        world.resource_mut::<Time<Real>>().update_with_duration(Duration::from_secs_f64(WAITING_PLAY_TIMEOUT / 2.0));
        with_player(&mut world, |p| p.start_waiting());
        assert_eq!((world.resource::<WaitingPlays>().0.len(), world.resource::<ClipCache>().music.loads.len()), (1, 1));
        world.resource_mut::<ClipCache>().music.loads.clear();
        world.resource_mut::<Time<Real>>().update_with_duration(Duration::from_secs_f64(WAITING_PLAY_TIMEOUT / 2.0));
        with_player(&mut world, |p| p.start_waiting());
        assert!(world.resource::<WaitingPlays>().0.is_empty());
        assert!(world.resource::<ClipCache>().music.loads.is_empty(), "a decode asked for a dropped play");
    }

    /// Round 14 item 7: a chain past [`MUSIC_MAX_STEPS`] is cut after exactly that many steps, and
    /// once cut, ticks do nothing until another input.
    #[test]
    fn a_runaway_music_chain_is_cut_once_and_ticks_stop() {
        let mut steps = 0;
        assert!(run_music_chain(MusicInput::Tick, |_| {
            steps += 1;
            Some(MusicInput::Failed)
        }));
        assert_eq!(steps, MUSIC_MAX_STEPS);
        let mut steps = 0;
        assert!(!run_music_chain(MusicInput::Tick, |i| {
            steps += 1;
            (i == MusicInput::Tick).then_some(MusicInput::Loading)
        }));
        assert_eq!(steps, 2);
        let mut stalled = true;
        for _ in 0..3 {
            assert!(!music_gate(&mut stalled, &MusicInput::Tick));
        }
        assert!(stalled);
        assert!(music_gate(&mut stalled, &MusicInput::SetState(vec![])));
        assert!(!stalled && music_gate(&mut stalled, &MusicInput::Tick));
    }

    #[test]
    fn ui_sound_rules() {
        use UiSoundChoice::{Named, Slot};
        assert_eq!(ui_sound_choices(UiEvent::LClickUp, "grand_campaign"), vec![Named("grand_campaign".into()), Slot("DEFAULT_UI_SOUND")]);
        assert_eq!(ui_sound_choices(UiEvent::Shortcut, "entry_inf_3"), vec![Slot("UNIT_CARD_SELECTED")]);
        assert_eq!(ui_sound_choices(UiEvent::RClickUp, "entry_inf_3"), vec![Slot("UNIT_CARD_RIGHT_CLICK_SELECTED")]);
        assert_eq!(ui_sound_choices(UiEvent::RClickUp, "button_ok"), vec![Named("right_click_button_ok".into())]);
        assert_eq!(ui_sound_choices(UiEvent::Move, "music_volume"), vec![Named("slider_moved_music_volume".into())]);
        assert_eq!(ui_sound_choices(UiEvent::MouseOn, "item3"), vec![Named("mouse_over_Slot1".into())]);
        assert_eq!(
            ui_sound_choices(UiEvent::MouseOn, "entry_cav_1"),
            vec![Named("mouse_over_entry_cav_1".into()), Slot("mouse_over_unit_card")]
        );
    }

    /// The loudness at the user's real preferences (MIDDLEWARE_VERIFY.md §1.3 "Loudness check").
    #[test]
    fn loudness_at_user_preferences() {
        let g = |v: i32| mixer::GroupVolume { enabled: true, volume: v };
        let click = mixer::voice_2d_gain(mixer::manager_gain(g(100), g(100), 0.07, 1.0, 2.0));
        assert!((click - 0.306).abs() < 0.002, "{click}");
        let music = mixer::voice_2d_gain(mixer::manager_gain(g(100), g(16), 0.3, 1.0, 2.0));
        assert!((music - 0.164).abs() < 0.002, "{music}");
        assert_eq!(mixer::voice_2d_gain(mixer::manager_gain(g(100), g(100), 0.3, 1.0, 2.0)), 1.0);
    }
}
