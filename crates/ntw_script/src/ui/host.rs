//! The UI script host: runs the original UI `.luac` scripts against a live [`UiWorld`].
//!
//! What the engine does for UI scripts, as recovered from the scripts' bytecode and the exe
//! (evidence and tags in `analysis/frontend/UI_SCRIPTING.md`):
//! - every component has its **own global environment**; its global `Address` is its own address
//!   and `Component.*` acts on it (CONFIRMED by usage: `UIComponent(Address)`,
//!   `Component.Adopt(x)`, `Component.CreateFromLayout(path, id, Address)`);
//! - a component's scripts are its inline `script` text and the file
//!   `<folder>/<layout>_scripts/<id>.lua` or `<folder>/<layout>.<id>.lua` (CONFIRMED file names;
//!   order INFERRED);
//! - `InitState` is called with `{ State = <state name> }` once the layout is built and whenever
//!   the component changes state (INFERRED from `template.fe_button_standard.lua`, which greys its
//!   text in state `inactive` and moves it 1 px in state `down`);
//! - mouse events call the function bound in the layout (`OnMouseLClickUp → OnLeftClickUp`) or a
//!   callback set with `SetEventCallback` (CONFIRMED names; dispatch INFERRED).

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, LightUserData, Lua, MultiValue, Table, Value, Variadic};
use ntw_formats::loc::Localisation;
use ntw_formats::ui_layout::UiLayout;

use super::world::{NodeId, PointerEvent, UiRect, UiWorld, set_changed};
use crate::host::load_chunk;
use crate::source::ScriptSource;

const PRELUDE: &str = include_str!("ui_prelude.lua");

/// Facts about the player's machine that `FrontEnd.*` functions report. The caller fills them in.
#[derive(Debug, Clone, Default)]
pub struct FrontEndFacts {
    /// `FrontEnd.CampaignSavesExist()`.
    pub campaign_saves_exist: bool,
    /// `FrontEnd.SpanishCampaignEnabled()` (the Peninsular Campaign DLC).
    pub spanish_campaign: bool,
    /// `FrontEnd.GameVersion()` text.
    pub game_version: String,
    /// The original game's user folder (`%APPDATA%\The Creative Assembly\Napoleon`). Read only:
    /// the load-game page lists its `save_games\`, and our preferences copy is seeded from its
    /// `scripts\preferences.script.txt` the first time.
    pub original_user_dir: Option<std::path::PathBuf>,
    /// NapoleonRust's own user folder: our preferences copy is written to
    /// `<user_dir>\scripts\preferences.script.txt`. `None` = keep changes in memory only.
    pub user_dir: Option<std::path::PathBuf>,
    /// Napoleon's campaign progress (`nap_unlock`, 1..=4). The original keeps it in the registry
    /// `HKCU\Software\The Creative Assembly\Napoleon`, default 1 (CONFIRMED `0x0047B750`).
    pub nap_unlock: u32,
}

/// Something the scripts asked the game to do that is outside the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum UiRequest {
    /// `FrontEnd.Quit()`.
    Quit,
    /// `FrontEnd.LoadCampaign(path)` / `ContinueCampaign()`: load this save.
    LoadCampaign(std::path::PathBuf),
    /// `FrontEnd.StartCampaign(...)`: start a new campaign.
    StartCampaign {
        /// Campaign key, e.g. `ita_napoleon`.
        campaign: String,
        /// Faction played.
        faction: String,
    },
    /// `FrontEnd.StartBattle(setup, players)`: fight a battle.
    StartBattle {
        /// `battles` record key, e.g. `NHB_Arcole`.
        battle: String,
        /// Terrain preset folder (lower case, e.g. `hb_arcole`), if found.
        map: Option<String>,
    },
    /// `FrontEnd.StartBattle(setup, players, prefs)` from the custom battle pages: fight on a map
    /// with the armies the players set up.
    StartCustomBattle {
        /// `battles` record key of the map, e.g. `NAP_MP_Amazon`.
        battle: String,
        /// Terrain preset folder (lower case), if found.
        map: Option<String>,
        /// The armies, one per player.
        armies: Vec<CustomArmy>,
    },
}

/// One player's army of a custom battle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CustomArmy {
    /// Alliance index (0 = the first alliance).
    pub alliance: u32,
    /// Army index within the alliance.
    pub army: u32,
    /// Faction key.
    pub faction: String,
    /// A human player (else the AI).
    pub human: bool,
    /// (unit key, experience) in the army's order.
    pub units: Vec<(String, u32)>,
}

pub(super) struct Inner {
    pub(super) world: RefCell<UiWorld>,
    pub(super) source: ScriptSource,
    pub(super) loc: Localisation,
    pub(super) facts: FrontEndFacts,
    /// Our preferences copy (see `FrontEndFacts::user_dir`).
    pub(super) prefs: RefCell<ntw_formats::preferences::Preferences>,
    screen: RefCell<(f32, f32)>,
    root: RefCell<Option<NodeId>>,
    log: RefCell<Vec<String>>,
    pub(super) requests: RefCell<Vec<UiRequest>>,
    /// `StealShortcutKey(key)` calls, oldest first: (component, key name).
    stolen_keys: RefCell<Vec<(NodeId, String)>>,
    /// The component that last called `StealInputFocus`.
    focus: RefCell<Option<NodeId>>,
    /// Components to destroy at the end of the frame (`DestroyChildren`).
    pending_destroy: RefCell<Vec<NodeId>>,
    /// Destroyed components whose scripts stay callable until the end of the frame (INFERRED:
    /// file_requesters.lua's RequesterEnded destroys the requester, then asks it for its
    /// `PathName`, CONFIRMED order in the bytecode).
    dead_envs: RefCell<Vec<NodeId>>,
    /// Layouts adopted directly under the top root are sized to the screen (front-end pages).
    pages_fill_screen: std::cell::Cell<bool>,
    /// The scripts see the root at its layout size and place components in that frame (the
    /// campaign HUD; CONFIRMED at 1920x1080 by the debugger sitting of 2026-10-07, see
    /// `UiWorld::script_rect`; other screen sizes, above all ones smaller than 1280x960, are
    /// INFERRED to follow the same rule, not read).
    script_frame: std::cell::Cell<bool>,
    /// Fonts loaded for text measuring (`SetStateText`).
    fonts: RefCell<std::collections::HashMap<String, Option<ntw_formats::font::CufFont>>>,
    /// The UIEd template library (`data/UI/Templates/uied.templates`), read on first use.
    templates: RefCell<Option<Option<Rc<ntw_formats::ui_templates::UiTemplateLibrary>>>>,
    /// A campaign HUD host (`install_campaign` ran): engine behaviours the front-end pages do not
    /// rely on yet are switched on only here (see `GetStateText`).
    pub(super) campaign_mode: std::cell::Cell<bool>,
    /// How many times the tree was laid out (`relayout`); tests check the layout rule with it.
    layouts: std::cell::Cell<u64>,
    /// The screen size or the page rule changed since the last layout (changes inside the tree
    /// mark [`UiWorld::layout_dirty`] instead); see [`lay_out_if_stale`].
    layout_stale: std::cell::Cell<bool>,
    /// A layout deferred by a held `world()` borrow was logged (logged once per host).
    layout_deferred_logged: std::cell::Cell<bool>,
    /// The time (whole ms) [`UiScriptHost::pulse`] last handed to `OnUpdatePulse`. Only the
    /// campaign's `CampaignUI.Time` reads it, so its start times and pulse times share one base.
    pub(super) ui_time_ms: std::cell::Cell<f64>,
}

impl Inner {
    /// The screen size the layout is laid out for.
    pub(super) fn screen(&self) -> (f32, f32) {
        *self.screen.borrow()
    }

    /// The top component the loaded layout created (`load_root_layout`), whose script environment
    /// holds the root layout's globals. Read by the campaign bindings that have to reach a root
    /// global the way the shipped scripts do (`root:LuaCall(name, ...)`, see
    /// `agent_options.lua:109`).
    pub(super) fn root_node(&self) -> Option<NodeId> {
        *self.root.borrow()
    }
}

/// Runs UI scripts. Single-threaded (Lua is), like the original.
pub struct UiScriptHost {
    /// The campaign HUD's state, once [`install_campaign`](Self::install_campaign) ran.
    pub(super) campaign: RefCell<Option<Rc<super::campaign::CampaignUi>>>,
    lua: Lua,
    inner: Rc<Inner>,
}

pub(super) fn addr(id: NodeId) -> Value {
    Value::LightUserData(LightUserData((id + 1) as *mut std::ffi::c_void))
}

pub(super) fn node_of(v: &Value) -> Option<NodeId> {
    match v {
        Value::LightUserData(p) => (p.0 as usize).checked_sub(1),
        Value::Table(t) => t.raw_get::<Value>("__addr").ok().as_ref().and_then(node_of),
        _ => None,
    }
}

impl UiScriptHost {
    /// Creates the Lua state with the engine's UI functions.
    pub fn new(source: ScriptSource, loc: Localisation, facts: FrontEndFacts, screen: (f32, f32)) -> mlua::Result<Self> {
        let lua = Lua::new();
        let prefs = super::frontend::load_preferences(facts.user_dir.as_deref(), facts.original_user_dir.as_deref());
        let inner = Rc::new(Inner {
            world: RefCell::new(UiWorld::new()),
            source,
            loc,
            facts,
            prefs: RefCell::new(prefs),
            screen: RefCell::new(screen),
            root: RefCell::new(None),
            log: RefCell::new(Vec::new()),
            requests: RefCell::new(Vec::new()),
            stolen_keys: RefCell::new(Vec::new()),
            focus: RefCell::new(None),
            fonts: RefCell::new(std::collections::HashMap::new()),
            templates: RefCell::new(None),
            campaign_mode: std::cell::Cell::new(false),
            pending_destroy: RefCell::new(Vec::new()),
            dead_envs: RefCell::new(Vec::new()),
            pages_fill_screen: std::cell::Cell::new(true),
            script_frame: std::cell::Cell::new(false),
            layouts: std::cell::Cell::new(0),
            layout_stale: std::cell::Cell::new(false),
            layout_deferred_logged: std::cell::Cell::new(false),
            ui_time_ms: std::cell::Cell::new(0.0),
        });
        let host = Self { lua, inner, campaign: RefCell::new(None) };
        host.install()?;
        Ok(host)
    }

    /// The live component tree, laid out ([`lay_out_if_stale`]). Also for reads that need no
    /// geometry (texts, ids): a stale tree is laid out once per change whoever reads first, and the
    /// frame's render needs that layout anyway, so one accessor costs nothing extra.
    pub fn world(&self) -> std::cell::Ref<'_, UiWorld> {
        lay_out_if_stale(&self.inner);
        self.inner.world.borrow()
    }

    /// A function that reads how many layouts the host has done so far (tests of the layout rule).
    #[cfg(test)]
    pub(super) fn layout_counter(&self) -> impl Fn() -> u64 + 'static {
        let inner = self.inner.clone();
        move || inner.layouts.get()
    }

    /// The script environment (globals) of component `id`, if its scripts have one.
    pub fn script_env(&self, id: NodeId) -> Option<Table> {
        component_script_env(&self.lua, id)
    }

    /// The raw Lua state, for engine tables a game mode adds (e.g. the campaign HUD's
    /// `CampaignUI.*` functions) and for calling layout globals.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub(super) fn inner(&self) -> &Rc<Inner> {
        &self.inner
    }

    /// The top root (the first layout loaded with [`load_root_layout`](Self::load_root_layout)).
    pub fn root(&self) -> Option<NodeId> {
        *self.inner.root.borrow()
    }

    /// Script log lines (stubs called, `out.*` output, errors), oldest first; drained.
    pub fn take_log(&self) -> Vec<String> {
        std::mem::take(&mut *self.inner.log.borrow_mut())
    }

    /// Requests from the scripts (quit, ...); drained.
    pub fn take_requests(&self) -> Vec<UiRequest> {
        std::mem::take(&mut *self.inner.requests.borrow_mut())
    }

    /// Loads a layout file as the top root, sized to the screen, and runs its scripts.
    pub fn load_root_layout(&self, path: &str) -> Result<NodeId, String> {
        let id = create_layout(&self.lua, &self.inner, path, None, None)?;
        *self.inner.root.borrow_mut() = Some(id);
        init_new(&self.lua, &self.inner, id);
        Ok(id)
    }

    /// The window changed size.
    /// Whether layouts adopted directly under the top root are sized to the screen (default true:
    /// front-end pages centre their docked panels that way). The battle HUD turns it off: its
    /// panels (deployment, victory options, results) dock themselves inside the root.
    pub fn set_pages_fill_screen(&self, on: bool) {
        self.inner.pages_fill_screen.set(on);
        self.inner.layout_stale.set(true);
    }

    /// Whether the scripts work in the root's layout-size frame (`UiWorld::script_rect`): on for
    /// the campaign HUD (CONFIRMED there); off elsewhere (the battle HUD is not traced, BACKLOG §0).
    pub fn set_script_frame(&self, on: bool) {
        self.inner.script_frame.set(on);
    }

    pub fn set_screen(&self, w: f32, h: f32) {
        *self.inner.screen.borrow_mut() = (w, h);
        self.inner.layout_stale.set(true);
    }

    /// The top-most component under the point that reacts to the mouse.
    pub fn hit(&self, x: f32, y: f32) -> Option<NodeId> {
        let root = self.root()?;
        lay_out_if_stale(&self.inner);
        self.inner.world.borrow().hit(root, x, y)
    }

    /// A pointer event on a component: fires the matching Lua event (`OnMouseLClickDown` /
    /// `OnMouseLClickUp` / `OnMouseOn` / `OnMouseOff`; none for a release elsewhere), then applies
    /// its state's transition map, running the states' exit / `InitState` / enter functions if the
    /// state changed (see [`state_transitioned`]).
    ///
    /// Event first, transition second: CONFIRMED for all five in the exe's handlers (press
    /// `0x0102E1F0`, click `0x0102E340`, mouse on `0x0102EBC0`, mouse off `0x0102EB40`; the release
    /// elsewhere is `0x0102E340`'s other branch, transition only). A state with the +0x115 flag gets
    /// no press or click event, but its transition still applies (CONFIRMED, same handlers); that
    /// flag is our `disabled` (INFERRED mapping).
    pub fn pointer(&self, id: NodeId, event: PointerEvent) {
        let disabled = self.inner.world.borrow().get(id).and_then(|n| n.current()).is_some_and(|s| s.disabled);
        let click = matches!(event, PointerEvent::LeftDown | PointerEvent::LeftUp);
        let name = match event {
            PointerEvent::Enter => Some("OnMouseOn"),
            PointerEvent::Leave => Some("OnMouseOff"),
            PointerEvent::LeftDown => Some("OnMouseLClickDown"),
            PointerEvent::LeftUp => Some("OnMouseLClickUp"),
            PointerEvent::LeftUpElsewhere => None,
        };
        if let Some(name) = name.filter(|_| !(disabled && click)) {
            let mut args = MultiValue::new();
            if click {
                if matches!(event, PointerEvent::LeftUp) {
                    // A click gives the focus to a component that takes it and clears it
                    // otherwise, before the click event (CONFIRMED order in `0x0102E340`). Whether
                    // it takes it is read from the state the click starts from: the focus setter
                    // `0x010367F0` asks `0x01029FB0`, which reads the current state's +0xD4, and
                    // the transition comes after (CONFIRMED).
                    set_focus(&self.lua, &self.inner, Some(id), false);
                }
                // Mouse clicks pass the pointer position inside the component (INFERRED:
                // template.text_input.lua's OnSelect(x) places the caret from it).
                let g = self.lua.globals();
                let (cx, cy) = (g.get::<Option<f64>>("__ntw_cursor_x").ok().flatten(), g.get::<Option<f64>>("__ntw_cursor_y").ok().flatten());
                if let (Some(cx), Some(cy)) = (cx, cy)
                    && let Some(r) = rect_of(&self.inner, id)
                {
                    args.push_back(Value::Number(cx - f64::from(r.x)));
                    args.push_back(Value::Number(cy - f64::from(r.y)));
                }
            }
            fire_event(&self.lua, &self.inner, id, name, args);
        }
        // The state the event's script left the component in is the one the transition starts
        // from (the exe reads the current state inside `0x01035620`, after the event).
        let left = self.inner.world.borrow_mut().transition(id, event);
        if let Some(old) = left {
            state_transitioned(&self.lua, &self.inner, id, old);
        }
    }

    /// Typed text: each character goes to the focused component's `CharacterInput(c)` when its
    /// state takes the focus (CONFIRMED dispatch `0x0102DD50`; the global's name CONFIRMED). The
    /// characters reach the script as single Latin-1 bytes; others are dropped (PROVISIONAL: no
    /// IME, ASCII and Latin-1 only). Returns true if a component took them.
    pub fn text_input(&self, text: &str) -> bool {
        let Some(id) = *self.inner.focus.borrow() else { return false };
        if !accepts_focus(&self.inner, id) {
            return false;
        }
        for c in text.chars().filter(|c| (*c as u32) < 0x100 && !c.is_control()) {
            let r = (|| -> mlua::Result<bool> {
                let f: Function = self.lua.globals().get("__ntw_call_if_defined")?;
                f.call::<bool>((addr(id), "CharacterInput", self.lua.create_string([c as u8])?))
            })();
            if let Err(e) = r {
                log(&self.inner, format!("ERROR in CharacterInput of component {id}: {e}"));
            }
        }
        true
    }

    /// A key was pressed and released (`key` is the engine's key name, e.g. `"ESCAPE"`,
    /// `"RETURN"`). It goes to the component that most recently stole that key with
    /// `StealShortcutKey` and is still shown (attached under the top root and visible), whose
    /// `OnKey(key, false, true)` runs. INFERRED: root.lua steals ESCAPE and its `OnKey` calls
    /// `TransitionBack` only when the third argument is true; sub-panels such as the options
    /// gamma panel steal ESCAPE/RETURN later, while they are open. Returns true if delivered.
    ///
    /// A focused component that takes the focus hears the key first, through its layout's `OnKey`
    /// binding, once pressed (`OnKey(key, false, false)`) and once released (`(key, false, true)`)
    /// (INFERRED from template.text_input.lua: it edits on the `false` call and lets go of the
    /// focus on RETURN / ESCAPE on the `true` one); the shortcut holder still gets it afterwards.
    pub fn key(&self, key: &str) -> bool {
        let focused = (*self.inner.focus.borrow()).filter(|&f| accepts_focus(&self.inner, f));
        if let Some(f) = focused {
            for up in [false, true] {
                let args: MultiValue = mlua::IntoLuaMulti::into_lua_multi((key, false, up), &self.lua).unwrap_or_default();
                fire_event(&self.lua, &self.inner, f, "OnKey", args);
            }
        }
        let target = {
            let w = self.inner.world.borrow();
            let root = *self.inner.root.borrow();
            let shown = |mut id: NodeId| loop {
                let Some(n) = w.get(id) else { return false };
                if !n.visible {
                    return false;
                }
                match n.parent {
                    Some(p) => id = p,
                    None => return Some(id) == root,
                }
            };
            self.inner.stolen_keys.borrow().iter().rev().find(|(n, k)| k == key && shown(*n)).map(|(n, _)| *n)
        };
        let Some(id) = target else { return focused.is_some() };
        let r = (|| -> mlua::Result<bool> {
            let f: Function = self.lua.globals().get("__ntw_call_if_defined")?;
            f.call::<bool>((addr(id), "OnKey", key, false, true))
        })();
        r.unwrap_or_else(|e| {
            log(&self.inner, format!("ERROR in OnKey of component {id}: {e}"));
            false
        })
    }

    /// One UI frame: fires `OnUpdatePulse(time_ms)` on every shown component that listens for it
    /// (bound in its layout or with `SetEventCallback`). The argument is the time in milliseconds
    /// (template.heading.lua slides a heading in over `(t - t0) / 750`). CONFIRMED in the exe: the
    /// root's per-frame update `0x0102F4F0` (UI root vtable +0x30) takes a u32 time, keeps it in
    /// `0x0176591C` and hands it, as a float, to the root's `OnUpdatePulse` handler (event 11).
    ///
    /// `time_ms` is also the clock `CampaignUI.Time()` reads (seconds = `time_ms / 1000`). CONFIRMED
    /// in the exe: the campaign UI's frame (`0x009D5F08`..`0x009D5F31`) calls the very getter
    /// `CampaignUI.Time` calls (manager `+0x18` part, vtable +0x30 = `0x00A0EB80`: a u32 ms counter
    /// × 0.001), multiplies by 1000, truncates to int and passes that to the root's update. The
    /// shipped scripts rely on it (template.BuildingFrame.lua's OnUpdate turns its argument into
    /// seconds, `t * 0.001`, and measures it against start times taken from `CampaignUI.Time()`):
    /// a second clock lets the two drift apart and delays every scripted transition by the gap.
    ///
    /// The time is kept in whole milliseconds (floored here, for every host): the exe's root update
    /// takes a u32, and the campaign frame truncates `(int)(Time() * 1000)`.
    pub fn pulse(&self, time_ms: f64) {
        let time_ms = time_ms.floor();
        self.inner.ui_time_ms.set(time_ms);
        // End of the previous frame: the destroyed components' scripts go, and what
        // DestroyChildren detached is destroyed.
        let dead = std::mem::take(&mut *self.inner.dead_envs.borrow_mut());
        if let Ok(envs) = self.lua.globals().get::<Table>("__ntw_envs") {
            for g in dead {
                let _ = envs.raw_set(addr(g), Value::Nil);
            }
        }
        let doomed = std::mem::take(&mut *self.inner.pending_destroy.borrow_mut());
        for d in doomed {
            if self.inner.world.borrow().get(d).is_some_and(|n| n.parent.is_none()) {
                let _ = destroy(&self.lua, &self.inner, d);
            }
        }
        let listeners: Vec<(NodeId, Option<String>)> = {
            let w = self.inner.world.borrow();
            let Some(root) = self.root() else { return };
            let mut out = Vec::new();
            w.visit_visible(root, &mut |id, n| {
                let bound = n.data.event("OnUpdatePulse").map(str::to_owned);
                if bound.is_some() || n.script_events.iter().any(|e| e == "OnUpdatePulse") {
                    out.push((id, bound));
                }
            });
            out
        };
        for (id, func) in listeners {
            let r = (|| -> mlua::Result<()> {
                let fire: Function = self.lua.globals().get("__ntw_fire")?;
                fire.call::<bool>((addr(id), "OnUpdatePulse", func, time_ms))?;
                Ok(())
            })();
            if let Err(e) = r {
                log(&self.inner, format!("ERROR in OnUpdatePulse of component {id}: {e}"));
            }
        }
    }

    /// The cursor the scripts asked for with `Cursor(name)` (e.g. `"busy"` during a page
    /// change), or `None` after `SetMode("normal")`.
    pub fn cursor(&self) -> Option<String> {
        self.lua.globals().get::<Option<String>>("__ntw_cursor").ok().flatten()
    }

    /// The component that holds the input focus (`StealInputFocus`), if any.
    pub fn focus(&self) -> Option<NodeId> {
        *self.inner.focus.borrow()
    }

    /// Fires a layout event on a component (e.g. `"OnMouseLClickUp"`).
    pub fn fire(&self, id: NodeId, event: &str) {
        let func = self.inner.world.borrow().get(id).and_then(|n| n.data.event(event).map(str::to_owned));
        let r = (|| -> mlua::Result<()> {
            let fire: Function = self.lua.globals().get("__ntw_fire")?;
            fire.call::<bool>((addr(id), event, func))?;
            Ok(())
        })();
        if let Err(e) = r {
            log(&self.inner, format!("ERROR in {event} of component {id}: {e}"));
        }
    }

    fn install(&self) -> mlua::Result<()> {
        let lua = &self.lua;
        let g = lua.globals();
        let inner = self.inner.clone();
        g.set("__ntw_log", lua.create_function(move |_, text: String| {
            log(&inner, text);
            Ok(())
        })?)?;
        let inner = self.inner.clone();
        g.set("__ntw_loadfile", lua.create_function(move |lua, path: String| {
            match inner.source.find(&path) {
                Some(f) => Ok((Some(load_chunk(lua, &f.bytes, &f.chunk_name)?), None)),
                None => Ok((None, Some(format!("no file '{path}' in the install")))),
            }
        })?)?;
        g.set("__ui", component_methods(lua, &self.inner)?)?;
        g.set("__comp", component_functions(lua, &self.inner)?)?;
        g.set("__frontend", frontend_functions(lua, &self.inner)?)?;
        g.set("__ntw_image", super::image::image_functions(lua, &self.inner)?)?;
        // The `bit` library the engine registers for every Lua state (template.CampaignUnitCard.lua
        // uses bit.band; the same functions as the campaign host's, see `crate::bit`).
        g.set("bit", crate::bit::create(lua)?)?;
        let inner = self.inner.clone();
        g.set("__ntw_screen_size", lua.create_function(move |_, ()| Ok(inner.screen()))?)?;
        install_require(lua, &self.inner)?;
        lua.load(PRELUDE).set_name("@ntw_ui_prelude.lua").exec()?;
        Ok(())
    }
}

