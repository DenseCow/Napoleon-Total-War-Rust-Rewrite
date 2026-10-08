//! Movie playback: Bink movies (`.bik`) from the install, decoded by our own
//! `ntw_formats::bink` (the original's `binkw32.dll` is never used), shown as a texture at the
//! movie's frame rate, with the sound through our audio system (`crate::audio`, movie volumes from
//! `sound_events`). Notes: `analysis/video/BINK.md` (player section).
//!
//! # Hooks
//! - [`PlayMovie`]: play a movie full screen (intro, cutscenes: `ncs_*`, `nhb_*`, duel clips...)
//!   or into a texture only ([`MovieMode::Texture`], e.g. inside a UI component). Finished or
//!   skipped movies send [`MovieFinished`]. [`StopMovie`] stops movies.
//! - Start-up movies before the front end (CONFIRMED list and order, [`INTRO_MOVIES`]): on by default;
//!   `--no-intro` turns them off, harness runs (see [`is_harness_run`]) skip them unless `--intro`.
//! - `--play-movie <name>`: play one movie full screen at start (test harness).
//! - The front end's background movie (`Frontend2.bik`, looped) in place of the layout's still image: on
//!   by default (`--no-frontend-movie` off; harness runs only with `--frontend-movie`).
//!
//! Skipping (PROVISIONAL, `BINK.md`): any key or mouse button skips a skippable full-screen movie.

pub mod source;

use std::sync::{Arc, Mutex};

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};
use bevy::window::PrimaryWindow;
use ntw_formats::pack::Vfs;

use crate::audio::PlayMovieAudio;
use crate::config;
use crate::GameMode;
use source::{BinkSource, MovieInfo, MovieSource, MovieStream, Planes, Poll};

/// Converts a movie's three planes to RGB in its texture: the game's movie-shader maths
/// (`movie_yuv.wgsl`, `BINK.md` §6), drawn by an off-screen camera onto [`Movie::image`].
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct MovieYuvMaterial {
    #[uniform(0)]
    params: MovieYuvParams,
    #[texture(1)]
    #[sampler(2)]
    y: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    cb: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    cr: Handle<Image>,
}

#[derive(bevy::render::render_resource::ShaderType, Debug, Clone)]
struct MovieYuvParams {
    /// xy: visible size; z: `2 / gfx_gamma_setting`; w: `gfx_brightness_setting / 1.2`.
    view: Vec4,
    /// xy: Y plane size; zw: chroma plane size.
    sizes: Vec4,
}

impl Material2d for MovieYuvMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://napoleon/video/movie_yuv.wgsl".into()
    }
}

/// The renderer settings the movie shader uses (`gfx_gamma_setting`, `gfx_brightness_setting`).
/// PROVISIONAL: the defaults (2 and 1.2, which make both steps no-ops); not yet read from the
/// player's preferences.
const GAMMA: f32 = 2.0;
const BRIGHTNESS: f32 = 1.2;

/// The start-up movies, in order. CONFIRMED set: Napoleon.exe `0x00484D30` (front-end UI vtable
/// slot 34, run once per start) queues `NTW_Intro.bik`, `Corei7_Intro.bik` and
/// `SEGA_logo_sting_HD.bik`, in that call order, unconditionally. Order CONFIRMED (2026-10-04): the
/// queue player `0x0048B5B0` takes the **last** entry (count at +0x1BC, 0x14-byte entries at +0x1C0)
/// and shrinks the count, so the movies play SEGA logo, Intel logo, then the intro.
pub const INTRO_MOVIES: [&str; 3] = ["movies\\sega_logo_sting_hd.bik", "movies\\corei7_intro.bik", "movies\\ntw_intro.bik"];