pub(super) fn log(inner: &Inner, text: String) {
    inner.log.borrow_mut().push(text);
}

/// The UIEd template library, read from the install on first use (a loose file; CONFIRMED path
/// `data/UI/templates/uied.templates`, see `ntw_formats::ui_templates`).
pub(super) fn template_library(inner: &Inner) -> Option<Rc<ntw_formats::ui_templates::UiTemplateLibrary>> {
    let mut slot = inner.templates.borrow_mut();
    slot.get_or_insert_with(|| {
        let file = inner.source.find("ui/templates/uied.templates")?;
        match ntw_formats::ui_templates::UiTemplateLibrary::read(&file.bytes) {
            Ok(lib) => Some(Rc::new(lib)),
            Err(e) => {
                inner.log.borrow_mut().push(format!("ERROR reading the template library: {e}"));
                None
            }
        }
    })
    .clone()
}

impl UiScriptHost {
    /// Every image embedded in the UIEd template library, as (path stored in the components,
    /// TGA bytes), for the renderer: template components refer to the artists' original files
    /// (e.g. `C:\Documents and Settings\...\tab_selected.tga`), which only exist there.
    pub fn template_images(&self) -> Vec<(String, Vec<u8>)> {
        template_library(&self.inner)
            .map(|lib| lib.templates.iter().flat_map(|t| t.images.iter().cloned()).collect())
            .unwrap_or_default()
    }
}

/// Every node in a subtree, pre-order.
fn collect_subtree(inner: &Inner, id: NodeId) -> Vec<NodeId> {
    let w = inner.world.borrow();
    let mut out = Vec::new();
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        if let Some(node) = w.get(n) {
            out.push(n);
            stack.extend(node.children.iter().rev());
        }
    }
    out
}

/// **Layout rule** (the one place it is written down): geometry is current at every read, as in
/// the exe, which has no layout pass: Width / Height read the current state's size (`0x01037880`),
/// a position is computed on each read from the parent chain (`0x0102FEA0`), and Resize re-docks
/// the children inside the call (`0x010324E0`) (CONFIRMED, UI_FIDELITY.md "Round 15 trace"). Ours
/// is lazy and observably the same: a change the layout depends on (an offset, a size, docking, a
/// state, the tree's shape: [`UiWorld::layout_dirty`]; the screen size or the page rule:
/// `Inner::layout_stale`) marks it stale, and every geometry read (the `UIComponent` size and
/// position methods, MoveTo, Resize, SetStateText, InitState's text measurement, a click's
/// position, hit tests, the credits builder, [`UiScriptHost::world`], which the renderer reads)
/// calls this first, which lays the tree out only if it is stale. Texts, colours, images and
/// visibility do not mark it ([`UiWorld::update_appearance`]). Nothing else lays out, so any
/// number of changes between two reads cost one layout and a read after none costs nothing.
///
/// While a [`UiScriptHost::world`] borrow is held nothing in the tree can change (RefCell), so
/// only a screen or page-rule change can be pending then; it is laid out at the first read after
/// the borrow is dropped instead of panicking on a second borrow. Known gap (PROVISIONAL): our
/// layout re-docks every child, the exe only a resized parent's.
pub(super) fn lay_out_if_stale(inner: &Inner) {
    if !inner.layout_stale.get() && !inner.world.borrow().layout_dirty {
        return;
    }
    let Ok(mut world) = inner.world.try_borrow_mut() else {
        // Only while a `world()` borrow is held (see above): this read sees the last layout.
        if !inner.layout_deferred_logged.replace(true) {
            log(inner, "UI layout deferred: the component tree is borrowed; laid out at the next read (logged once)".to_owned());
        }
        return;
    };
    relayout(inner, &mut world);
}

/// Lays out the whole tree for the current screen (only through [`lay_out_if_stale`]). The top
/// root fills the screen (the engine sizes its root to the screen, INFERRED); layouts adopted
/// directly under it are sized to the screen too (PROVISIONAL, so pages such as `main` centre
/// their docked panels).
fn relayout(inner: &Inner, world: &mut UiWorld) {
    let Some(root) = *inner.root.borrow() else {
        // Nothing to lay out. Clearing the flags is safe: a root only comes from `instantiate`,
        // which marks the tree dirty again, and the screen size is read at that layout.
        world.layout_dirty = false;
        inner.layout_stale.set(false);
        return;
    };
    inner.layouts.set(inner.layouts.get() + 1);
    let (w, h) = *inner.screen.borrow();
    let pages: Vec<NodeId> = if inner.pages_fill_screen.get() { world.get(root).map(|n| n.children.clone()).unwrap_or_default() } else { Vec::new() };
    for p in pages {
        // Template components (tooltips, cards) created under the root keep their own size.
        let is_page_root = world.get(p).is_some_and(|n| {
            !n.keep_size && !n.layout_file.starts_with("ui/templates/") && n.layout_file != world.get(root).map(|r| r.layout_file.clone()).unwrap_or_default()
        });
        if is_page_root && let Some(n) = world.get_mut(p) {
            n.size_override = Some((w, h));
        }
    }
    world.layout(root, UiRect { x: 0.0, y: 0.0, w, h });
    world.layout_dirty = false;
    inner.layout_stale.set(false);
}

/// Reads `path` (e.g. `data/ui/frontend ui/main`), builds its nodes under `parent` (renaming
/// the new top component to `id` if given) and runs every new component's scripts. `InitState`
/// is not called here; see [`init_new`].
///
/// For a script's `CreateFromLayout` (a `parent` is given), the file's root is an editor wrapper:
/// when it has exactly one child, that child is the component created and returned (INFERRED:
/// every front-end page file is `root` + one child named like the page; options.lua's
/// `Find(2)` must reach that child's button_ok, and root.lua calls `OnEnter` on what
/// CreateFromLayout returned, which the child's script defines). The engine's own load of the
/// front-end layout keeps the root (root.lua runs on it).
fn create_layout(lua: &Lua, inner: &Rc<Inner>, path: &str, id: Option<&str>, parent: Option<NodeId>) -> Result<NodeId, String> {
    let norm = path.replace('\\', "/").to_ascii_lowercase();
    let norm = norm.strip_prefix("data/").unwrap_or(&norm).to_owned();
    let file = inner.source.find(&norm).ok_or_else(|| format!("layout {norm} not found"))?;
    let layout = UiLayout::read(&file.bytes).map_err(|e| format!("{norm}: {e}"))?;
    let top = match (parent, layout.root.children.as_slice()) {
        (Some(_), [only]) => only,
        _ => &layout.root,
    };
    let mut created = Vec::new();
    let root = inner.world.borrow_mut().instantiate(top, parent, &norm, &mut created);
    // The first layout becomes the top root before its scripts run, so their main chunks see laid
    // out sizes (review_DY.lua reads its tab group's width at load to size the unit cards).
    if parent.is_none() && inner.root.borrow().is_none() {
        *inner.root.borrow_mut() = Some(root);
    }
    localise(inner, &created);
    // Scripts are found by the layout's own component id; the new id is given afterwards
    // (INFERRED: PanelManager opens `deployment_end` as "finish_deployment" and then LuaCalls
    // InitDeploymentPanel, defined by `deployment_end_scripts/deployment_end.luac`).
    for &c in &created {
        run_component_scripts(lua, inner, c, (c == root).then_some(top.id.as_str()));
    }
    if let Some(name) = id
        && let Some(n) = inner.world.borrow_mut().get_mut(root)
    {
        n.data.id = name.to_owned();
    }
    Ok(root)
}

/// A Lua string as text: UTF-8 when it is valid UTF-8, else Latin-1 bytes (typed characters
/// reach the scripts as single Latin-1 bytes, see [`UiScriptHost::text_input`]; INFERRED: the
/// original's Lua strings are narrow, code-page strings).
pub(super) fn lua_text(s: &mlua::LuaString) -> String {
    let b = s.as_bytes();
    match std::str::from_utf8(&b) {
        Ok(t) => t.to_owned(),
        Err(_) => b.iter().map(|&c| c as char).collect(),
    }
}

/// Fires a layout or script event on a component with arguments (as `UiScriptHost::fire`).
fn fire_event(lua: &Lua, inner: &Inner, id: NodeId, event: &str, args: MultiValue) {
    let func = inner.world.borrow().get(id).and_then(|n| n.data.event(event).map(str::to_owned));
    let r = (|| -> mlua::Result<()> {
        let fire: Function = lua.globals().get("__ntw_fire")?;
        let mut all = MultiValue::new();
        all.push_back(addr(id));
        all.push_back(Value::String(lua.create_string(event)?));
        all.push_back(match func {
            Some(f) => Value::String(lua.create_string(&f)?),
            None => Value::Nil,
        });
        all.extend(args);
        fire.call::<bool>(all)?;
        Ok(())
    })();
    if let Err(e) = r {
        log(inner, format!("ERROR in {event} of component {id}: {e}"));
    }
}

/// True if the component's current state takes the input focus (the state record's +0xD4,
/// our `unknown_d4`, CONFIRMED: the focus setter `0x010367F0` and the character dispatch
/// `0x0102DD50` test it; 2 on the text fields).
fn accepts_focus(inner: &Inner, id: NodeId) -> bool {
    inner.world.borrow().get(id).and_then(|n| n.current()).is_some_and(|s| s.unknown_d4 != 0)
}

/// Moves the input focus (`0x010367F0`, CONFIRMED): a component that does not take the focus
/// clears it unless `forced` (StealInputFocus(true)); the old holder hears `OnInputFocusLose`,
/// the new one `OnInputFocusGain` (CONFIRMED event names).
fn set_focus(lua: &Lua, inner: &Inner, new: Option<NodeId>, forced: bool) {
    let new = new.filter(|&n| forced || accepts_focus(inner, n));
    let old = *inner.focus.borrow();
    if old == new {
        return;
    }
    *inner.focus.borrow_mut() = None;
    if let Some(o) = old {
        fire_event(lua, inner, o, "OnInputFocusLose", MultiValue::new());
    }
    *inner.focus.borrow_mut() = new;
    if let Some(n) = new {
        fire_event(lua, inner, n, "OnInputFocusGain", MultiValue::new());
    }
}

/// A layout font (`"Frontend 22, Normal"`), loaded once from the install.
fn with_font<R>(inner: &Inner, name: &str, f: impl FnOnce(&ntw_formats::font::CufFont) -> R) -> Option<R> {
    let mut fonts = inner.fonts.borrow_mut();
    let font = fonts.entry(name.to_owned()).or_insert_with(|| {
        ntw_formats::font::font_file_for(name)
            .and_then(|p| inner.source.find(&p))
            .and_then(|file| ntw_formats::font::CufFont::read(&file.bytes).ok())
    });
    font.as_ref().map(f)
}

/// Width, height and line count of `text` wrapped at `max_width` in `font`.
pub(super) fn text_extent(inner: &Inner, font: &str, text: &str, max_width: i32) -> (f32, f32, usize) {
    with_font(inner, font, |f| {
        let lines = f.wrap_lines(text, max_width);
        let w = lines.iter().map(|l| f.text_width(l)).max().unwrap_or(0);
        let h = f.line_height().max(f.ascent()) * lines.len() as i32;
        (w as f32, h as f32, lines.len())
    })
    .unwrap_or((0.0, 0.0, 0))
}

/// The engine localises each state's text label at load (state reader 0x01021410, CONFIRMED).
fn localise(inner: &Inner, created: &[NodeId]) {
    let mut w = inner.world.borrow_mut();
    for &c in created {
        w.update_appearance(c, |n| {
            let mut changed = false;
            for s in &mut n.data.states {
                if s.text_localised
                    && !s.text_label.is_empty()
                    && let Some(t) = inner.loc.get(&s.text_label)
                {
                    changed |= set_changed(&mut s.text, t.to_owned());
                }
            }
            changed
        });
    }
}

/// Common tail of `CreateFromComponent` / `CreateComponentFromTemplate`: renames the new root,
/// places it at (x, y) inside its parent, sets the texts of named descendants and initialises the
/// new components ([`init_new`]).
fn finish_created(
    lua: &Lua,
    inner: &Rc<Inner>,
    new: NodeId,
    id: Option<&str>,
    pos: (Option<f32>, Option<f32>),
    texts: Option<Table>,
    images: &Value,
) -> mlua::Result<()> {
    {
        let mut w = inner.world.borrow_mut();
        if let Some(n) = w.get_mut(new) {
            if let Some(id) = id {
                n.data.id = id.to_owned();
            }
            if let (Some(x), Some(y)) = pos {
                n.offset = (x, y);
            }
        }
        if let Some(texts) = texts {
            for pair in texts.pairs::<String, Value>() {
                let (child, text) = pair?;
                let text = match text {
                    Value::String(s) => s.to_string_lossy(),
                    Value::Integer(i) => i.to_string(),
                    Value::Number(n) => n.to_string(),
                    _ => continue,
                };
                // Every state gets the text (INFERRED: Construction.lua passes a constructable's
                // cost as `building_cost` and then switches that component to its "red" state when
                // the faction cannot afford it; the original shows the cost in red).
                if let Some(c) = w.find(new, &child) {
                    w.update_appearance(c, |n| {
                        for s in &mut n.data.states {
                            s.text = text.clone();
                        }
                        true
                    });
                }
            }
        }
        // The 6th argument: image replacements "{<component id>:<n>}<path>" (CONFIRMED string
        // shape: Recruitment.lua passes {"{" .. card_id .. ":1}" .. image_path .. ".tga"} for the
        // card it creates; INFERRED: n is 1-based in the component's image list).
        if let Value::Table(list) = images {
            for v in list.sequence_values::<String>() {
                let Ok(s) = v else { continue };
                let Some((head, path)) = s.strip_prefix('{').and_then(|r| r.split_once('}')) else { continue };
                if path.is_empty() {
                    continue;
                }
                let Some((cid, n)) = head.rsplit_once(':') else { continue };
                let Ok(n) = n.trim().parse::<usize>() else { continue };
                let target = if w.get(new).is_some_and(|x| x.data.id == cid) { Some(new) } else { w.find(new, cid) };
                if let Some(t) = target {
                    w.update_appearance(t, |node| {
                        let Some(img) = n.checked_sub(1).and_then(|i| node.data.images.get_mut(i)) else { return false };
                        img.path = path.strip_prefix("data/").unwrap_or(path).to_owned();
                        node.painted = true;
                        true
                    });
                }
            }
        }
    }
    init_new(lua, inner, new);
    Ok(())
}

/// Initialises newly created components the way the exe's component initialiser `0x01034210`
/// does (CONFIRMED, disassembly): children first, each subtree in turn (post-order), then the
/// component itself, which, if it has a script environment, gets its current state's InitState
/// and enter function with an empty old-state name (`0x0102DF60` with an empty string built by
/// `0x004F0640`). Each goes through [`run_state_entry`], which does nothing for a component with
/// nothing to call (`0x01034210` asks `HasLiveUIScriptEnvironment` first; ours asks
/// `__ntw_has_handler`, which a host hooking `__ntw_call_if_defined` answers too).
///
/// Not modelled yet: `0x01034210` then fires the component's `OnCreate` event when one is bound
/// (only `unit_bling.luac` uses that name in the shipped scripts).
fn init_new(lua: &Lua, inner: &Rc<Inner>, root: NodeId) {
    for c in collect_subtree_post_order(inner, root) {
        if inner.world.borrow().get(c).is_none() {
            continue;
        }
        if let Err(e) = run_state_entry(lua, inner, c, None) {
            log(inner, format!("ERROR initialising component {c}: {e}"));
        }
    }
}

/// Every node in a subtree, children before their parent (post-order), siblings in order.
fn collect_subtree_post_order(inner: &Inner, id: NodeId) -> Vec<NodeId> {
    let w = inner.world.borrow();
    let mut out = Vec::new();
    let mut stack = vec![(id, false)];
    while let Some((n, children_done)) = stack.pop() {
        let Some(node) = w.get(n) else { continue };
        if children_done {
            out.push(n);
        } else {
            stack.push((n, true));
            stack.extend(node.children.iter().rev().map(|&c| (c, false)));
        }
    }
    out
}

/// Stack a state-function call must have left before it runs, else it continues on a fresh stack
/// segment of [`STATE_CALL_STACK_SEGMENT`] bytes.
const STATE_CALL_RED_ZONE: usize = 256 * 1024;
/// Size of each extra stack segment for state-function calls.
const STATE_CALL_STACK_SEGMENT: usize = 2 * 1024 * 1024;
/// Most extra native stack the state-function calls of one thread may take in all (see
/// [`state_function_guard`]).
const STATE_CALL_STACK_BUDGET: usize = 32 * 1024 * 1024;