/// The Bink sound-track id for a language code (`language.txt`, e.g. `EN`). CONFIRMED from
/// Napoleon.exe: the table at `0x01419A58` maps EN 0, FR 1, DE 2, ES 3, IT 4, RU 5, PO 6, CZ 7
/// (`0x0121AB70`, unknown code = 8), and the movie opener (`0x01215C10`) plays track 0 for 7
/// (Czech has no dubbed track) and for an id past the track count, through
/// `BinkSetSoundTrack(1, &id)`.
pub fn language_track(code: &str) -> u32 {
    const LANGS: [&str; 8] = ["EN", "FR", "DE", "ES", "IT", "RU", "PO", "CZ"];
    match LANGS.iter().position(|l| l.eq_ignore_ascii_case(code.trim())) {
        Some(i) if i < 7 => i as u32,
        _ => 0,
    }
}

/// The front end's background movie. CONFIRMED: Napoleon.exe `0x004858D0` looks up component
/// `movie_bg` and plays `Frontend2.bik` in it through `0x004831D0(name, 0, 1, 0)` (looped: INFERRED).
pub const FRONTEND_MOVIE: &str = "movies\\frontend2.bik";

/// Render layer of the full-screen movie camera and picture.
const MOVIE_LAYER: usize = 30;

/// How a movie is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieMode {
    /// Over everything, letterboxed to the window, on black.
    Fullscreen { skippable: bool },
    /// Only into its texture ([`Movie::image`]); the caller draws it.
    Texture,
}

/// Play a movie. `path` is a Vfs path (`movies\ncs_09_waterloo.bik`) or a bare file name
/// (`ncs_09_waterloo.bik`, looked up under `movies\`).
#[derive(Message, Debug, Clone)]
pub struct PlayMovie {
    pub path: String,
    pub mode: MovieMode,
    /// Sound track id (`None` = the player's language, [`language_track`]).
    pub track: Option<u32>,
    pub looped: bool,
    /// Play the sound track.
    pub sound: bool,
}

impl PlayMovie {
    /// A skippable full-screen movie with sound (cutscenes).
    pub fn cutscene(path: impl Into<String>) -> Self {
        Self { path: path.into(), mode: MovieMode::Fullscreen { skippable: true }, track: None, looped: false, sound: true }
    }
}

/// A movie ended (`skipped` = the player skipped it, or it was stopped).
#[derive(Message, Debug, Clone)]
#[allow(dead_code)] // a hook: read by whoever started the movie
pub struct MovieFinished {
    pub path: String,
    pub skipped: bool,
    pub entity: Entity,
}

/// Stop the movie(s) playing `path` (`None` = all).
#[derive(Message, Debug, Clone)]
pub struct StopMovie {
    pub path: Option<String>,
}

/// A playing movie. Its picture is [`Movie::image`], updated as frames fall due.
#[derive(Component)]
pub struct Movie {
    pub path: String,
    /// The RGBA picture (a render target the conversion pass draws into).
    pub image: Handle<Image>,
    /// The Y, Cb, Cr plane textures the decoded frames are uploaded to.
    planes: [Handle<Image>; 3],
    pub mode: MovieMode,
    info: MovieInfo,
    stream: Mutex<MovieStream>,
    /// Real time at which frame 0 was shown.
    start: Option<f64>,
    pending: Option<source::DecodedFrame>,
    /// Frame number now on screen.
    shown: Option<u64>,
    sound: bool,
}

impl Movie {
    #[allow(dead_code)] // tests and hooks
    /// Frame number on screen (counting on through loops).
    pub fn frame(&self) -> Option<u64> {
        self.shown
    }
}

/// The full-screen picture of a movie (child of nothing; despawned with its movie).
#[derive(Component)]
struct MovieScreen(Entity);

/// The texture of the front end's background movie, when `--frontend-movie` is on. The front-end
/// renderer draws it in place of the `movie_bg` still image.
#[derive(Resource, Clone)]
pub struct FrontEndMovieTexture(pub Handle<Image>);

/// The intro playlist still to play (`--intro`).
#[derive(Resource, Default)]
struct IntroQueue(Vec<String>);

/// `--movie-hold <frame>` (test harness): movies stop advancing at this frame, so a
/// `--screenshot` captures a known frame (compared with `bink_probe decode` PNGs).
#[derive(Resource, Clone, Copy)]
pub struct HoldFrame(pub u64);

/// Our own Vfs for movies, opened on first use.
#[derive(Resource, Default)]
struct MovieVfs(Option<Arc<Vfs>>);

/// Test-harness runs (scripted clicks, screenshots, direct battle / campaign / model starts, single
/// movies): no start-up movies and no moving front-end background unless asked for, so captures repeat.
pub fn is_harness_run(args: &[String]) -> bool {
    const FLAGS: [&str; 9] = ["--screenshot", "--ui-click", "--play-movie", "--movie-hold", "--battle", "--battle-key", "--campaign", "--view-model", "--list-models"];
    args.iter().any(|a| FLAGS.contains(&a.as_str()))
}

/// Whether the start-up movies play: on by default, as in the original (it queues them once per start,
/// `0x00484D30`); off with `--no-intro` and in harness runs unless `--intro` is given.
pub fn intro_enabled(args: &[String]) -> bool {
    let has = |f: &str| args.iter().any(|a| a == f);
    has("--intro") || (!has("--no-intro") && !is_harness_run(args))
}

/// Registers movie playback.
pub struct VideoPlugin {
    /// `--intro`: play the start-up movies, then the front end (the app must start in
    /// [`GameMode::Intro`]).
    pub intro: bool,
    /// `--frontend-movie`: the front end's background movie.
    pub frontend_movie: bool,
    /// `--play-movie <name>`: play one movie full screen at start (test harness, e.g. with
    /// `--screenshot` to capture a frame).
    pub play: Option<String>,
    /// `--movie-hold <frame>`: see [`HoldFrame`].
    pub hold: Option<u64>,
}

impl VideoPlugin {
    pub fn from_args(args: &[String]) -> Self {
        let has = |f: &str| args.iter().any(|a| a == f);
        Self {
            intro: intro_enabled(args),
            frontend_movie: has("--frontend-movie") || (!has("--no-frontend-movie") && !is_harness_run(args)),
            play: args.iter().position(|a| a == "--play-movie").and_then(|i| args.get(i + 1)).cloned(),
            hold: args.iter().position(|a| a == "--movie-hold").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()),
        }
    }
}

impl Plugin for VideoPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "movie_yuv.wgsl");
        app.add_plugins(Material2dPlugin::<MovieYuvMaterial>::default())
            .add_message::<PlayMovie>()
            .add_message::<MovieFinished>()
            .add_message::<StopMovie>()
            .init_resource::<MovieVfs>()
            .init_resource::<ConversionLayers>()
            .add_systems(Update, (start_movies, stop_movies, skip_movies, advance_movies, fit_screens).chain());
        if self.intro {
            app.insert_resource(IntroQueue(INTRO_MOVIES.iter().map(|s| s.to_string()).collect()))
                .add_systems(OnEnter(GameMode::Intro), next_intro_movie)
                .add_systems(Update, intro_progress.after(advance_movies).run_if(in_state(GameMode::Intro)));
        }
        if let Some(h) = self.hold {
            app.insert_resource(HoldFrame(h));
        }
        if let Some(name) = self.play.clone() {
            app.add_systems(Startup, move |mut play: MessageWriter<PlayMovie>| {
                play.write(PlayMovie::cutscene(name.clone()));
            });
        }
        if self.frontend_movie {
            app.add_systems(OnEnter(GameMode::FrontEnd), start_frontend_movie)
                .add_systems(OnExit(GameMode::FrontEnd), stop_frontend_movie);
        }
    }
}