thread_local! {
    /// Extra stack the state-function calls of this thread hold now (bytes).
    static STATE_CALL_STACK_GROWN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Runs `f`, one state-function call (an exit function, or a state's InitState and enter
/// function), protected against running out of native stack. Every state-function call goes
/// through here (via [`run_state_entry`] or [`state_transitioned`]), since they nest through many
/// paths: SetState from a state function, InitState creating components whose InitState sets a
/// state, and so on. The campaign review panel's tab change (a tab's state change in the exe,
/// `0x009C7D80`, made inside the script's request) goes through here too.
///
/// No nesting limit of ours: the original has none (`0x01035620`, `0x0102DF60`, `0x0102DFD0`,
/// CONFIRMED), so a script that keeps changing state from its state functions runs until it stops
/// by itself (a bounded A → B → A) or until Lua's own nested C-call limit raises "C stack
/// overflow" (Lua 5.1, as the original's scripts), which the caller logs. So that a small thread
/// stack (e.g. 2 MiB) cannot overflow first, a call that finds less than
/// [`STATE_CALL_RED_ZONE`] left continues on a fresh [`STATE_CALL_STACK_SEGMENT`]. Where the
/// thread's own stack cannot be measured (`remaining_stack()` is None: not on Windows, Linux or
/// macOS, `the_native_stack_is_measured_on_the_shipped_platforms`), the first call moves to a
/// segment, and the calls inside it measure that segment (stacker sets the limit of every segment
/// it makes, stacker 0.1.25 `_grow`), so it grows again only when that segment runs low: no
/// segment per call. What bounds the memory is a byte budget, not a depth: the segments one
/// thread holds at once stay within [`STATE_CALL_STACK_BUDGET`] (given back when a call ends,
/// also by a panic); past it the call fails with the same "C stack overflow" error Lua raises,
/// before any memory is taken. Lua's own limit ends a runaway script well before that, see
/// `a_state_function_that_re_enters_its_own_state_runs_until_luas_call_limit`.
pub(super) fn state_function_guard<R>(f: impl FnOnce() -> R) -> mlua::Result<R> {
    if stacker::remaining_stack().is_some_and(|left| left >= STATE_CALL_RED_ZONE) {
        return Ok(f());
    }
    let grown = STATE_CALL_STACK_GROWN.with(|g| g.get());
    if grown + STATE_CALL_STACK_SEGMENT > STATE_CALL_STACK_BUDGET {
        return Err(mlua::Error::runtime("C stack overflow (UI state functions nested past the host's native stack budget)"));
    }
    // Gives the segment back to the budget when the call ends, also when it unwinds (a panic).
    struct Restore(usize);
    impl Drop for Restore {
        fn drop(&mut self) {
            STATE_CALL_STACK_GROWN.with(|g| g.set(self.0));
        }
    }
    let _restore = Restore(grown);
    STATE_CALL_STACK_GROWN.with(|g| g.set(grown + STATE_CALL_STACK_SEGMENT));
    Ok(stacker::grow(STATE_CALL_STACK_SEGMENT, f))
}

/// The script environment (globals) of component `id`, if its scripts have one: the table
/// `__ntw_envs[address]` the component's scripts run in.
pub(super) fn component_script_env(lua: &Lua, id: NodeId) -> Option<Table> {
    let envs: Table = lua.globals().get("__ntw_envs").ok()?;
    envs.get::<Option<Table>>(addr(id)).ok().flatten()
}

/// Whether a call of `name` on component `id` through `__ntw_call_if_defined` would run a script,
/// as `__ntw_has_handler` answers it (ui_prelude.lua: the component's own function; a host that
/// hooks `__ntw_call_if_defined` to supply a function must answer for it there too). InitState's
/// setup (the state table, the text measurement) is skipped when this says no ([`call_init_state`]).
fn has_handler(lua: &Lua, id: NodeId, name: &str) -> bool {
    lua.globals()
        .get::<Function>("__ntw_has_handler")
        .and_then(|f| f.call::<bool>((addr(id), name)))
        .unwrap_or(false)
}

/// The component's id and the name of its state `state`, for error lines.
fn component_and_state_names(inner: &Inner, id: NodeId, state: usize) -> (String, String) {
    inner.world.borrow().get(id).map(|n| (n.data.id.clone(), n.data.states.get(state).map(|s| s.name.clone()).unwrap_or_default())).unwrap_or_default()
}

/// The enter function to run for component `id` entering its current state, with the name of
/// the state it came from (`old`; none = empty name): (state, function, old state's name), or
/// None when the state is disabled or has none (`0x0102DF60`; the flag is our `disabled`,
/// INFERRED). Read before the state's InitState runs, as `0x0102DF60` keeps its state pointer
/// across InitState (CONFIRMED).
fn enter_call(inner: &Inner, id: NodeId, old: Option<usize>) -> Option<(usize, String, String)> {
    let w = inner.world.borrow();
    let n = w.get(id)?;
    let s = n.current().filter(|s| !s.disabled && !s.enter_function.is_empty())?;
    let old_name = old.and_then(|o| n.data.states.get(o)).map(|o| o.name.clone()).unwrap_or_default();
    Some((n.state, s.enter_function.clone(), old_name))
}

/// THE way a state's InitState and enter function run (the body of `0x0102DF60`), for its three
/// callers (CONFIRMED, its cross-references): a pointer transition (`0x01035620`), a script's
/// SetState (`0x01035B30`) and a new component's initialisation (`0x01034210`). Under the stack
/// guard ([`state_function_guard`]): InitState through `__ntw_call_if_defined` (hooks run,
/// [`call_init_state`]); then the enter function, if the state has one and is not disabled, with
/// the old state's name (`old`; empty for a new component), if the component is still there.
/// Nothing is laid out here: every geometry read lays out what changed ([`lay_out_if_stale`]).
fn run_state_entry(lua: &Lua, inner: &Rc<Inner>, id: NodeId, old: Option<usize>) -> mlua::Result<()> {
    state_function_guard(|| {
        let enter = enter_call(inner, id, old);
        call_init_state(lua, inner, id);
        if let Some((owner, enter, old_name)) = enter
            && inner.world.borrow().get(id).is_some()
        {
            call_state_function(lua, inner, id, owner, &enter, &old_name);
        }
    })
}

/// A pointer event moved a component from state `old` to its current one (`0x01035620`,
/// CONFIRMED): the old state's exit function runs with the new state's name, unless the old
/// state is disabled (`0x0102DFD0`); then, if the component still has its script environment
/// (`0x010587B0`), the state it is in *now* (re-read: the exit function may have set another)
/// gets, unless it is disabled, `InitState` and its enter function with the old state's name
/// (`0x0102DF60`). The state record's +0x115 flag is the one the click handler `0x0102E340`
/// refuses clicks on, our `disabled` (INFERRED mapping). This is how `template.button_close.lua`
/// closes its popup: the shipped close buttons give their "depress" state the exit function
/// `OnLeaveDepress` and bind no event.
///
/// CONFIRMED as the original, kept on purpose: the +0x115 state gets no `InitState` on this path
/// (unlike SetState, see `component_methods`); and the exit function runs on *any* move out of
/// the state, so a press dragged off a close button ("depress" → "depress mouse off", key 1)
/// runs `OnLeaveDepress` and closes the panel too.
///
/// Still alive: `Component.Destroy` only queues the component for the end of the frame and
/// leaves its script environment (`0x01017F10` → `0x01037230`, CONFIRMED), so a close button
/// whose exit function closed its own panel still gets InitState and its enter function in the
/// original. Ours defers Destroy the same way, and skips only a component already destroyed.
///
/// Every host runs these (front end, battle, campaign): the exe has one UI component class for
/// all three and these functions test no mode (CONFIRMED). Nesting: see
/// [`state_function_guard`]; InitState and the enter function run through [`run_state_entry`].
fn state_transitioned(lua: &Lua, inner: &Rc<Inner>, id: NodeId, old: usize) {
    let exit = {
        let w = inner.world.borrow();
        let Some(n) = w.get(id) else { return };
        n.data.states.get(old).filter(|o| !o.disabled && !o.exit_function.is_empty()).map(|o| (o.exit_function.clone(), n.state_name().to_owned()))
    };
    if let Some((exit, new_name)) = exit
        && let Err(e) = state_function_guard(|| call_state_function(lua, inner, id, old, &exit, &new_name))
    {
        log(inner, format!("ERROR in {exit} of component {id}: {e}"));
    }
    // Only a component that still has its script environment (`0x010587B0`, re-asked after the
    // exit function, CONFIRMED) and whose new state is not disabled.
    let entered = inner.world.borrow().get(id).and_then(|n| n.current().map(|s| s.disabled));
    if entered != Some(false) || component_script_env(lua, id).is_none() {
        return;
    }
    if let Err(e) = run_state_entry(lua, inner, id, Some(old)) {
        log(inner, format!("ERROR entering the state of component {id}: {e}"));
    }
}

/// Runs the enter or exit function `name` of the component's state `owner` (a global of the
/// component's scripts) with the other state's name (`0x0102DF60` / `0x0102DFD0` push that name,
/// CONFIRMED). A failure is logged with the component and the state.
fn call_state_function(lua: &Lua, inner: &Rc<Inner>, id: NodeId, owner: usize, name: &str, other_state: &str) {
    let r = (|| -> mlua::Result<bool> {
        let f: Function = lua.globals().get("__ntw_call_if_defined")?;
        f.call::<bool>((addr(id), name, other_state))
    })();
    if let Err(e) = r {
        let (comp, state) = component_and_state_names(inner, id, owner);
        log(inner, format!("ERROR in {name} (state \"{state}\") of component {id} \"{comp}\": {e}"));
    }
}

/// Calls the component's `InitState` for its current state (through `__ntw_call_if_defined`, so
/// its hooks run) and applies the image metrics it hands back (`0x0102B480`). Nothing is set up
/// for a component with no InitState to call ([`has_handler`]): the exe calls it only on a
/// component with a script environment (`0x01034210` asks `HasLiveUIScriptEnvironment` first).
/// The text is measured at the component's width as it is now (laid out first if the tree
/// changed, [`lay_out_if_stale`], so an InitState earlier in a batch that changed the tree is
/// seen).
fn call_init_state(lua: &Lua, inner: &Rc<Inner>, id: NodeId) {
    if !has_handler(lua, id, "InitState") {
        return;
    }
    // The state the call is for is fixed here, before the script runs: an InitState that calls
    // SetState still hands back the metrics of THIS state, and they go to it, not to the state it
    // moved to (`0x0102DF60` keeps its state pointer across InitState, CONFIRMED).
    let info = inner.world.borrow().get(id).map(|n| {
        let s = n.current();
        (n.state, n.state_name().to_owned(), s.map(|s| s.font.clone()).unwrap_or_default(), s.map(|s| s.text.clone()).unwrap_or_default())
    });
    let Some((si, state, font, text)) = info else { return };
    // InitState({State = name, Text = {DisplayWidth, DisplayHeight}}): CONFIRMED field names (an
    // inline script in grand_campaign resizes its text box to state.Text.DisplayHeight).
    let (tw, th, _) = if text.is_empty() {
        (0.0, 0.0, 0)
    } else {
        lay_out_if_stale(inner);
        let width = inner.world.borrow().get(id).map_or(0.0, |n| n.rect.w);
        text_extent(inner, &font, &text, width as i32)
    };
    let r = (|| -> mlua::Result<()> {
        let t = lua.create_table()?;
        t.set("State", state)?;
        let tt = lua.create_table()?;
        tt.set("DisplayWidth", tw)?;
        tt.set("DisplayHeight", th)?;
        t.set("Text", tt)?;
        // Images = the state's image metrics {X, Y, Width, Height}; a script may change them and
        // return the table, and the engine applies the new values (CONFIRMED: template.tab.lua
        // widens its middle image and moves its right cap, then returns the state; nil = keep).
        let metrics: Vec<(i32, i32, i32, i32)> = inner
            .world
            .borrow()
            .get(id)
            .and_then(|n| n.data.states.get(si).map(|s| s.image_metrics.iter().map(|m| (m.offset.0, m.offset.1, m.width, m.height)).collect()))
            .unwrap_or_default();
        let images = lua.create_table()?;
        for (i, (x, y, w, h)) in metrics.iter().enumerate() {
            let m = lua.create_table()?;
            m.set("X", *x)?;
            m.set("Y", *y)?;
            m.set("Width", *w)?;
            m.set("Height", *h)?;
            images.set(i + 1, m)?;
        }
        t.set("Images", images)?;
        // Through __ntw_call_if_defined, which hosts may hook (see `__ntw_has_handler`); the script
        // edits `t` in place, so the changes are read back from it.
        let f: Function = lua.globals().get("__ntw_call_if_defined")?;
        if !f.call::<bool>((addr(id), "InitState", t.clone()))? {
            return Ok(());
        }
        let Ok(Value::Table(imgs)) = t.get::<Value>("Images") else { return Ok(()) };
        let mut w = inner.world.borrow_mut();
        w.update_appearance(id, |n| {
            let Some(s) = n.data.states.get_mut(si) else { return false };
            let mut changed = false;
            for (i, m) in s.image_metrics.iter_mut().enumerate() {
                let Ok(Value::Table(e)) = imgs.get::<Value>(i + 1) else { continue };
                if let Ok(Some(v)) = e.get::<Option<f64>>("X") {
                    changed |= set_changed(&mut m.offset.0, v as i32);
                }
                if let Ok(Some(v)) = e.get::<Option<f64>>("Y") {
                    changed |= set_changed(&mut m.offset.1, v as i32);
                }
                if let Ok(Some(v)) = e.get::<Option<f64>>("Width") {
                    changed |= set_changed(&mut m.width, v as i32);
                }
                if let Ok(Some(v)) = e.get::<Option<f64>>("Height") {
                    changed |= set_changed(&mut m.height, v as i32);
                }
            }
            changed
        });
        Ok(())
    })();
    if let Err(e) = r {
        log(inner, format!("ERROR in InitState of component {id}: {e}"));
    }
}

/// Creates the component's environment and runs its inline script and its script file.
fn run_component_scripts(lua: &Lua, inner: &Rc<Inner>, id: NodeId, layout_top: Option<&str>) {
    let (inline, files) = {
        let w = inner.world.borrow();
        let Some(n) = w.get(id) else { return };
        let (dir, layout) = n.layout_file.rsplit_once('/').unwrap_or(("", &n.layout_file));
        // ScriptFileNameOverride first: a template script (`template.button_standard.lua` in
        // ui/templates/) or a script of the page's own `_scripts` folder (napoleon_battles' icons
        // name `battle_icon.lua`, CONFIRMED file napoleon_battles_scripts/battle_icon.luac). Then the
        // id-named files. INFERRED order: in `options`, the panel `brightness_gamma` names
        // `gamma.lua` while its slider `gamma` (override: the slider template) must not run it.
        let mut files = Vec::new();
        if !n.data.script_override.is_empty() {
            files.push(format!("ui/templates/{}", n.data.script_override));
            files.push(format!("{dir}/{layout}_scripts/{}", n.data.script_override));
        }
        // The override "null" means no script file at all (INFERRED: options' gamma slider names "null"
        // and must not run options_scripts/gamma.lua, which is its panel's script).
        if n.data.script_override != "null" {
            files.push(format!("{dir}/{layout}_scripts/{}.lua", n.data.id));
            files.push(format!("{dir}/{layout}.{}.lua", n.data.id));
            // `<folder>/<layout>.lua` belongs to a layout's top component (CONFIRMED files such as
            // message_box.luac and credits.luac; exe string "%S/%s.lua"; attachment INFERRED).
            if let Some(top_id) = layout_top {
                files.push(format!("{dir}/{layout}.lua"));
                // Last, `ui/templates/template.<id>.lua` (exe string "template." CONFIRMED; INFERRED
                // rule: city_info_bar, created by Labels.lua as "label", runs template.city_info_bar.lua;
                // the id in the file counts, not the one it was created under).
                // Only for a component without an inline script (the options page's tabs load
                // template.tab.lua themselves, with their own settings first).
                if n.data.script.trim().is_empty() {
                    files.push(format!("ui/templates/template.{top_id}.lua"));
                }
            }
            // Last: `<folder>/<id>.lua`. The battle UI folder has scripts named after components:
            // `root.luac` (the HUD's top component), `play.luac`, `pause.luac`, `fwd.luac`,
            // `ffwd.luac`, `slow_mo.luac`, `button_halt.luac`, ... (CONFIRMED files; rule INFERRED).
            // A file's `root` wrapper created under another component (land_battle_orders has
            // two children, so its wrapper is kept) must not run the HUD's root.lua again.
            // Only for the folder's main `layout` file: the front end's `player_stats.luac` must not
            // run on a component of that name inside a page.
            if layout == "layout" && (n.data.id != "root" || n.parent.is_none()) {
                files.push(format!("{dir}/{}.lua", n.data.id));
            }
            // Any other component without a script of its own: `template.<id>.lua` (INFERRED:
            // the entity lists' `tabgroup` has no script field, yet its tabs (template.tab.lua)
            // LuaCall SetTabVisibility / SetInitialTab on it, which template.tabgroup.lua defines).
            if layout_top.is_none() && n.data.script.trim().is_empty() && n.data.script_override.is_empty() {
                files.push(format!("ui/templates/template.{}.lua", n.data.id));
            }
        }
        (n.data.script.clone(), files)
    };
    let r = (|| -> mlua::Result<()> {
        let make: Function = lua.globals().get("__ntw_make_env")?;
        let env: Table = make.call(addr(id))?;
        let run: Function = lua.globals().get("__ntw_run_in")?;
        if !inline.trim().is_empty() {
            let f = lua.load(inline.as_str()).set_name(format!("@component {id} inline script")).into_function()?;
            run.call::<MultiValue>((env.clone(), f))?;
        }
        if let Some(file) = files.iter().find_map(|p| inner.source.find(p)) {
            let f = load_chunk(lua, &file.bytes, &file.chunk_name)?;
            run.call::<MultiValue>((env, f))?;
        }
        Ok(())
    })();
    if let Err(e) = r {
        log(inner, format!("ERROR in scripts of component {id}: {e}"));
    }
}

/// Lua 5.1 `require` reading from the install (same rules as the campaign host).
fn install_require(lua: &Lua, inner: &Rc<Inner>) -> mlua::Result<()> {
    let package: Table = lua.globals().get("package")?;
    package.set("path", "?.lua")?;
    let loaders: Table = package.get("loaders")?;
    let inner = inner.clone();
    let searcher = lua.create_function(move |lua, name: String| {
        let package: Table = lua.globals().get("package")?;
        let path: String = package.get("path")?;
        let file = name.replace('.', "/");
        let mut tried = String::new();
        for template in path.split(';').filter(|t| !t.is_empty()) {
            let candidate = template.replace('?', &file);
            match inner.source.find(&candidate) {
                Some(f) => return Ok(Value::Function(load_chunk(lua, &f.bytes, &f.chunk_name)?)),
                None => tried.push_str(&format!("\n\tno file '{candidate}' in the install")),
            }
        }
        Ok(Value::String(lua.create_string(&tried)?))
    })?;
    let n = loaders.raw_len();
    for i in (2..=n).rev() {
        let v: Value = loaders.raw_get(i)?;
        loaders.raw_set(i + 1, v)?;
    }
    loaders.raw_set(2, searcher)?;
    Ok(())
}

/// A component's on-screen rectangle as it is now (laid out first if the tree changed).
fn rect_of(inner: &Inner, id: NodeId) -> Option<UiRect> {
    lay_out_if_stale(inner);
    inner.world.borrow().get(id).map(|n| n.rect)
}

/// A component's rectangle as the scripts see it (Position, Dimensions, Width, Height, Bounds):
/// in the campaign HUD (`Inner::script_frame`), the scripts' frame of
/// [`super::world::UiWorld::script_rect`], where the root keeps its layout size; elsewhere the
/// on-screen rectangle.
fn script_rect_of(inner: &Inner, id: NodeId) -> Option<UiRect> {
    lay_out_if_stale(inner);
    script_rect_in(inner, &inner.world.borrow(), id)
}

/// [`script_rect_of`] on a tree already laid out.
fn script_rect_in(inner: &Inner, w: &UiWorld, id: NodeId) -> Option<UiRect> {
    if inner.script_frame.get() { w.script_rect(*inner.root.borrow(), id) } else { w.get(id).map(|n| n.rect) }
}

/// Moves a node so its top-left lands on (x, y) in the scripts' frame ([`script_rect_of`]).
fn move_to(inner: &Inner, id: NodeId, x: f32, y: f32) {
    lay_out_if_stale(inner);
    let mut w = inner.world.borrow_mut();
    let (dx, dy) = if inner.script_frame.get() { w.script_to_screen(id) } else { (0.0, 0.0) };
    let Some(n) = w.get_mut(id) else { return };
    n.offset.0 += x + dx - n.rect.x;
    n.offset.1 += y + dy - n.rect.y;
}

/// `UIComponent` methods implemented in Rust; each takes the component's address first.
fn component_methods(lua: &Lua, inner: &Rc<Inner>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    macro_rules! method {
        ($name:literal, |$inner:ident, $lua:ident, $id:ident, $args:ident| $body:expr) => {{
            let $inner = inner.clone();
            t.set(
                $name,
                lua.create_function(move |$lua, (a, $args): (Value, MultiValue)| {
                    let Some($id) = node_of(&a) else {
                        log(&$inner, format!("{}: not a component address", $name));
                        return Ok(MultiValue::new());
                    };
                    let _ = (&$args, &$lua);
                    // Name the method in an error (a wrong-typed argument says which call it was).
                    #[allow(clippy::redundant_closure_call)]
                    let r: mlua::Result<MultiValue> = (|| $body)();
                    r.map_err(|e| mlua::Error::runtime(format!("UIComponent.{}: {e}", $name)))
                })?,
            )?;
        }};
    }
    fn arg<T: mlua::FromLua>(lua: &Lua, args: &MultiValue, i: usize) -> mlua::Result<T> {
        T::from_lua(args.get(i).cloned().unwrap_or(Value::Nil), lua)
    }
    fn ret(lua: &Lua, values: impl mlua::IntoLuaMulti) -> mlua::Result<MultiValue> {
        values.into_lua_multi(lua)
    }

    method!("Address", |inner, lua, id, args| ret(lua, addr(id)));
    method!("Id", |inner, lua, id, args| ret(lua, inner.world.borrow().get(id).map(|n| n.data.id.clone())));
    // IsDragged(): whether a mouse drag is in progress on this component (0-E, 2026-10-06).
    // CONFIRMED the question and its one caller: `ui\templates\cards.luac`'s `OnLeftClickUp`
    // (proto at line 26) reads `Component.Call("IsDragged")` -- with no path, so it resolves to
    // *this* component -- and does nothing unless the answer is exactly `false`:
    //   `if false == Component.Call("IsDragged") and this:CurrentState() ~= "UnSelectable" then ...`
    // Before this binding the call answered nil, `false == nil` is false, and **every card in the
    // campaign and battle card groups silently ignored its own left click** (the log line the
    // script prints on that branch is `cards.lua:26`'s last constant; it is what showed it).
    // PROVISIONAL: always false -- our HUD keeps no drag-and-drop state, so nothing is ever
    // dragged and the original's answer can only be false (or true while a drag runs).
    method!("IsDragged", |inner, lua, _id, args| ret(lua, false));
    // Parent(n): the n-th ancestor, default 1; Parent("id"): an ancestor with that id (see below).
    // INFERRED from use: template.list.lua calls Parent(2) to reach its list view, and
    // napoleon_battles_scripts/battle_icon.lua calls Parent("napoleon_battles").
    method!("Parent", |inner, lua, id, args| {
        let w = inner.world.borrow();
        let up = |c: NodeId| w.get(c).and_then(|n| n.parent);
        let found = match args.front() {
            Some(Value::String(s)) => {
                let s = s.to_string_lossy();
                // The OUTERMOST ancestor with that id (INFERRED: a page root is renamed to the page
                // name and runs the page script too, and only that instance gets OnEnter from
                // root.lua's TransitionTo; battle_icon.lua must reach that instance).
                let mut cur = up(id);
                let mut found = None;
                while let Some(c) = cur {
                    if w.get(c).is_some_and(|n| n.data.id == s) {
                        found = Some(c);
                    }
                    cur = up(c);
                }
                found
            }
            other => {
                let n = match other {
                    Some(Value::Integer(i)) => (*i).max(1) as usize,
                    Some(Value::Number(f)) => f.max(1.0) as usize,
                    _ => 1,
                };
                let mut cur = Some(id);
                for _ in 0..n {
                    cur = cur.and_then(up);
                }
                cur
            }
        };
        ret(lua, found.map(addr))
    });
    method!("ChildCount", |inner, lua, id, args| ret(lua, inner.world.borrow().get(id).map(|n| n.children.len()).unwrap_or(0)));
    method!("Find", |inner, lua, id, args| {
        let w = inner.world.borrow();
        let found = match args.front() {
            Some(Value::Integer(i)) => w.get(id).and_then(|n| n.children.get(*i as usize).copied()),
            Some(Value::Number(i)) => w.get(id).and_then(|n| n.children.get(*i as usize).copied()),
            Some(Value::String(s)) => {
                let s = s.to_string_lossy();
                // Descendants first; failing that, the parent's other children's subtrees
                // (INFERRED: review_DY.lua finds its sibling `tab_group` with Find and sizes the
                // unit cards from its width).
                w.get(id).and_then(|n| n.children.iter().find_map(|&c| w.find(c, &s))).or_else(|| {
                    let parent = w.get(id)?.parent?;
                    w.get(parent)?.children.iter().filter(|&&c| c != id).find_map(|&c| w.find(c, &s))
                })
            }
            _ => None,
        };
        ret(lua, found.map(addr))
    });
    method!("SetVisible", |inner, lua, id, args| {
        let v: bool = arg(lua, &args, 0)?;
        inner.world.borrow_mut().update_appearance(id, |n| set_changed(&mut n.visible, v));
        ret(lua, ())
    });
    method!("Visible", |inner, lua, id, args| ret(lua, inner.world.borrow().get(id).is_some_and(|n| n.visible)));
    method!("CurrentState", |inner, lua, id, args| ret(lua, inner.world.borrow().get(id).map(|n| n.state_name().to_owned())));
    method!("SetState", |inner, lua, id, args| {
        let name: String = arg(lua, &args, 0)?;
        let old = inner.world.borrow().get(id).map(|n| n.state);
        inner.world.borrow_mut().set_state(id, &name);
        // InitState follows every SetState to a state the component has, also to the state it is
        // already in (CONFIRMED: `0x01035B30` finds the state by name, makes it current and calls
        // `0x0102DF60` → InitState without comparing with the old state; template.tooltip.lua
        // relies on it: SetText sets the text, Resize(200, 200), SetState("NewState"), and its
        // InitState then fits the box to the text). `0x0102DF60` then runs the new state's enter
        // function with the old state's name, on every SetState, also to the state it was already
        // in, and SetState runs no exit function (CONFIRMED, `0x01035B30`, `0x0102DF60`). The
        // enter function is skipped for a state with the +0x115 flag, which still gets InitState
        // on this path, unlike a pointer transition (CONFIRMED); that flag is our `disabled`
        // (INFERRED mapping).
        //
        // CONFIRMED as the original, kept on purpose: the enter function runs on EVERY SetState,
        // also one to the state the component is already in. So engine-side code of ours (the
        // preludes' card managers, the battle cards' per-frame update) must not call SetState more
        // often than the engine does: they call it only when the state differs (how the engine
        // itself marks selected cards is UNKNOWN), as a repeated SetState("Selected") would re-run
        // BattleUnitCard's "Selected" enter function every frame.
        let exists = inner.world.borrow().get(id).is_some_and(|n| n.current().is_some_and(|s| s.name == name));
        if exists {
            run_state_entry(lua, &inner, id, old)?;
        }
        ret(lua, ())
    });
    // The geometry reads lay out first what changed since the last layout (layout rule,
    // [`lay_out_if_stale`]): Position, Dimensions, Width, Height, Bounds, MoveTo, Resize,
    // SetStateText.
    method!("Position", |inner, lua, id, args| {
        ret(lua, script_rect_of(&inner, id).map(|r| (r.x, r.y)).unwrap_or_default())
    });
    method!("Dimensions", |inner, lua, id, args| {
        ret(lua, script_rect_of(&inner, id).map(|r| (r.w, r.h)).unwrap_or_default())
    });
    method!("MoveTo", |inner, lua, id, args| {
        let (x, y): (f32, f32) = (arg(lua, &args, 0)?, arg(lua, &args, 1)?);
        move_to(&inner, id, x, y);
        ret(lua, ())
    });
    // DockingPoint() → horizontal, vertical; SetDockingPoint(horizontal, vertical) (handlers
    // 0x01016050 / 0x01016100, CONFIRMED tables): horizontal 1 left, 2 right, 3 centre; vertical
    // 3 centre, 4 top, 5 bottom (the `g_*` values of Huds.lua); the layout's docking 1..9 is the
    // grid top-left .. bottom-right, 0 = not docked (DockingPoint gives 0, 0). The argument order
    // is INFERRED from PanelManager's SetDockingPoint(Side, g_centre) and its use of the result.
    method!("DockingPoint", |inner, lua, id, args| {
        let d = inner.world.borrow().get(id).map_or(0, |n| n.data.docking);
        let hv = match d {
            1 => (1, 4),
            2 => (3, 4),
            3 => (2, 4),
            4 => (1, 3),
            5 => (3, 3),
            6 => (2, 3),
            7 => (1, 5),
            8 => (3, 5),
            9 => (2, 5),
            _ => (0, 0),
        };
        ret(lua, hv)
    });
    method!("SetDockingPoint", |inner, lua, id, args| {
        let h: i64 = arg::<Option<i64>>(lua, &args, 0)?.unwrap_or(0);
        let v: i64 = arg::<Option<i64>>(lua, &args, 1)?.unwrap_or(0);
        const GRID: [u32; 9] = [4, 6, 5, 1, 3, 2, 7, 9, 8];
        let d = if (3..=5).contains(&v) && (1..=3).contains(&h) { GRID[((h - 1) + (v - 3) * 3) as usize] } else { 0 };
        if let Some(n) = inner.world.borrow_mut().get_mut(id) {
            n.data.docking = d;
        }
        ret(lua, ())
    });
    method!("Resize", |inner, lua, id, args| {
        let (w, h): (f32, f32) = (arg(lua, &args, 0)?, arg(lua, &args, 1)?);
        // Resize(w, h, resize_children): the handler (0x01014760) passes a third flag, true unless
        // the script gives false (CONFIRMED); false keeps the children's sizes (INFERRED from
        // template.map_image.lua, which resizes the map and its overlay one by one with false).
        let children: bool = arg::<Option<bool>>(lua, &args, 2)?.unwrap_or(true);
        // The children grow from their sizes as they are now; the exe re-docks them inside the
        // call (`0x010324E0`), ours at the next geometry read.
        lay_out_if_stale(&inner);
        {
            let mut world = inner.world.borrow_mut();
            let old = world.get(id).map(|n| (n.rect.w, n.rect.h, n.children.clone()));
            if let Some(n) = world.get_mut(id) {
                n.size_override = Some((w, h));
                n.resized = true;
            }
            // Children that allow resizing keep their margins: they grow and shrink with the
            // parent (INFERRED from the Tooltip template, whose docked frame edges must follow the
            // size its script gives it).
            if let Some((ow, oh, kids)) = old
                && children
                && ow > 0.0
                && oh > 0.0
            {
                let (dw, dh) = (w - ow, h - oh);
                for k in kids {
                    if let Some(c) = world.get_mut(k) {
                        let (cw, ch) = (c.rect.w, c.rect.h);
                        let nw = if c.data.allow_horizontal_resize { (cw + dw).max(0.0) } else { cw };
                        let nh = if c.data.allow_vertical_resize { (ch + dh).max(0.0) } else { ch };
                        if (nw, nh) != (cw, ch) {
                            c.size_override = Some((nw, nh));
                            c.resized = true;
                        }
                    }
                }
            }
        }
        ret(lua, ())
    });
    // GetStateText() → text, then its laid-out width and height (CONFIRMED: template.city_info_bar.lua
    // sizes its name bar from the second result; the third INFERRED as SetStateText's).
    method!("GetStateText", |inner, lua, id, args| {
        let info = inner.world.borrow().get(id).and_then(|n| n.current().map(|s| (s.text.clone(), s.font.clone())));
        match info {
            Some((text, font)) => {
                // Unwrapped width: the bar is sized to the text (INFERRED).
                let (tw, th, _) = if text.is_empty() { (0.0, 0.0, 0) } else { text_extent(&inner, &font, &text, 1 << 20) };
                // PROVISIONAL: front-end hosts keep returning the text only, until template.tab.lua's
                // use of the width (with InitState's Images, which it edits) is implemented for the
                // options page.
                // The extent follows only for a non-empty text (0x01013DD0 / 0x01036A40, CONFIRMED: the
                // text length +0x30 must be non-zero; then 4 results, else the text alone).
                if inner.campaign_mode.get() && !text.is_empty() { ret(lua, (text, tw, th)) } else { ret(lua, text) }
            }
            None => ret(lua, ()),
        }
    });
    // SetStateText(text) → width, height, line count of the text as laid out in the component
    // (CONFIRMED: template.TextView.lua resizes its text box to the returned height; Utilities.lua
    // reads three results). Lines wrap at the component's width minus the TextXOffset inset,
    // not at all for HBehaviour NeverSplit (the exe's layout 0x010258A0, CONFIRMED; the line
    // breaker itself is PROVISIONAL, see `CufFont::wrap_lines`).
    method!("SetStateText", |inner, lua, id, args| {
        let text: String = match args.front() {
            Some(Value::String(s)) => lua_text(s),
            Some(Value::Integer(i)) => i.to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        lay_out_if_stale(&inner);
        let (font, width) = {
            let mut out = None;
            inner.world.borrow_mut().update_appearance(id, |n| {
                let i = n.state;
                let (w, h) = (n.rect.w, n.rect.h);
                let Some(s) = n.data.states.get_mut(i) else { return false };
                let changed = set_changed(&mut s.text, text.clone());
                let width = if s.text_wraps() { s.text_area(w, h).0 } else { 1.0e6 };
                out = Some((s.font.clone(), width));
                changed
            });
            let Some(out) = out else { return ret(lua, ()) };
            out
        };
        let (w, h, lines) = text_extent(&inner, &font, &text, width as i32);
        ret(lua, (w, h, lines))
    });
    method!("GetStateTextDetails", |inner, lua, id, args| {
        let colour = inner.world.borrow().get(id).and_then(|n| n.current()).map(|s| s.font_colour).unwrap_or(0xFF00_0000);
        let [b, g, r, a] = colour.to_le_bytes();
        let c = lua.create_table()?;
        c.set("r", r)?;
        c.set("g", g)?;
        c.set("b", b)?;
        c.set("a", a)?;
        let t = lua.create_table()?;
        t.set("Colour", c)?;
        // The exe's GetStateTextDetails (0x01013EA0, CONFIRMED): XOffset / YOffset are the state's
        // TextXOffset / TextYOffset (+0x60 / +0x64), HAlign / VAlign / HBehaviour the numbers.
        let st = inner.world.borrow().get(id).and_then(|n| n.current().cloned()).unwrap_or_default();
        t.set("XOffset", st.text_x_offset)?;
        t.set("YOffset", st.text_y_offset)?;
        t.set("HAlign", st.text_align.0)?;
        t.set("VAlign", st.text_align.1)?;
        t.set("HBehaviour", st.text_behaviour.0)?;
        ret(lua, t)
    });
    method!("SetStateTextXOffset", |inner, lua, id, args| {
        // 0x01013D00 → 0x01035C80 (CONFIRMED): the current state's TextXOffset (+0x60).
        let v: Option<f64> = arg(lua, &args, 0)?;
        inner.world.borrow_mut().update_appearance(id, |n| {
            let i = n.state;
            n.data.states.get_mut(i).is_some_and(|s| set_changed(&mut s.text_x_offset, v.unwrap_or(0.0) as i32))
        });
        ret(lua, ())
    });
    method!("SetStateTextDetails", |inner, lua, id, args| {
        // The exe's SetStateTextDetails (0x01013930, CONFIRMED keys): XOffset / YOffset numbers,
        // HAlign / VAlign / HBehaviour as names (`top bottom left right centre`,
        // `SplitByCharacter SplitByWord NeverSplit`; numbers are taken too), Colour {r g b a}.
        // Keys that are absent keep the current value.
        let Some(t): Option<Table> = arg(lua, &args, 0)? else { return ret(lua, ()) };
        let int = |k: &str| t.get::<Option<f64>>(k).ok().flatten().map(|v| v as i32);
        let named = |k: &str, names: &[&str]| -> Option<i32> {
            match t.get::<Value>(k).ok()? {
                Value::String(s) => {
                    let s = s.to_str().ok()?.to_ascii_lowercase();
                    names.iter().position(|n| n.eq_ignore_ascii_case(&s)).map(|i| i as i32)
                }
                Value::Integer(i) => Some(i as i32),
                Value::Number(f) => Some(f as i32),
                _ => None,
            }
        };
        const ALIGN: [&str; 5] = ["top", "bottom", "left", "right", "centre"];
        const SPLIT: [&str; 3] = ["SplitByCharacter", "SplitByWord", "NeverSplit"];
        let colour = t.get::<Option<Table>>("Colour").ok().flatten().map(|c| {
            let ch = |k: &str| c.get::<Option<f64>>(k).ok().flatten().unwrap_or(255.0).clamp(0.0, 255.0) as u8;
            u32::from_le_bytes([ch("b"), ch("g"), ch("r"), ch("a")])
        });
        let (xo, yo) = (int("XOffset"), int("YOffset"));
        let (ha, va, hb) = (named("HAlign", &ALIGN), named("VAlign", &ALIGN), named("HBehaviour", &SPLIT));
        inner.world.borrow_mut().update_appearance(id, |n| {
            let i = n.state;
            let Some(s) = n.data.states.get_mut(i) else { return false };
            let mut changed = false;
            if let Some(c) = colour {
                changed |= set_changed(&mut s.font_colour, c);
            }
            if let Some(v) = xo {
                changed |= set_changed(&mut s.text_x_offset, v);
            }
            if let Some(v) = yo {
                changed |= set_changed(&mut s.text_y_offset, v);
            }
            if let Some(v) = ha {
                changed |= set_changed(&mut s.text_align.0, v);
            }
            if let Some(v) = va {
                changed |= set_changed(&mut s.text_align.1, v);
            }
            if let Some(v) = hb {
                changed |= set_changed(&mut s.text_behaviour.0, v);
            }
            changed
        });
        ret(lua, ())
    });
    method!("GetProperty", |inner, lua, id, args| {
        let key: String = arg(lua, &args, 0)?;
        ret(lua, inner.world.borrow().get(id).and_then(|n| n.data.properties.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone())))
    });
    method!("Adopt", |inner, lua, id, args| {
        if let Some(child) = args.front().and_then(node_of) {
            inner.world.borrow_mut().adopt(id, child);
        }
        ret(lua, ())
    });
    method!("Divorce", |inner, lua, id, args| {
        if let Some(child) = args.front().and_then(node_of) {
            let is_child = inner.world.borrow().get(child).is_some_and(|c| c.parent == Some(id));
            if is_child {
                inner.world.borrow_mut().divorce_from_parent(child);
            }
        }
        ret(lua, ())
    });
    method!("Width", |inner, lua, id, args| ret(lua, script_rect_of(&inner, id).map_or(0.0, |r| r.w)));
    method!("Height", |inner, lua, id, args| ret(lua, script_rect_of(&inner, id).map_or(0.0, |r| r.h)));
    // Bounds → width, height of the box around the component and its direct children
    // (`0x010133C0`, CONFIRMED; `RegisterHud` reads the HUD band's, which the debugger sitting of
    // 2026-10-07 read as 1280 x 241 in the original).
    method!("Bounds", |inner, lua, id, args| {
        lay_out_if_stale(&inner);
        let w = inner.world.borrow();
        let (Some(n), Some(r)) = (w.get(id), script_rect_in(&inner, &w, id)) else { return ret(lua, (0.0f32, 0.0f32)) };
        let (mut x0, mut y0, mut x1, mut y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
        // Every direct child counts, hidden or empty (the exe's loop tests neither).
        for &c in &n.children {
            if let Some(k) = script_rect_in(&inner, &w, c) {
                (x0, y0, x1, y1) = (x0.min(k.x), y0.min(k.y), x1.max(k.x + k.w), y1.max(k.y + k.h));
            }
        }
        ret(lua, (x1 - x0, y1 - y0))
    });
    method!("SetTooltipText", |inner, lua, id, args| {
        let text: Option<String> = arg(lua, &args, 0)?;
        inner.world.borrow_mut().update_appearance(id, |n| set_changed(&mut n.tooltip, text));
        ret(lua, ())
    });
    method!("GetTooltipText", |inner, lua, id, args| {
        let w = inner.world.borrow();
        let text = w.get(id).map(|n| {
            n.tooltip.clone().unwrap_or_else(|| {
                let state_tip = n.current().map(|s| s.tooltip_text.clone()).unwrap_or_default();
                if state_tip.is_empty() { n.data.tooltip_text.clone() } else { state_tip }
            })
        });
        ret(lua, text)
    });
    method!("SetInteractive", |inner, lua, id, args| {
        let v: bool = arg::<Option<bool>>(lua, &args, 0)?.unwrap_or(true);
        inner.world.borrow_mut().update_appearance(id, |n| set_changed(&mut n.interactive, v));
        ret(lua, ())
    });
    method!("IsInteractive", |inner, lua, id, args| ret(lua, inner.world.borrow().get(id).is_some_and(|n| n.interactive)));
    // SetDisabled(b): INFERRED to stop mouse input like SetInteractive(not b) (state art is chosen by scripts).
    method!("SetDisabled", |inner, lua, id, args| {
        let v: bool = arg::<Option<bool>>(lua, &args, 0)?.unwrap_or(true);
        inner.world.borrow_mut().update_appearance(id, |n| set_changed(&mut n.interactive, !v));
        ret(lua, ())
    });
    // PropagateVisibility(b): the component and every descendant (INFERRED from the name).
    method!("PropagateVisibility", |inner, lua, id, args| {
        let v: bool = arg::<Option<bool>>(lua, &args, 0)?.unwrap_or(true);
        for n in collect_subtree(&inner, id) {
            inner.world.borrow_mut().update_appearance(n, |n| set_changed(&mut n.visible, v));
        }
        ret(lua, ())
    });
    // SequentialFind("a", "b", ...): Find("a"), then "b" inside it, ... (INFERRED from the name).
    method!("SequentialFind", |inner, lua, id, args| {
        let w = inner.world.borrow();
        let mut cur = Some(id);
        for a in args.iter() {
            let Some(c) = cur else { break };
            cur = match a {
                Value::String(s) => {
                    let s = s.to_string_lossy();
                    w.get(c).and_then(|n| n.children.iter().find_map(|&k| w.find(k, &s)))
                }
                Value::Integer(i) => w.get(c).and_then(|n| n.children.get(*i as usize).copied()),
                Value::Number(i) => w.get(c).and_then(|n| n.children.get(*i as usize).copied()),
                _ => None,
            };
        }
        ret(lua, cur.map(addr))
    });
    // ReorderChildren(child, index) INFERRED shape: moves a child in the draw order.
    method!("ReorderChildren", |inner, lua, id, args| {
        let child = args.front().and_then(node_of);
        let index: Option<f64> = arg(lua, &args, 1)?;
        if let (Some(c), Some(i)) = (child, index) {
            let ok = inner.world.borrow().get(c).is_some_and(|n| n.parent == Some(id));
            if ok {
                inner.world.borrow_mut().reorder(c, i.max(0.0) as usize);
            }
        }
        ret(lua, ())
    });
    // Internal: a script bound a mouse event at run time (see `UiNode::script_events`).
    method!("__MarkEvent", |inner, lua, id, args| {
        let e: String = arg(lua, &args, 0)?;
        inner.world.borrow_mut().update_appearance(id, |n| {
            let new = !n.script_events.contains(&e);
            if new {
                n.script_events.push(e);
            }
            new
        });
        ret(lua, ())
    });
    // Internal: `UIImage:SetComponentTexture(component, index)` puts an image file on the
    // component's n-th image (CONFIRMED call shape in sp_load_game.lua and others).
    method!("__SetImagePath", |inner, lua, id, args| {
        let i: usize = arg::<Option<usize>>(lua, &args, 0)?.unwrap_or(0);
        let path: String = arg(lua, &args, 1)?;
        inner.world.borrow_mut().update_appearance(id, |n| {
            let Some(img) = n.data.images.get_mut(i) else { return false };
            set_changed(&mut img.path, path) | set_changed(&mut n.painted, true)
        });
        ret(lua, ())
    });
    method!("Priority", |inner, lua, id, args| ret(lua, inner.world.borrow().get(id).map(|n| n.data.priority).unwrap_or(0)));
    method!("StealShortcutKey", |inner, lua, id, args| {
        // StealShortcutKey(false): give back every key this component stole (INFERRED from
        // PanelManager's ClearupPanel and the results popup's OnDismissed).
        if matches!(args.front(), Some(Value::Boolean(false)) | None) {
            inner.stolen_keys.borrow_mut().retain(|(n, _)| *n != id);
            return ret(lua, ());
        }
        let key: String = arg(lua, &args, 0)?;
        let mut keys = inner.stolen_keys.borrow_mut();
        keys.retain(|(n, k)| !(*n == id && *k == key));
        keys.push((id, key));
        ret(lua, ())
    });
    method!("StealInputFocus", |inner, lua, id, args| {
        // StealInputFocus(take) (`0x01015DC0`, CONFIRMED): true takes the focus (even for a
        // component that does not take it on a click); false or nothing clears it (root.lua
        // passes false; template.text_input.lua lets go on RETURN / ESCAPE).
        let take = matches!(args.front(), Some(Value::Boolean(true)));
        set_focus(lua, &inner, take.then_some(id), true);
        ret(lua, ())
    });
    // IsCharPrintable(c) (`0x01015F10`): a space, or a character the component's font has a
    // glyph for (INFERRED: the exe asks its font object, `0x0102C1D0`); control characters never.
    method!("IsCharPrintable", |inner, lua, id, args| {
        let c = match args.front() {
            Some(Value::String(s)) => lua_text(s).chars().next(),
            _ => None,
        };
        let font = inner.world.borrow().get(id).and_then(|n| n.current().map(|s| s.font.clone())).unwrap_or_default();
        let ok = match c {
            Some(' ') => true,
            Some(c) if c.is_control() => false,
            Some(c) => with_font(&inner, &font, |f| f.glyph(c).is_some()).unwrap_or(true),
            None => false,
        };
        ret(lua, ok)
    });
    // FindPositionIntoCurrentText(text, x) (`0x01013D40` → `0x01029E10`): the caret position (a
    // character count) nearest to `x` pixels into the text in the component's font (INFERRED:
    // the nearest character boundary).
    method!("FindPositionIntoCurrentText", |inner, lua, id, args| {
        let text = match args.front() {
            Some(Value::String(s)) => lua_text(s),
            _ => String::new(),
        };
        let x: f64 = arg::<Option<f64>>(lua, &args, 1)?.unwrap_or(0.0);
        let font = inner.world.borrow().get(id).and_then(|n| n.current().map(|s| s.font.clone())).unwrap_or_default();
        let chars: Vec<char> = text.chars().collect();
        let pos = with_font(&inner, &font, |f| {
            (0..=chars.len())
                .min_by_key(|&i| {
                    let w = f.text_width(&chars[..i].iter().collect::<String>()) as f64;
                    ((w - x).abs() * 16.0) as i64
                })
                .unwrap_or(0)
        })
        .unwrap_or(chars.len());
        ret(lua, pos)
    });
    // SetImageColour(index, r, g, b, a): colour of the current state's n-th image metric.
    // CONFIRMED call shape in root.lua (0, 0, 0, 0, 255 on the "layout" backdrop); index meaning INFERRED.
    method!("SetImageColour", |inner, lua, id, args| {
        let i: usize = arg::<Option<usize>>(lua, &args, 0)?.unwrap_or(0);
        let ch = |k: usize| -> mlua::Result<u8> { Ok(arg::<Option<f64>>(lua, &args, k)?.unwrap_or(255.0).clamp(0.0, 255.0) as u8) };
        let argb = u32::from_le_bytes([ch(3)?, ch(2)?, ch(1)?, ch(4)?]);
        inner.world.borrow_mut().update_appearance(id, |n| {
            let s = n.state;
            let Some(m) = n.data.states.get_mut(s).and_then(|s| s.image_metrics.get_mut(i)) else { return false };
            set_changed(&mut m.colour, argb) | set_changed(&mut n.painted, true)
        });
        ret(lua, ())
    });
    // __ImageMetricsList() → { {x, y, w, h, colour argb}, ... } of the current state (for the
    // prelude's CurrentStateUI():ImageMetrics() objects).
    method!("__ImageMetricsList", |inner, lua, id, args| {
        let list: Vec<(i32, i32, i32, i32, u32)> = inner
            .world
            .borrow()
            .get(id)
            .and_then(|n| n.current().map(|s| s.image_metrics.iter().map(|m| (m.offset.0, m.offset.1, m.width, m.height, m.colour)).collect()))
            .unwrap_or_default();
        let t = lua.create_table()?;
        for (k, (x, y, w, h, c)) in list.into_iter().enumerate() {
            let e = lua.create_table()?;
            e.set(1, x)?;
            e.set(2, y)?;
            e.set(3, w)?;
            e.set(4, h)?;
            e.set(5, c)?;
            t.set(k + 1, e)?;
        }
        ret(lua, t)
    });
    // SetImageMetrics(index, x, y, w, h). CONFIRMED call shape in root.lua (0, 0, 0, FrontEnd.ScreenSize()).
    method!("SetImageMetrics", |inner, lua, id, args| {
        let i: usize = arg::<Option<usize>>(lua, &args, 0)?.unwrap_or(0);
        let v = |k: usize| -> mlua::Result<i32> { Ok(arg::<Option<f64>>(lua, &args, k)?.unwrap_or(0.0) as i32) };
        let (x, y, w, h) = (v(1)?, v(2)?, v(3)?, v(4)?);
        inner.world.borrow_mut().update_appearance(id, |n| {
            let s = n.state;
            let Some(m) = n.data.states.get_mut(s).and_then(|s| s.image_metrics.get_mut(i)) else { return false };
            set_changed(&mut m.offset, (x, y)) | set_changed(&mut m.width, w) | set_changed(&mut m.height, h)
        });
        ret(lua, ())
    });
    // DestroyChildren(): the children leave the tree at once and are destroyed at the end of the
    // frame (INFERRED: graphics.lua takes list_box:Find(0) as the row template, destroys the
    // children, and then copies the template).
    method!("DestroyChildren", |inner, lua, id, args| {
        let kids = inner.world.borrow().get(id).map(|n| n.children.clone()).unwrap_or_default();
        for k in kids {
            inner.world.borrow_mut().divorce_from_parent(k);
            inner.pending_destroy.borrow_mut().push(k);
            // The parent hears about each child leaving (INFERRED: CardGroup.lua's OnDivorce(child)
            // removes the card from its card manager, so a destroyed card is not reused).
            let f: Function = lua.globals().get("__ntw_call_if_defined")?;
            f.call::<bool>((addr(id), "OnDivorce", addr(k)))?;
        }
        ret(lua, ())
    });
    Ok(t)
}

fn destroy(lua: &Lua, inner: &Rc<Inner>, id: NodeId) -> mlua::Result<()> {
    let gone = inner.world.borrow_mut().destroy(id);
    let _ = lua;
    inner.dead_envs.borrow_mut().extend(gone);
    Ok(())
}

/// `Component.*` functions (act on the running component, whose address comes first).
fn component_functions(lua: &Lua, inner: &Rc<Inner>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let i = inner.clone();
    t.set("Address", lua.create_function(move |_, a: Value| Ok(node_of(&a).map(addr).unwrap_or(Value::Nil)))?)?;
    let i2 = i.clone();
    t.set("Root", lua.create_function(move |_, a: Value| {
        let id = node_of(&a);
        Ok(id.map(|id| addr(i2.world.borrow().root_of(id))))
    })?)?;
    let i3 = i.clone();
    t.set("Adopt", lua.create_function(move |_, (a, child): (Value, Value)| {
        if let (Some(p), Some(c)) = (node_of(&a), node_of(&child)) {
            i3.world.borrow_mut().adopt(p, c);
        }
        Ok(())
    })?)?;
    let i4 = i.clone();
    // Destroy(target) only marks: the component leaves its parent now (gone from the screen and
    // from Find) and is destroyed at the end of the frame, like DestroyChildren's. CONFIRMED that
    // the original defers: PanelManager's panel clean-up (`panelmanager.lua:65`) calls
    // `Component.Destroy(panel)` at line 93 and then reads `UIComponent(panel):Id()` at line 103
    // ("Removing panel " .. Id()), and its own log says "Marked for destroy". Destroying at once
    // made that Id nil, the concatenation failed and the close stopped half way (2026-10-06).
    t.set("Destroy", lua.create_function(move |_lua, (_a, target): (Value, Value)| {
        if let Some(c) = node_of(&target) {
            i4.world.borrow_mut().divorce_from_parent(c);
            i4.pending_destroy.borrow_mut().push(c);
        }
        Ok(())
    })?)?;
    let i5 = i.clone();
    // CreateFromLayout(path, id, parent) → address. CONFIRMED call shape in root.lua's TransitionTo.
    // Optional x, y (CONFIRMED 5-argument calls in Labels.lua): the new component's position in its
    // parent (INFERRED, as CreateFromComponent).
    t.set("CreateFromLayout", lua.create_function(move |lua, (_a, path, id, parent, x, y): (Value, String, Option<String>, Value, Value, Value)| {
        match create_layout(lua, &i5, &path, id.as_deref(), node_of(&parent)) {
            Ok(new) => {
                if let (Some(x), Some(y)) = (x.as_f64(), y.as_f64())
                    && let Some(n) = i5.world.borrow_mut().get_mut(new)
                {
                    n.offset = (x as f32, y as f32);
                    n.keep_size = true;
                    n.size_override = None;
                }
                init_new(lua, &i5, new);
                Ok(Some(addr(new)))
            }
            Err(e) => {
                log(&i5, format!("CreateFromLayout failed: {e}"));
                Ok(None)
            }
        }
    })?)?;
    // CreateFromComponent(source, id, parent, x, y, {...}, {child_id = text, ...}) → address.
    // CONFIRMED call shape (sp_load_game.lua copies its hidden "row_example" per save); INFERRED:
    // x, y = offset in the parent, the last table sets the texts of the copy's named children;
    // the 6th table (e.g. {"{icon:1}"}) is UNKNOWN and ignored.
    let i6 = i.clone();
    t.set("CreateFromComponent", lua.create_function(move |lua, (_a, src, id, parent, x, y, _fmt, texts): (Value, Value, Option<String>, Value, Option<f32>, Option<f32>, Value, Option<Table>)| {
        let Some(src) = node_of(&src) else { return Ok(None) };
        let mut created = Vec::new();
        let new = i6.world.borrow_mut().clone_subtree(src, node_of(&parent), &mut created);
        let Some(new) = new else { return Ok(None) };
        for &c in &created {
            run_component_scripts(lua, &i6, c, None);
        }
        finish_created(lua, &i6, new, id.as_deref(), (x, y), texts, &_fmt)?;
        Ok(Some(addr(new)))
    })?)?;
    // CreateComponentFromTemplate(template, id, parent, x, y, {...}, {...}) → address: the same,
    // built from the layout `ui/templates/<template>` (40 such layouts ship; INFERRED).
    let i7 = i.clone();
    t.set("CreateComponentFromTemplate", lua.create_function(move |lua, (_a, template, id, parent, x, y, _fmt, texts): (Value, String, Option<String>, Value, Option<f32>, Option<f32>, Value, Option<Table>)| {
        let path = format!("ui/templates/{}", template.to_ascii_lowercase());
        // A template layout file, else the UIEd template library (INFERRED order; e.g. the
        // campaign HUD's ReviewPanelTab exists only in the library).
        let root = match i7.source.find(&path) {
            // A template file is an editor wrapper `root` holding the template itself (e.g.
            // `campaignunitcard`: root 1600x960 + CampaignUnitCard 66x88): the child is created
            // (INFERRED, the same rule as CreateFromLayout).
            Some(file) => match UiLayout::read(&file.bytes) {
                Ok(l) if l.root.children.len() == 1 => l.root.children.into_iter().next().unwrap(),
                Ok(l) => l.root,
                Err(e) => {
                    log(&i7, format!("CreateComponentFromTemplate {path}: {e}"));
                    return Ok(None);
                }
            },
            None => match template_library(&i7).and_then(|lib| lib.get(&template).map(|t| t.component.clone())) {
                Some(c) => c,
                None => {
                    log(&i7, format!("CreateComponentFromTemplate: no template {path}"));
                    return Ok(None);
                }
            },
        };
        let mut created = Vec::new();
        let new = i7.world.borrow_mut().instantiate(&root, node_of(&parent), &path, &mut created);
        localise(&i7, &created);
        // A template's untextured images are colour fills from the start (INFERRED: the Tooltip
        // template's dark backdrop is one, and its script never paints it).
        {
            let mut w = i7.world.borrow_mut();
            for &c in &created {
                w.update_appearance(c, |n| {
                    n.data.images.iter().any(|im| im.path.is_empty() && (im.colour >> 24) != 0) && set_changed(&mut n.painted, true)
                });
            }
        }
        for &c in &created {
            run_component_scripts(lua, &i7, c, None);
        }
        finish_created(lua, &i7, new, id.as_deref(), (x, y), texts, &_fmt)?;
        Ok(Some(addr(new)))
    })?)?;
    // KeyboardModifiersHeld() → shift, ctrl, alt (INFERRED order); the mouse UI never holds any.
    t.set("KeyboardModifiersHeld", lua.create_function(|_, _: Variadic<Value>| Ok((false, false, false)))?)?;
    // Input priority / focus: no visible effect in our renderer yet (UNKNOWN details).
    t.set("LockPriority", lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    Ok(t)
}

/// `FrontEnd.*` functions implemented so far (the rest log UNKNOWN in the prelude).
fn frontend_functions(lua: &Lua, inner: &Rc<Inner>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    super::frontend::install(lua, inner, &t)?;
    super::battle_setup::install(lua, inner, &t)?;
    super::battle_setup::install_faction_details(lua, inner, &t)?;
    super::battle_setup::install_offline(lua, &t)?;
    super::army_setup::install(lua, inner, &t)?;
    let i = inner.clone();
    t.set("ScreenSize", lua.create_function(move |_, ()| Ok(*i.screen.borrow()))?)?;
    let i = inner.clone();
    t.set("CampaignSavesExist", lua.create_function(move |_, ()| Ok(i.facts.campaign_saves_exist))?)?;
    let i = inner.clone();
    t.set("SpanishCampaignEnabled", lua.create_function(move |_, ()| Ok(i.facts.spanish_campaign))?)?;
    let i = inner.clone();
    t.set("GameVersion", lua.create_function(move |_, ()| Ok(i.facts.game_version.clone()))?)?;
    // No Steam: no new downloadable content to advertise, no invites, nothing played before.
    t.set("SteamNewContentAvailable", lua.create_function(|_, ()| Ok(false))?)?;
    t.set("MPHasGameInvite", lua.create_function(|_, ()| Ok(false))?)?;
    t.set("PreviousGameType", lua.create_function(|_, ()| Ok(-1))?)?;
    let i = inner.clone();
    t.set("Quit", lua.create_function(move |_, ()| {
        i.requests.borrow_mut().push(UiRequest::Quit);
        Ok(())
    })?)?;
    let i = inner.clone();
    // "Retrieve a string from the random localisation strings table" (CONFIRMED description):
    // loc keys random_localisation_strings_string_<key> (e.g. difficulty_level_1 = "Easy").
    t.set("LocalisationString", lua.create_function(move |_, key: String| {
        Ok(i.loc.get(&format!("random_localisation_strings_string_{key}")).or_else(|| i.loc.get(&key)).map(str::to_owned))
    })?)?;
    let i = inner.clone();
    t.set("UILocalisationString", lua.create_function(move |_, key: String| Ok(i.loc.get(&key).map(str::to_owned)))?)?;
    Ok(t)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn facts() -> FrontEndFacts {
        FrontEndFacts { campaign_saves_exist: false, spanish_campaign: false, game_version: "test".into(), ..Default::default() }
    }

    /// A v033 layout: root with a clickable child "button" that has inline Lua.
    fn layout_bytes(script: &str) -> Vec<u8> {
        layout_bytes_with_root("", script)
    }

    /// [`layout_bytes`] with inline Lua on the root as well: the root's script defines the global
    /// functions the engine calls through `call_root_global` (our own made-up test scripts).
    pub(crate) fn layout_bytes_with_root(root_script: &str, script: &str) -> Vec<u8> {
        fn s(b: &mut Vec<u8>, t: &str) {
            b.extend_from_slice(&(t.len() as u16).to_le_bytes());
            b.extend_from_slice(t.as_bytes());
        }
        fn u(b: &mut Vec<u8>, x: u32) {
            b.extend_from_slice(&x.to_le_bytes());
        }
        fn comp(b: &mut Vec<u8>, this: u32, id: &str, script: &str, events: &[(&str, &str)], states: &[&str], kids: &[(u32, &str, &str)]) {
            u(b, this);
            s(b, id);
            u(b, 0);
            u(b, 0);
            b.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0]);
            s(b, "");
            u(b, u32::MAX);
            b.extend_from_slice(&[0, 0, 0, 0]);
            u(b, 0);
            b.push(0);
            s(b, script);
            u(b, 0); // images
            u(b, 0);
            u(b, 0);
            u(b, states.len() as u32);
            for (k, st) in states.iter().enumerate() {
                u(b, this * 10 + k as u32);
                s(b, st);
                u(b, 50);
                u(b, 20);
                b.extend_from_slice(&[0, 0, 0, 0]);
                for x in [2, 0, 0, 1, 1] {
                    u(b, x);
                }
                b.extend_from_slice(&[0, 0, 0, 0, 0]);
                s(b, "Ingame 12, Normal");
                u(b, 2);
                u(b, 1);
                u(b, 0xFF00_0000);
                u(b, 0);
                u(b, 0);
                u(b, 0);
                b.extend_from_slice(&[1, 0, 0]);
                s(b, "");
                for _ in 0..4 {
                    u(b, 0);
                }
                s(b, "");
                s(b, "");
                u(b, 0);
                u(b, 0);
                u(b, 0);
                u(b, 0);
            }
            u(b, 0); // properties
            for (e, f) in events {
                s(b, e);
                s(b, f);
            }
            s(b, "events_end");
            u(b, 0);
            u(b, kids.len() as u32);
            for (t, id, sc) in kids {
                comp(b, *t, id, sc, &[("OnMouseLClickUp", "OnLeftClickUp")], &["normal", "down"], &[]);
            }
            s(b, "");
        }
        let mut b = b"Version033".to_vec();
        comp(&mut b, 1, "root", root_script, &[], &["NewState"], &[(2, "button", script)]);
        b
    }

    #[test]
    fn scripts_run_per_component_and_events_fire() {
        let script = "clicks = 0\nfunction OnLeftClickUp() clicks = clicks + 1; UIComponent(Address):SetState('down'); UIComponent(Component.Root()):SetGlobal('seen', Component.Address() == Address) end\nfunction InitState(t) last_state = t.State end";
        let source = ScriptSource::empty().with_memory_file("ui/test/page", layout_bytes(script));
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        let root = host.load_root_layout("data/ui/test/page").unwrap();
        let button = host.world().find(root, "button").unwrap();
        host.fire(button, "OnMouseLClickUp");
        assert_eq!(host.world().get(button).unwrap().state_name(), "down");
        let benv = component_env(&host, button);
        assert_eq!(benv.get::<i64>("clicks").unwrap(), 1);
        assert_eq!(benv.get::<String>("last_state").unwrap(), "down");
        let renv = component_env(&host, root);
        assert!(renv.get::<bool>("seen").unwrap());
        // The root's environment does not see the button's globals.
        assert!(renv.get::<Option<i64>>("clicks").unwrap().is_none());
        no_errors(&host);
    }

    /// The script environment (globals) of component `id`.
    pub(crate) fn component_env(host: &UiScriptHost, id: NodeId) -> Table {
        host.script_env(id).expect("the component has scripts")
    }

    /// Gives component `id`'s first state a transition to its second on `event`, then lets `edit`
    /// set up the states (the test layouts' buttons: 0 = "normal", 1 = "down").
    pub(crate) fn add_state_transition(host: &UiScriptHost, id: NodeId, event: PointerEvent, edit: impl FnOnce(&mut [ntw_formats::ui_layout::UiState])) {
        let mut w = host.inner.world.borrow_mut();
        let states = &mut w.get_mut(id).unwrap().data.states;
        let down = states[1].this;
        states[0].transitions.push(ntw_formats::ui_layout::UiTransition { key: event as u32, value: down, name: String::new(), a: 0, b: 0 });
        edit(states);
    }

    /// The test page with `script` on its "button", whose "normal" state moves to "down" on
    /// `event`; `edit` then sets up the two states (0 = normal, 1 = down).
    fn state_function_page(script: &str, event: PointerEvent, edit: impl FnOnce(&mut [ntw_formats::ui_layout::UiState])) -> (UiScriptHost, NodeId) {
        let source = ScriptSource::empty().with_memory_file("ui/test/page", layout_bytes(script));
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        let root = host.load_root_layout("ui/test/page").unwrap();
        let button = host.world().find(root, "button").unwrap();
        add_state_transition(&host, button, event, edit);
        (host, button)
    }

    /// A global of the button's script environment.
    fn button_global<T: mlua::FromLua>(host: &UiScriptHost, button: NodeId, name: &str) -> T {
        component_env(host, button).get(name).unwrap()
    }

    use crate::ui::test_support::no_errors;

    /// Nested state-function calls take extra native stack only up to the budget, then fail with
    /// Lua's "C stack overflow" error instead of growing further; the budget is free again after.
    #[test]
    fn nested_state_function_calls_stay_within_the_stack_budget() {
        fn recurse(depth: &mut u32) -> mlua::Result<()> {
            super::state_function_guard(|| {
                *depth += 1;
                let frame = std::hint::black_box([0u8; 64 * 1024]);
                std::hint::black_box(&frame);
                recurse(depth)
            })?
        }
        let mut depth = 0;
        let e = recurse(&mut depth).unwrap_err();
        assert!(e.to_string().contains("C stack overflow"), "{e}");
        assert!(depth > (super::STATE_CALL_STACK_BUDGET / (64 * 1024)) as u32 / 2, "it grew before failing: {depth} levels");
        assert_eq!(super::STATE_CALL_STACK_GROWN.with(|g| g.get()), 0);

        // A panic on a grown segment gives the segment back too.
        fn recurse_then_panic() -> mlua::Result<()> {
            super::state_function_guard(|| {
                let frame = std::hint::black_box([0u8; 64 * 1024]);
                std::hint::black_box(&frame);
                if super::STATE_CALL_STACK_GROWN.with(|g| g.get()) > 0 {
                    panic!("on a grown segment");
                }
                recurse_then_panic()
            })?
        }
        assert!(std::panic::catch_unwind(recurse_then_panic).is_err());
        assert_eq!(super::STATE_CALL_STACK_GROWN.with(|g| g.get()), 0, "the budget is free again after the panic");
    }

    /// New components are initialised children first (post-order), and each gets its current
    /// state's InitState and then its enter function with an empty old-state name, as the exe's
    /// `0x01034210` → `0x0102DF60` (CONFIRMED).
    #[test]
    fn created_components_get_init_state_and_their_enter_function_children_first() {
        let source = ScriptSource::empty().with_memory_file(
            "ui/test/page",
            layout_bytes_with_root("function InitState(t) table.insert(order, 'root') end", "function InitState(t) table.insert(order, 'button') end"),
        );
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        host.lua.globals().set("order", host.lua.create_table().unwrap()).unwrap();
        host.load_root_layout("ui/test/page").unwrap();
        let order: Vec<String> = host.lua.globals().get::<Table>("order").unwrap().sequence_values().map(Result::unwrap).collect();
        assert_eq!(order, ["button", "root"], "children before their parent");

        // A copy of a component whose current state has an enter function runs it on creation.
        let script = "log = ''\n\
            function InitState(t) log = log .. 'init:' .. t.State .. ';' end\n\
            function Enter(from) log = log .. 'enter<' .. from .. '>;' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| s[0].enter_function = "Enter".into());
        let root = host.root().unwrap();
        let f: Function = host.lua.load("local s, r = ...; return Component.CreateFromComponent(s, 'copy', r, 0, 0)").into_function().unwrap();
        let copy = node_of(&f.call::<Value>((addr(button), addr(root))).unwrap()).expect("created");
        assert_eq!(component_env(&host, copy).get::<String>("log").unwrap(), "init:normal;enter<>;");
        no_errors(&host);
    }

    /// A host hook that supplies an InitState the component does not define (and says so through
    /// `__ntw_has_handler`) reads the new state's geometry when it runs.
    #[test]
    fn a_hook_supplied_init_state_sees_the_new_states_layout() {
        let (host, button) = state_function_page("", PointerEvent::LeftUp, |s| s[1].width = 80);
        let f: Function = host
            .lua
            .load(
                "local button = ...\n\
                 widths = {}\n\
                 local call, has = __ntw_call_if_defined, __ntw_has_handler\n\
                 function __ntw_has_handler(a, name) return (a == button and name == 'InitState') or has(a, name) end\n\
                 function __ntw_call_if_defined(a, name, ...)\n\
                     if a == button and name == 'InitState' then widths[#widths + 1] = UIComponent(a):Width() return true end\n\
                     return call(a, name, ...)\n\
                 end",
            )
            .into_function()
            .unwrap();
        f.call::<()>(addr(button)).unwrap();
        host.pointer(button, PointerEvent::LeftUp);
        let widths: Vec<f64> = host.lua.globals().get::<Table>("widths").unwrap().sequence_values().map(Result::unwrap).collect();
        assert_eq!(widths, [80.0]);
        no_errors(&host);
    }

    /// A Lua global `layouts()` that reads the host's layout count, for scripts of the tests.
    fn expose_layout_counter(host: &UiScriptHost) {
        let count = host.layout_counter();
        host.lua.globals().set("layouts", host.lua.create_function(move |_, ()| Ok(count())).unwrap()).unwrap();
    }

    /// Lazy layout (round 15, the exe's geometry is current on every read): events and changes
    /// lay nothing out by themselves (regression: a key, a hover, every SetVisible and every
    /// transition each laid the whole tree out); the first geometry read after any number of
    /// changes lays out once, and a read with nothing changed costs none.
    #[test]
    fn layout_happens_only_at_a_geometry_read_after_a_change() {
        let script = "function Leave(to) end\nfunction InitState(t) end\nfunction Enter(from) end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            s[0].exit_function = "Leave".into();
            s[1].enter_function = "Enter".into();
            s[1].width = 80;
        });
        let _ = host.world();
        let count = host.layout_counter();
        let before = count();
        host.key("ESCAPE");
        host.hover(Some(button));
        host.pointer(button, PointerEvent::LeftUp);
        host.lua.load("local b = ...; for i = 1, 3 do UIComponent(b):SetVisible(i % 2 == 1) end").into_function().unwrap().call::<()>(addr(button)).unwrap();
        assert_eq!(count() - before, 0, "no read, no layout");
        assert_eq!(host.world().get(button).unwrap().rect.w, 80.0, "the read sees the transition's new state");
        assert_eq!(count() - before, 1, "one layout for all the changes");
        let _ = host.world();
        assert_eq!(count() - before, 1, "nothing changed: no layout");
        no_errors(&host);
    }

    /// Changes that move and resize nothing do not make the next geometry read lay out
    /// (regression: every SetStateText bumped the change counter the layout keyed on, so a loop of
    /// SetStateText + Height laid the whole tree out on every pass).
    #[test]
    fn text_changes_between_geometry_reads_cost_no_layout() {
        let (host, button) = state_function_page("", PointerEvent::LeftUp, |_| {});
        let _ = host.world();
        let count = host.layout_counter();
        let before = count();
        let f: Function = host
            .lua
            .load("local b = ...; local c = UIComponent(b); for i = 1, 10 do c:SetStateText('line ' .. i); c:SetTooltipText('t'); c:SetVisible(i % 2 == 0); h = c:Height() end")
            .into_function()
            .unwrap();
        f.call::<()>(addr(button)).unwrap();
        assert_eq!(count() - before, 0, "no geometry changed: no layout");
        let f: Function = host.lua.load("local b = ...; local c = UIComponent(b); for i = 1, 10 do c:Resize(40 + i, 20); h = c:Width() end").into_function().unwrap();
        f.call::<()>(addr(button)).unwrap();
        assert_eq!(count() - before, 10, "a resize before each read: one layout per read");
        no_errors(&host);
    }

    /// An appearance setter that writes the value already there asks for no redraw (polish: every
    /// SetVisible / SetStateText / SetTooltipText bumped the redraw counter, so a script setting
    /// the same values each frame redrew every frame).
    #[test]
    fn an_unchanged_appearance_value_asks_for_no_redraw() {
        let (host, button) = state_function_page("", PointerEvent::LeftUp, |_| {});
        let set = |v: bool| {
            host.lua
                .load("local b, v = ...; local c = UIComponent(b); c:SetVisible(v); c:SetStateText('same'); c:SetTooltipText('tip'); c:SetInteractive(v)")
                .into_function()
                .unwrap()
                .call::<()>((addr(button), v))
                .unwrap();
            host.world().generation
        };
        let first = set(false);
        assert_eq!(set(false), first, "the same values again: no redraw");
        assert!(set(true) > first, "a new value redraws");
        no_errors(&host);
    }

    /// Debug builds refuse an appearance change that moves or resizes something (it would skip
    /// the layout it needs).
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "an appearance change moved or resized component")]
    fn a_geometry_change_through_update_appearance_is_caught() {
        let mut w = UiWorld::new();
        let mut created = Vec::new();
        let comp = ntw_formats::ui_layout::UiComponent::default();
        let id = w.instantiate(&comp, None, "ui/test", &mut created);
        w.update_appearance(id, |n| {
            n.offset.0 += 1.0;
            true
        });
    }

    /// A second `world()` while one is held does not panic (regression: a pending screen change
    /// made the second call borrow the world mutably while the first borrow was alive); the change
    /// is laid out at the first read after the borrow is gone.
    #[test]
    fn a_second_world_borrow_with_a_screen_change_pending_does_not_panic() {
        let (host, _button) = state_function_page("", PointerEvent::LeftUp, |_| {});
        let root = host.root().unwrap();
        let held = host.world();
        host.set_screen(300.0, 200.0);
        let w = held.get(root).unwrap().rect.w;
        let second = host.world();
        let third = host.world();
        assert_eq!(second.get(root).unwrap().rect.w, w, "laid out later, not now");
        drop((held, second, third));
        let deferred = host.take_log().iter().filter(|l| l.contains("layout deferred")).count();
        assert_eq!(deferred, 1, "the deferred layout is logged once (review: on every read)");
        assert_eq!(host.world().get(root).unwrap().rect.w, 300.0);
        no_errors(&host);
    }

    /// Reads before there is a root clear the stale flags (polish: they stayed set, so every read
    /// tried again); the first root is still laid out for the current screen.
    #[test]
    fn reads_before_the_root_leave_no_stale_flags_and_the_root_is_laid_out() {
        let source = ScriptSource::empty().with_memory_file("ui/test/page", layout_bytes(""));
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        host.set_screen(300.0, 200.0);
        let _ = host.world();
        assert!(!host.inner.layout_stale.get() && !host.inner.world.borrow().layout_dirty);
        let root = host.load_root_layout("ui/test/page").unwrap();
        let r = host.world().get(root).unwrap().rect;
        assert_eq!((r.w, r.h), (300.0, 200.0));
        no_errors(&host);
    }

    /// A script reads every change laid out, also one another script made just before
    /// (regression: the shortcut holder's OnKey ran before the focused component's change was
    /// laid out): the focused component's OnKey moves it to its wider state, the holder reads
    /// the new width.
    #[test]
    fn the_shortcut_holder_reads_the_focused_components_change_laid_out() {
        let source = ScriptSource::empty().with_memory_file(
            "ui/test/page",
            layout_bytes_with_root(
                "UIComponent(Address):StealShortcutKey('ESCAPE')\nfunction OnKey(k, a, up) width = UIComponent(UIComponent(Address):Find('button')):Width() end",
                "function FocusKey(k, a, up) if up then UIComponent(Address):SetState('down') end end\nUIComponent(Address):SetEventCallback('OnKey', FocusKey)",
            ),
        );
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        let root = host.load_root_layout("ui/test/page").unwrap();
        let button = host.world().find(root, "button").unwrap();
        {
            let mut w = host.inner.world.borrow_mut();
            let states = &mut w.get_mut(button).unwrap().data.states;
            states[0].unknown_d4 = 2;
            states[1].width = 80;
        }
        host.lua.load("local b = ...; UIComponent(b):StealInputFocus(true)").into_function().unwrap().call::<()>(addr(button)).unwrap();
        assert!(host.key("ESCAPE"));
        assert_eq!(component_env(&host, root).get::<f64>("width").unwrap(), 80.0);
        no_errors(&host);
    }

    /// InitState runs where there is one to call, own or a hook's that answers
    /// `__ntw_has_handler` (also on a component without scripts); a component with none gets no
    /// InitState setup at all (regression: every new component had its state table built and its
    /// text measured for nothing).
    #[test]
    fn init_state_is_set_up_only_where_there_is_one_to_call() {
        let (host, button) = state_function_page("", PointerEvent::LeftUp, |_| {});
        // A hook on the dispatcher alone: the button defines no InitState, so it is never asked.
        host.lua
            .load("seen = {}\nlocal orig = __ntw_call_if_defined\nfunction __ntw_call_if_defined(a, name, ...) if name == 'InitState' then seen[#seen + 1] = a end return orig(a, name, ...) end")
            .exec()
            .unwrap();
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(host.lua.globals().get::<Table>("seen").unwrap().raw_len(), 0, "nothing to call: no InitState");
        // A hook that answers for it too gets it, also for a copy without a script environment.
        host.lua
            .load(
                "local has = __ntw_has_handler\nfunction __ntw_has_handler(a, name) return name == 'InitState' or has(a, name) end\n\
                 local make = __ntw_make_env\nfunction __ntw_make_env(a) if no_env then return {} end return make(a) end",
            )
            .exec()
            .unwrap();
        host.lua.load("local b = ...; UIComponent(b):SetState('normal')").into_function().unwrap().call::<()>(addr(button)).unwrap();
        let root = host.root().unwrap();
        let f: Function = host
            .lua
            .load("local s, r = ...; no_env = true; local c = Component.CreateFromComponent(s, 'copy', r, 0, 0); no_env = false; return c")
            .into_function()
            .unwrap();
        let copy = node_of(&f.call::<Value>((addr(button), addr(root))).unwrap()).expect("created");
        assert!(host.script_env(copy).is_none(), "the copy has no script environment");
        let seen: Vec<Value> = host.lua.globals().get::<Table>("seen").unwrap().sequence_values().map(Result::unwrap).collect();
        assert!(seen.iter().any(|v| node_of(v) == Some(button)), "the hook's InitState for the button");
        assert!(seen.iter().any(|v| node_of(v) == Some(copy)), "and for the scriptless copy");
        no_errors(&host);
    }

    /// In a batch of new components, an InitState measures its text at the width as it is after
    /// the InitStates before it (regression: the batch was laid out once at its end, so a
    /// parent's InitState measured a rect its child's InitState had changed).
    #[test]
    fn init_state_measures_the_layout_the_earlier_init_states_left() {
        let source = ScriptSource::empty().with_memory_file(
            "ui/test/page",
            layout_bytes_with_root(
                "function InitState(t) parent_at = layouts() end",
                "function InitState(t) UIComponent(UIComponent(Address):Parent()):Resize(90, 90) child_at = layouts() end",
            ),
        );
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        expose_layout_counter(&host);
        let root = host.load_root_layout("ui/test/page").unwrap();
        // The root's state gets a text, so its InitState measures it; a copy of the root is the batch.
        host.inner.world.borrow_mut().get_mut(root).unwrap().data.states[0].text = "text".into();
        let f: Function = host.lua.load("local s = ...; return Component.CreateFromComponent(s, 'copy', s, 0, 0)").into_function().unwrap();
        let copy = node_of(&f.call::<Value>(addr(root)).unwrap()).expect("created");
        let child = host.world().get(copy).unwrap().children[0];
        let child_at: u64 = component_env(&host, child).get("child_at").unwrap();
        let parent_at: u64 = component_env(&host, copy).get("parent_at").unwrap();
        assert!(parent_at > child_at, "laid out between the child's Resize and the parent's measurement: {child_at} -> {parent_at}");
        no_errors(&host);
    }

    /// InitState's image metrics go to the state it was called for, also when it called SetState
    /// (regression: they went to the state SetState moved to).
    #[test]
    fn init_state_metrics_go_to_the_state_it_was_called_for() {
        let script = "function InitState(t) if t.State == 'down' then UIComponent(Address):SetState('normal'); t.Images[1].Width = 99 end end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            for st in s.iter_mut() {
                st.image_metrics.push(ntw_formats::ui_layout::UiImageMetric { width: 5, ..Default::default() });
            }
        });
        host.pointer(button, PointerEvent::LeftUp);
        let w = host.world();
        let n = w.get(button).unwrap();
        assert_eq!(n.state_name(), "normal");
        assert_eq!(n.data.states[1].image_metrics[0].width, 99, "the down state's InitState set its own metrics");
        assert_eq!(n.data.states[0].image_metrics[0].width, 5, "the state SetState moved to is untouched");
        drop(w);
        no_errors(&host);
    }

    /// The enter function of a transition with neither an exit function nor InitState reads the
    /// NEW state's width (every read lays out what changed, `lay_out_if_stale`).
    #[test]
    fn the_enter_function_sees_the_new_states_layout() {
        let script = "widths = ''\n\
            function Enter(from) widths = widths .. UIComponent(Address):Width() .. ';' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            s[1].width = 80;
            s[1].enter_function = "Enter".into();
        });
        component_env(&host, button).set("widths", "").unwrap();
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(button_global::<String>(&host, button, "widths"), "80;");
        no_errors(&host);
    }

    /// A pointer transition into a disabled (+0x115) state runs neither its InitState nor its
    /// enter function: `0x01035620` tests the new state's +0x115 and skips
    /// `CallUIStateInitAndEnterFunction` (`0x0102DF60`) altogether (CONFIRMED, disassembly at
    /// `0x010356A9`). The state's new size is still read at the next geometry read.
    #[test]
    fn a_transition_into_a_disabled_state_runs_no_init_state() {
        let script = "inits = ''\n\
            function InitState(t) inits = inits .. t.State .. ';' end\n\
            function Enter(from) inits = inits .. 'enter;' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            s[1].disabled = true;
            s[1].width = 80;
            s[1].enter_function = "Enter".into();
        });
        component_env(&host, button).set("inits", "").unwrap();
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(host.world().get(button).unwrap().state_name(), "down");
        assert_eq!(button_global::<String>(&host, button, "inits"), "");
        assert_eq!(host.world().get(button).unwrap().rect.w, 80.0);
        no_errors(&host);
    }

    /// A pointer transition runs the old state's exit function and the new state's enter function,
    /// each with the other state's name; SetState runs only the enter function (`0x01035620`,
    /// `0x01035B30`); a disabled old state runs no exit function.
    #[test]
    fn state_changes_run_the_states_enter_and_exit_functions() {
        let script = "log = ''\nfunction Leave(to) log = log .. 'exit:' .. to .. ';' end\nfunction Enter(from) log = log .. 'enter:' .. from .. ';' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            s[0].exit_function = "Leave".into();
            s[1].enter_function = "Enter".into();
            s[1].exit_function = "Leave".into();
        });
        let log = || button_global::<String>(&host, button, "log");
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(host.world().get(button).unwrap().state_name(), "down");
        assert_eq!(log(), "exit:down;enter:normal;");
        let set_state = |s: &str| {
            let f: Function = host.lua.load("local a, s = ...; UIComponent(a):SetState(s)").into_function().unwrap();
            f.call::<()>((addr(button), s)).unwrap();
        };
        set_state("normal");
        set_state("down");
        assert_eq!(log(), "exit:down;enter:normal;enter:normal;", "SetState runs the enter function only");
        set_state("normal");
        host.inner.world.borrow_mut().get_mut(button).unwrap().data.states[0].disabled = true;
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(log(), "exit:down;enter:normal;enter:normal;enter:normal;", "no exit function from a disabled state");
        no_errors(&host);
    }

    /// The click event fires before the transition (`0x0102E340`: event, then
    /// `ApplyUIComponentStateTransition`), so the click handler still sees the old state and the
    /// exit function runs after it; the press is the same (`0x0102E1F0`).
    #[test]
    fn the_click_event_fires_before_the_transition() {
        let script = "log = ''\n\
            function OnLeftClickUp() log = log .. 'click:' .. UIComponent(Address):CurrentState() .. ';' end\n\
            function Leave(to) log = log .. 'exit:' .. UIComponent(Address):CurrentState() .. ';' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| s[0].exit_function = "Leave".into());
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(button_global::<String>(&host, button, "log"), "click:normal;exit:down;");
        no_errors(&host);
    }

    /// The transition starts from the state the click handler left the component in: the click
    /// handler `0x0102E340` fires the event and only then calls `0x01035620`, which reads the
    /// component's current state (+0xAC) on entry (CONFIRMED, disassembly). A handler that already
    /// set "down" leaves no transition from "normal" to run, so no exit function runs; and when the
    /// state the handler set has a transition of its own for the click, that one runs (as a card
    /// whose click handler selected it would leave "Selected" if that state had a left-up
    /// transition; the shipped cards' has none, see campaign_ui
    /// `selecting_an_army_shows_its_unit_cards`).
    #[test]
    fn the_transition_starts_from_the_state_the_click_left() {
        let script = "log = ''\n\
            function OnLeftClickUp() UIComponent(Address):SetState('down') end\n\
            function Leave(to) log = log .. 'exit:' .. to .. ';' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| s[0].exit_function = "Leave".into());
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(host.world().get(button).unwrap().state_name(), "down");
        assert_eq!(button_global::<String>(&host, button, "log"), "");
        no_errors(&host);

        // "down" has a left-up transition back to "normal": the handler's "down" is left at once.
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            let normal = s[0].this;
            s[1].transitions.push(ntw_formats::ui_layout::UiTransition { key: PointerEvent::LeftUp as u32, value: normal, name: String::new(), a: 0, b: 0 });
            s[1].exit_function = "Leave".into();
        });
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(host.world().get(button).unwrap().state_name(), "normal");
        assert_eq!(button_global::<String>(&host, button, "log"), "exit:normal;", "the transition ran from \"down\"");
        no_errors(&host);
    }

    /// A release elsewhere fires no event and a disabled state gets no click event, but both
    /// still apply the transition and run its state functions (`0x0102E340`).
    #[test]
    fn a_release_elsewhere_and_a_disabled_click_still_transition() {
        let script = "log = ''\n\
            function OnLeftClickUp() log = log .. 'click;' end\n\
            function Enter(from) log = log .. 'enter:' .. from .. ';' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUpElsewhere, |s| s[1].enter_function = "Enter".into());
        host.pointer(button, PointerEvent::LeftUpElsewhere);
        assert_eq!(host.world().get(button).unwrap().state_name(), "down");
        assert_eq!(button_global::<String>(&host, button, "log"), "enter:normal;");
        no_errors(&host);
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            s[0].disabled = true;
            s[1].enter_function = "Enter".into();
        });
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(host.world().get(button).unwrap().state_name(), "down");
        assert_eq!(button_global::<String>(&host, button, "log"), "enter:normal;", "no click event from a disabled state");
        no_errors(&host);
    }

    /// The exit function may destroy the component; then nothing more runs on it (the exe checks
    /// it still has its script environment, `0x01035620`), and nothing fails.
    #[test]
    fn a_component_gone_after_its_exit_function_gets_no_enter_function() {
        let script = "log = ''\n\
            function InitState(t) log = log .. 'init:' .. t.State .. ';' end\n\
            function Leave(to) log = log .. 'exit;'; kill() end\n\
            function Enter(from) log = log .. 'enter;' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            s[0].exit_function = "Leave".into();
            s[1].enter_function = "Enter".into();
        });
        let env = component_env(&host, button);
        env.set("log", "").unwrap();
        let inner = host.inner.clone();
        let kill = host.lua.create_function(move |_, ()| {
            inner.world.borrow_mut().destroy(button);
            Ok(())
        });
        env.set("kill", kill.unwrap()).unwrap();
        host.pointer(button, PointerEvent::LeftUp);
        assert!(host.world().get(button).is_none());
        assert_eq!(env.get::<String>("log").unwrap(), "exit;");
        no_errors(&host);
    }

    /// The exit function may move the component to another state with SetState; the enter
    /// function that follows is then the state it is in now (`0x01035620` re-reads the current
    /// state after the exit function).
    #[test]
    fn the_enter_function_is_the_state_the_exit_function_left() {
        let script = "log = ''\n\
            function Leave(to) UIComponent(Address):SetState('normal') end\n\
            function EnterNormal(from) log = log .. 'normal<' .. from .. ';' end\n\
            function EnterDown(from) log = log .. 'down<' .. from .. ';' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| {
            s[0].exit_function = "Leave".into();
            s[0].enter_function = "EnterNormal".into();
            s[1].enter_function = "EnterDown".into();
        });
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(host.world().get(button).unwrap().state_name(), "normal");
        // SetState's own enter call (old state "down"), then the transition's (old state "normal").
        assert_eq!(button_global::<String>(&host, button, "log"), "normal<down;normal<normal;");
        no_errors(&host);
    }

    /// A state function that sets its own state again recurses: no limit of ours (the original
    /// has none), so Lua's nested C-call limit stops it with an error ("C stack overflow", Lua
    /// 5.1 as the original's scripts), logged with the function, state and component; the native
    /// stack grows as needed, so nothing crashes. The host carries on: the next event recurses as
    /// deep again.
    #[test]
    fn a_state_function_that_re_enters_its_own_state_runs_until_luas_call_limit() {
        let script = "n = 0\n\
            function Enter(from) n = n + 1; UIComponent(Address):SetState('down') end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| s[1].enter_function = "Enter".into());
        host.pointer(button, PointerEvent::LeftUp);
        let n = button_global::<i64>(&host, button, "n");
        assert!(n > 100, "no small cap of ours: {n} nested calls");
        let log = host.take_log();
        assert!(
            log.iter().any(|l| l.starts_with("ERROR in Enter (state \"down\")") && l.contains("\"button\"") && l.contains("stack overflow")),
            "{log:?}"
        );
        host.inner.world.borrow_mut().get_mut(button).unwrap().state = 0;
        component_env(&host, button).set("n", 0).unwrap();
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(button_global::<i64>(&host, button, "n"), n, "the next event recurses as deep again");
    }

    /// `state_function_guard`'s cheap path needs the thread's stack measured: on the platforms we
    /// ship, a thread's remaining stack is known, so a state-function call runs in place and only a
    /// deep nest moves to a segment (polish: the unmeasured fallback moves the first call of every
    /// nest to a fresh segment).
    #[test]
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    fn the_native_stack_is_measured_on_the_shipped_platforms() {
        assert!(stacker::remaining_stack().is_some());
        assert!(std::thread::spawn(|| stacker::remaining_stack().is_some()).join().unwrap(), "also on a spawned thread");
    }

    /// Two states whose InitState each sets the other for ever (A → B → A → ...) ping-pong until
    /// Lua's call limit ends it with an error; nothing crashes and the component ends in a state
    /// it has.
    #[test]
    fn an_endless_init_state_ping_pong_ends_with_luas_error_not_a_crash() {
        let script = "inits = 0\n\
            function InitState(t)\n\
                inits = inits + 1\n\
                if t.State == 'normal' then UIComponent(Address):SetState('down') else UIComponent(Address):SetState('normal') end\n\
            end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |_| {});
        host.take_log();
        component_env(&host, button).set("inits", 0).unwrap();
        let f: Function = host.lua.load("local a = ...; UIComponent(a):SetState('normal')").into_function().unwrap();
        f.call::<()>(addr(button)).unwrap();
        assert!(button_global::<i64>(&host, button, "inits") > 100);
        assert!(host.take_log().iter().any(|l| l.starts_with("ERROR") && l.contains("stack overflow")));
        assert!(["normal", "down"].contains(&host.world().get(button).unwrap().state_name()));
    }

    /// A bounded A → B → A runs to completion, as in the original: A's InitState sets B, B's sets
    /// A again, and A's second InitState no longer does; so A's InitState runs twice, no error.
    #[test]
    fn a_bounded_state_ping_pong_runs_to_completion() {
        let script = "inits = ''\n\
            done = false\n\
            function InitState(t)\n\
                inits = inits .. t.State .. ';'\n\
                if t.State == 'normal' and not done then done = true; UIComponent(Address):SetState('down')\n\
                elseif t.State == 'down' then UIComponent(Address):SetState('normal') end\n\
            end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |_| {});
        let env = component_env(&host, button);
        env.set("inits", "").unwrap();
        env.set("done", false).unwrap();
        host.take_log();
        let f: Function = host.lua.load("local a = ...; UIComponent(a):SetState('normal')").into_function().unwrap();
        f.call::<()>(addr(button)).unwrap();
        assert_eq!(button_global::<String>(&host, button, "inits"), "normal;down;normal;");
        assert_eq!(host.world().get(button).unwrap().state_name(), "normal");
        no_errors(&host);
    }

    /// InitState after a pointer transition measures the NEW state's laid-out width.
    #[test]
    fn init_state_after_a_transition_sees_the_new_states_width() {
        let script = "widths = ''\n\
            function InitState(t) widths = widths .. t.State .. '=' .. UIComponent(Address):Width() .. ';' end";
        let (host, button) = state_function_page(script, PointerEvent::LeftUp, |s| s[1].width = 80);
        component_env(&host, button).set("widths", "").unwrap();
        host.pointer(button, PointerEvent::LeftUp);
        assert_eq!(button_global::<String>(&host, button, "widths"), "down=80;");
        assert_eq!(host.world().get(button).unwrap().rect.w, 80.0);
        no_errors(&host);
    }

    /// A click takes the input focus if the state the click STARTS from takes it (`0x010367F0`
    /// asks `0x01029FB0`, the current state's +0xD4, before the transition), not the state it
    /// leads to. (The real text fields: frontend_ui `typed_file_name_saves_the_army`.)
    #[test]
    fn a_click_takes_the_focus_by_the_state_it_starts_from() {
        for (from_takes, to_takes) in [(true, false), (false, true)] {
            let (host, button) = state_function_page("", PointerEvent::LeftUp, |s| {
                s[0].unknown_d4 = if from_takes { 2 } else { 0 };
                s[1].unknown_d4 = if to_takes { 2 } else { 0 };
            });
            host.pointer(button, PointerEvent::LeftUp);
            assert_eq!(host.world().get(button).unwrap().state_name(), "down");
            assert_eq!(host.focus().is_some(), from_takes, "starts in a state that takes the focus: {from_takes}");
            no_errors(&host);
        }
    }

    #[test]
    fn state_text_details_use_the_text_offsets() {
        let script = "local c = UIComponent(Address)\n\
            c:SetStateTextDetails({XOffset = 7, YOffset = 3, HAlign = 'centre', VAlign = 'bottom', HBehaviour = 'NeverSplit'})\n\
            local d = c:GetStateTextDetails()\n\
            got = {d.XOffset, d.YOffset, d.HAlign, d.VAlign, d.HBehaviour}\n\
            c:SetStateTextXOffset(11)\n\
            x2 = c:GetStateTextDetails().XOffset";
        let source = ScriptSource::empty().with_memory_file("ui/test/page", layout_bytes(script));
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        let root = host.load_root_layout("ui/test/page").unwrap();
        let button = host.world().find(root, "button").unwrap();
        let benv = component_env(&host, button);
        let got: Vec<i64> = benv.get("got").unwrap();
        assert_eq!(got, vec![7, 3, 4, 1, 2]);
        assert_eq!(benv.get::<i64>("x2").unwrap(), 11);
        let st = host.world().get(button).unwrap().current().cloned().unwrap();
        assert_eq!((st.text_x_offset, st.text_y_offset, st.text_align, st.text_behaviour.0), (11, 3, (4, 1), 2));
        no_errors(&host);
    }

    #[test]
    fn unknown_engine_calls_log_instead_of_failing() {
        let script = "FrontEnd.SomethingNew(1); UIComponent(Address):SomeMethod(); Component.Whatever()";
        let source = ScriptSource::empty().with_memory_file("ui/test/page", layout_bytes(script));
        let host = UiScriptHost::new(source, Localisation::new(), facts(), (100.0, 100.0)).unwrap();
        host.load_root_layout("ui/test/page").unwrap();
        let log = host.take_log();
        assert!(log.iter().any(|l| l == "UNKNOWN FrontEnd.SomethingNew"), "{log:?}");
        assert!(log.iter().any(|l| l == "UNKNOWN UIComponent:SomeMethod"), "{log:?}");
        assert!(log.iter().any(|l| l == "UNKNOWN Component.Whatever"), "{log:?}");
    }
}