/// Resolves a movie name to a Vfs path.
fn movie_path(vfs: &Vfs, name: &str) -> String {
    let n = name.replace('/', "\\").to_ascii_lowercase();
    if vfs.contains(&n) {
        return n;
    }
    let under = format!("movies\\{n}");
    if vfs.contains(&under) { under } else { n }
}

/// An 8-bit plane texture (bilinear, like the original's plane samplers).
fn plane_image(w: u32, h: u32, fill: u8) -> Image {
    let mut img = Image::new_fill(
        Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[fill],
        TextureFormat::R8Unorm,
        RenderAssetUsages::default(),
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

/// The RGBA picture of a movie: a render target (sRGB, like the decoded colours).
fn movie_image(w: u32, h: u32) -> Image {
    let mut img = Image::new_target_texture(w.max(1), h.max(1), TextureFormat::Rgba8UnormSrgb, None);
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
    img
}

/// Opens a movie and starts decoding it. Returns the movie component (not yet spawned).
pub fn open_movie(vfs: &Arc<Vfs>, req: &PlayMovie, images: &mut Assets<Image>) -> Result<Movie, String> {
    let path = movie_path(vfs, &req.path);
    let track = req.track.or_else(|| Some(language_track(&ntw_formats::pack::effective_language(config::game_data_dir()))));
    let src = BinkSource::open(vfs.clone(), &path, track)?;
    Ok(movie_from_source(Box::new(src), path, req, images))
}

/// A movie from any [`MovieSource`] (tests use [`source::StubSource`]).
pub fn movie_from_source(src: Box<dyn MovieSource>, path: String, req: &PlayMovie, images: &mut Assets<Image>) -> Movie {
    let info = src.info().clone();
    let stream = MovieStream::start(src, req.looped, req.sound);
    let (ys, cs) = (info.y_size, info.c_size);
    Movie {
        path,
        image: images.add(movie_image(info.width, info.height)),
        planes: [images.add(plane_image(ys.0, ys.1, 0)), images.add(plane_image(cs.0, cs.1, 128)), images.add(plane_image(cs.0, cs.1, 128))],
        mode: req.mode,
        info,
        stream: Mutex::new(stream),
        start: None,
        pending: None,
        shown: None,
        sound: req.sound,
    }
}

fn movie_vfs(v: &mut MovieVfs) -> Option<Arc<Vfs>> {
    if v.0.is_none() {
        match Vfs::open_install(config::game_data_dir()) {
            Ok(vfs) => v.0 = Some(Arc::new(vfs)),
            Err(e) => warn!("Movies: the install could not be opened: {e}"),
        }
    }
    v.0.clone()
}

/// What spawning a movie needs besides `Commands`.
#[derive(bevy::ecs::system::SystemParam)]
pub struct MovieSpawner<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<MovieYuvMaterial>>,
    layers: ResMut<'w, ConversionLayers>,
}

/// Render layers for the per-movie conversion passes (cycled; a few movies at most play at once).
#[derive(Resource, Default)]
pub struct ConversionLayers(usize);

const FIRST_CONVERSION_LAYER: usize = 10;
const CONVERSION_LAYERS: usize = 16;

/// Spawns a movie entity, its conversion pass (an off-screen camera drawing the planes through
/// [`MovieYuvMaterial`] into [`Movie::image`]) and, full screen, its camera and picture.
fn spawn_movie(commands: &mut Commands, sp: &mut MovieSpawner, movie: Movie) -> Entity {
    let mode = movie.mode;
    let image = movie.image.clone();
    let (w, h) = (movie.info.width as f32, movie.info.height as f32);
    let (ys, cs) = (movie.info.y_size, movie.info.c_size);
    let material = MovieYuvMaterial {
        params: MovieYuvParams {
            view: Vec4::new(w, h, 2.0 / GAMMA, BRIGHTNESS / 1.2),
            sizes: Vec4::new(ys.0 as f32, ys.1 as f32, cs.0 as f32, cs.1 as f32),
        },
        y: movie.planes[0].clone(),
        cb: movie.planes[1].clone(),
        cr: movie.planes[2].clone(),
    };
    let e = commands.spawn(movie).id();
    let conv = RenderLayers::layer(FIRST_CONVERSION_LAYER + sp.layers.0 % CONVERSION_LAYERS);
    sp.layers.0 += 1;
    commands.spawn((
        Camera2d,
        // Before the cameras that show the picture, so they see this frame's conversion.
        Camera { order: -100, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(image.clone().into()),
        Tonemapping::None,
        DebandDither::Disabled,
        Msaa::Off,
        conv.clone(),
        MovieScreen(e),
    ));
    commands.spawn((
        Mesh2d(sp.meshes.add(Rectangle::new(w, h))),
        MeshMaterial2d(sp.materials.add(material)),
        Transform::default(),
        conv,
        MovieScreen(e),
    ));
    if let MovieMode::Fullscreen { .. } = mode {
        let layer = RenderLayers::layer(MOVIE_LAYER);
        commands.spawn((
            Camera2d,
            Camera { order: 1000, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
            layer.clone(),
            MovieScreen(e),
        ));
        // An opaque black backdrop under the picture: the camera's own clear does not hide the
        // cameras drawn before it (they are composited), so the letterbox bars are drawn.
        commands.spawn((Sprite::from_color(Color::BLACK, Vec2::ONE), Transform::from_xyz(0.0, 0.0, -1.0), layer.clone(), MovieScreen(e), Backdrop));
        commands.spawn((Sprite { image, ..default() }, Transform::default(), layer, MovieScreen(e)));
    }
    e
}

/// The black full-window sprite behind a full-screen movie.
#[derive(Component)]
struct Backdrop;

fn start_movies(
    mut commands: Commands,
    mut msgs: MessageReader<PlayMovie>,
    mut vfs: ResMut<MovieVfs>,
    mut images: ResMut<Assets<Image>>,
    mut spawner: MovieSpawner,
    mut finished: MessageWriter<MovieFinished>,
) {
    for req in msgs.read() {
        let opened = movie_vfs(&mut vfs).ok_or_else(|| "no install".to_string()).and_then(|v| open_movie(&v, req, &mut images));
        match opened {
            Ok(m) => {
                info!("movie: playing {}", m.path);
                spawn_movie(&mut commands, &mut spawner, m);
            }
            Err(e) => {
                warn!("movie {}: {e}", req.path);
                // Report it as finished so whoever waits on it moves on.
                finished.write(MovieFinished { path: req.path.clone(), skipped: true, entity: Entity::PLACEHOLDER });
            }
        }
    }
}

/// Ends a movie: stops decoding and sound, despawns it and its screen, reports it.
fn end_movie(commands: &mut Commands, e: Entity, m: &Movie, skipped: bool, screens: &Query<(Entity, &MovieScreen)>, finished: &mut MessageWriter<MovieFinished>) {
    m.stream.lock().unwrap_or_else(|p| p.into_inner()).stop();
    for (se, s) in screens {
        if s.0 == e {
            commands.entity(se).despawn();
        }
    }
    commands.entity(e).despawn();
    finished.write(MovieFinished { path: m.path.clone(), skipped, entity: e });
}

fn stop_movies(
    mut commands: Commands,
    mut msgs: MessageReader<StopMovie>,
    movies: Query<(Entity, &Movie)>,
    screens: Query<(Entity, &MovieScreen)>,
    mut finished: MessageWriter<MovieFinished>,
) {
    for s in msgs.read() {
        for (e, m) in &movies {
            if s.path.as_ref().is_none_or(|p| m.path.ends_with(&p.replace('/', "\\").to_ascii_lowercase())) {
                end_movie(&mut commands, e, m, true, &screens, &mut finished);
            }
        }
    }
}

/// Any key or mouse button skips skippable full-screen movies (PROVISIONAL rule).
fn skip_movies(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    movies: Query<(Entity, &Movie)>,
    screens: Query<(Entity, &MovieScreen)>,
    mut finished: MessageWriter<MovieFinished>,
) {
    if keys.get_just_pressed().next().is_none() && mouse.get_just_pressed().next().is_none() {
        return;
    }
    for (e, m) in &movies {
        if m.mode == (MovieMode::Fullscreen { skippable: true }) && m.shown.is_some() {
            end_movie(&mut commands, e, m, true, &screens, &mut finished);
        }
    }
}

/// Which frame should be on screen `t` seconds after frame 0 was shown.
pub fn frame_due(t: f64, fps_num: u32, fps_den: u32) -> u64 {
    if t <= 0.0 { 0 } else { (t * fps_num as f64 / fps_den.max(1) as f64).floor() as u64 }
}

/// Shows each movie's frame that is due, starting the clock (and the sound) with frame 0.
#[allow(clippy::too_many_arguments)] // Bevy system parameters
fn advance_movies(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut movies: Query<(Entity, &mut Movie)>,
    screens: Query<(Entity, &MovieScreen)>,
    mut images: ResMut<Assets<Image>>,
    mut audio: MessageWriter<PlayMovieAudio>,
    mut finished: MessageWriter<MovieFinished>,
    hold: Option<Res<HoldFrame>>,
) {
    let now = time.elapsed_secs_f64();
    for (e, mut m) in &mut movies {
        let m = &mut *m;
        let mut stream = m.stream.lock().unwrap_or_else(|p| p.into_inner());
        let (fps_num, fps_den) = (stream.info.fps_num, stream.info.fps_den);
        // The newest frame that is due; later ones wait in `pending`.
        let mut due = m.start.map_or(0, |s| frame_due(now - s, fps_num, fps_den));
        if let Some(h) = hold.as_ref() {
            due = due.min(h.0);
        }
        let mut best: Option<source::DecodedFrame> = None;
        let mut ended = false;
        loop {
            let f = match m.pending.take() {
                Some(f) => f,
                None => match stream.poll() {
                    Poll::Frame(f) => f,
                    Poll::Pending => break,
                    Poll::Ended => {
                        ended = true;
                        break;
                    }
                },
            };
            if f.index <= due || m.start.is_none() && best.is_none() {
                if let Some(old) = best.replace(f) {
                    stream.recycle(old.planes);
                }
                if m.start.is_none() {
                    break;
                }
            } else {
                m.pending = Some(f);
                break;
            }
        }
        if let Some(f) = best {
            if m.start.is_none() {
                m.start = Some(now);
                // The sound starts with the first picture.
                if let (true, Some(feed)) = (m.sound, stream.audio.clone()) {
                    audio.write(PlayMovieAudio { feed, movie: m.path.clone() });
                }
            }
            m.shown = Some(f.index);
            // Upload the three planes; the conversion pass turns them into the RGBA picture.
            let mut old = Planes::default();
            let Planes { y, u, v } = f.planes;
            for (handle, data, back) in [(&m.planes[0], y, &mut old.y), (&m.planes[1], u, &mut old.u), (&m.planes[2], v, &mut old.v)] {
                if let Some(mut img) = images.get_mut(handle) {
                    *back = img.data.replace(data).unwrap_or_default();
                }
            }
            stream.recycle(old);
        }
        // The end: everything decoded has been shown, and the last frame has had its time.
        let last_done = match (m.start, m.shown) {
            (Some(s), Some(n)) => now - s >= (n + 1) as f64 * stream.info.frame_secs(),
            _ => true,
        };
        if ended && m.pending.is_none() && last_done {
            if let Some(err) = stream.error() {
                warn!("movie {}: {err}", m.path);
            }
            drop(stream);
            end_movie(&mut commands, e, m, false, &screens, &mut finished);
        }
    }
}

/// The on-screen size of a full-screen movie in a `screen`-sized window (CONFIRMED rule of the queue
/// player `0x0048B5B0`): a wide movie (type 1) on a screen narrower than 1.4:1 is shown at
/// `(width, width × 0.5625)`; a 4:3 movie (type 0) on a screen wider than 1.4:1 at `(height × 4/3, height)`;
/// anything else fills the whole screen. The type comes from the queue entry in the exe; we take it
/// from the movie's own shape (wider than 1.4:1 = type 1: INFERRED, the intro entries are type 1 and
/// 16:9).
pub fn fullscreen_size(screen: Vec2, movie_aspect: f32) -> Vec2 {
    let screen_aspect = screen.x / screen.y.max(1.0);
    let wide_movie = movie_aspect > 1.4;
    if wide_movie && screen_aspect < 1.4 {
        Vec2::new(screen.x, screen.x * 0.5625)
    } else if !wide_movie && screen_aspect > 1.4 {
        Vec2::new(screen.y * (4.0 / 3.0), screen.y)
    } else {
        screen
    }
}

/// Letterboxes full-screen pictures to the window.
fn fit_screens(
    windows: Query<&Window, With<PrimaryWindow>>,
    movies: Query<&Movie>,
    images: Res<Assets<Image>>,
    mut screens: Query<(&MovieScreen, &mut Sprite, Has<Backdrop>)>,
) {
    let Ok(win) = windows.single() else { return };
    let (ww, wh) = (win.width(), win.height());
    for (s, mut sprite, backdrop) in &mut screens {
        let Ok(m) = movies.get(s.0) else { continue };
        let Some(img) = images.get(&m.image) else { continue };
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let size = if backdrop { Vec2::new(ww, wh) } else { fullscreen_size(Vec2::new(ww, wh), iw / ih) };
        if sprite.custom_size != Some(size) {
            sprite.custom_size = Some(size);
        }
    }
}

fn next_intro_movie(mut queue: ResMut<IntroQueue>, mut play: MessageWriter<PlayMovie>, mut next: ResMut<NextState<GameMode>>) {
    if queue.0.is_empty() {
        next.set(GameMode::FrontEnd);
        return;
    }
    let path = queue.0.remove(0);
    play.write(PlayMovie::cutscene(path));
}

fn intro_progress(
    mut done: MessageReader<MovieFinished>,
    queue: ResMut<IntroQueue>,
    play: MessageWriter<PlayMovie>,
    next: ResMut<NextState<GameMode>>,
) {
    if done.read().count() > 0 {
        next_intro_movie(queue, play, next);
    }
}

fn start_frontend_movie(mut commands: Commands, mut vfs: ResMut<MovieVfs>, mut images: ResMut<Assets<Image>>, mut spawner: MovieSpawner) {
    let req = PlayMovie { path: FRONTEND_MOVIE.into(), mode: MovieMode::Texture, track: None, looped: true, sound: false };
    let Some(v) = movie_vfs(&mut vfs) else { return };
    match open_movie(&v, &req, &mut images) {
        Ok(m) => {
            commands.insert_resource(FrontEndMovieTexture(m.image.clone()));
            let e = spawn_movie(&mut commands, &mut spawner, m);
            commands.entity(e).insert(FrontEndMovie);
        }
        Err(e) => warn!("front-end movie: {e}"),
    }
}

#[derive(Component)]
struct FrontEndMovie;

fn stop_frontend_movie(
    mut commands: Commands,
    movies: Query<(Entity, &Movie), With<FrontEndMovie>>,
    screens: Query<(Entity, &MovieScreen)>,
    mut finished: MessageWriter<MovieFinished>,
) {
    for (e, m) in &movies {
        end_movie(&mut commands, e, m, true, &screens, &mut finished);
    }
    commands.remove_resource::<FrontEndMovieTexture>();
}

#[cfg(test)]
mod tests {
    use super::source::StubSource;
    use super::*;

    #[test]
    fn fullscreen_sizes_follow_the_exe() {
        // 16:9 movie on 4:3: letterboxed to width × 9/16.
        assert_eq!(fullscreen_size(Vec2::new(1280.0, 960.0), 16.0 / 9.0), Vec2::new(1280.0, 720.0));
        // 16:9 movie on 16:10: fills the screen (stretched, as the original).
        assert_eq!(fullscreen_size(Vec2::new(1680.0, 1050.0), 16.0 / 9.0), Vec2::new(1680.0, 1050.0));
        // 4:3 movie on 16:9: pillarboxed to height × 4/3.
        assert_eq!(fullscreen_size(Vec2::new(1920.0, 1080.0), 4.0 / 3.0), Vec2::new(1440.0, 1080.0));
        let none: Vec<String> = Vec::new();
        assert!(intro_enabled(&none));
        assert!(!intro_enabled(&["--screenshot".into(), "x.png".into()]));
        assert!(intro_enabled(&["--screenshot".into(), "x.png".into(), "--intro".into()]));
        assert!(!intro_enabled(&["--no-intro".into()]));
    }

    #[test]
    fn language_tracks_follow_the_exe_table() {
        assert_eq!(language_track("EN"), 0);
        assert_eq!(language_track("fr"), 1);
        assert_eq!(language_track("PO"), 6);
        assert_eq!(language_track("CZ"), 0, "Czech plays the English track");
        assert_eq!(language_track("xx"), 0);
    }

    #[test]
    fn frames_fall_due_at_the_frame_rate() {
        assert_eq!(frame_due(0.0, 30, 1), 0);
        assert_eq!(frame_due(0.0333, 30, 1), 0);
        assert_eq!(frame_due(0.0334, 30, 1), 1);
        assert_eq!(frame_due(1.0, 30, 1), 30);
        assert_eq!(frame_due(129.0, 30, 1), 3870);
        assert_eq!(frame_due(1.0, 30000, 1001), 29);
    }

    /// The player in a minimal app: frames reach the texture in order, and the movie ends with
    /// a `MovieFinished`.
    #[test]
    fn plays_a_stub_movie_to_the_end() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Image>()
            .add_message::<PlayMovieAudio>()
            .add_message::<MovieFinished>()
            .add_systems(Update, advance_movies);
        let req = PlayMovie { path: "stub".into(), mode: MovieMode::Texture, track: None, looped: false, sound: true };
        let movie = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            movie_from_source(Box::new(StubSource::new(8, 4, 5, Some((44_100, 2)))), "stub".into(), &req, &mut images)
        };
        let luma = movie.planes[0].clone();
        app.world_mut().spawn(movie);
        let mut seen = Vec::new();
        let mut audio_started = 0;
        for _ in 0..2000 {
            app.update();
            if let Some(m) = app.world_mut().query::<&Movie>().iter(app.world()).next()
                && let Some(f) = m.frame()
                && seen.last() != Some(&f)
            {
                seen.push(f);
                let y = &app.world().resource::<Assets<Image>>().get(&luma).unwrap().data.as_ref().unwrap()[..8];
                assert_eq!(y[1], StubSource::luma(f as u32), "frame {f} in the Y texture");
                assert_eq!(y[(f * 4 % 8) as usize], 235, "frame {f} bar");
            }
            audio_started += app.world_mut().resource_mut::<Messages<PlayMovieAudio>>().drain().count();
            let done: Vec<MovieFinished> = app.world_mut().resource_mut::<Messages<MovieFinished>>().drain().collect();
            if let Some(d) = done.first() {
                assert!(!d.skipped);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(audio_started, 1, "the sound starts once, with frame 0");
        assert_eq!(seen.first(), Some(&0));
        assert_eq!(seen.last(), Some(&4));
        assert!(seen.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(app.world_mut().query::<&Movie>().iter(app.world()).count(), 0, "despawned at the end");
    }
}