impl UiScriptHost {
    /// The mouse moved: scripts read the position through `Cursor():DistanceToBL()` and
    /// `Component.CursorPosition()` (tooltip and map-cursor placement).
    pub fn set_cursor_position(&self, x: f32, y: f32) {
        let g = self.lua.globals();
        let _ = g.set("__ntw_cursor_x", x);
        let _ = g.set("__ntw_cursor_y", y);
    }

    /// The tooltip text the pointer shows over a component: a script's `SetTooltipText`, else its
    /// current state's text, else the layout's (the order `GetTooltipText` uses).
    pub fn tooltip_text(&self, node: NodeId) -> Option<String> {
        let w = self.world();
        let n = w.get(node)?;
        let t = n.tooltip.clone().unwrap_or_else(|| {
            let s = n.current().map(|s| s.tooltip_text.clone()).unwrap_or_default();
            if s.is_empty() { n.data.tooltip_text.clone() } else { s }
        });
        (!t.is_empty()).then_some(t)
    }

    /// The pointer now rests on `node` (or on nothing): the root layout's `SetTooltipText` shows
    /// that component's tooltip text in the original "Tooltip" template, or the tooltip is hidden
    /// (`__ntw_tooltip`, ui_prelude.lua). Used by the front end and the campaign HUD. The original
    /// draws the registered tooltip object after everything else (`0x01027D20` skips it in the
    /// tree and draws it last, CONFIRMED). PROVISIONAL: shown at once (the hover delay is
    /// UNKNOWN). `campaign_select` hides the tooltip with it before it changes the selection.
    pub fn hover(&self, node: Option<NodeId>) {
        let Some(root) = self.root() else { return };
        let text = node.and_then(|n| self.tooltip_text(n));
        let r = (|| -> mlua::Result<()> {
            let f: Function = self.lua.globals().get("__ntw_tooltip")?;
            f.call::<()>((addr(root), node.map(addr), text))
        })();
        if let Err(e) = r {
            log(&self.inner, format!("ERROR showing a tooltip: {e}"));
        }
    }
}
