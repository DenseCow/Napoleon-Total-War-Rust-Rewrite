-- NapoleonRust UI scripting prelude (our own code, NOT a game file).
--
-- Sets up what the original engine provides to UI scripts, on top of the Rust functions in
-- the table __ui (one per UIComponent method, taking the address first) and __comp (the
-- `Component.*` functions of the running component, address first). Evidence for every rule is
-- in analysis/frontend/UI_SCRIPTING.md; anything without real behaviour logs "UNKNOWN".

local log = __ntw_log
local ui = __ui
local comp = __comp

-- Each component's global environment, keyed by its address (light userdata).
__ntw_envs = {}

-- A table whose every unknown field is a logging stub returning nil.
local function stub_table(name, t)
    return setmetatable(t or {}, {
        __index = function(tbl, key)
            local f = function(...)
                log("UNKNOWN " .. name .. "." .. tostring(key))
                return nil
            end
            rawset(tbl, key, f)
            return f
        end,
    })
end
__ntw_stub_table = stub_table

-- UIComponent(address) -> object with the component methods (CONFIRMED usage: UIComponent(Address),
-- UIComponent(x:Find("id")), obj:Method(...)). Unknown methods log and return nil.
local methods = {}
for name, f in pairs(ui) do
    methods[name] = function(self, ...)
        return f(rawget(self, "__addr"), ...)
    end
end
-- Globals of another component (CONFIRMED usage in root.lua / main.main.lua).
function methods:SetGlobal(name, value)
    local env = __ntw_envs[rawget(self, "__addr")]
    if env then env[name] = value end
end
function methods:GlobalExists(name)
    local env = __ntw_envs[rawget(self, "__addr")]
    if env then return rawget(env, name) end
    return nil
end
-- TooltipTextSet(text): the tooltip text property's setter, called through Component.CallByAddress
-- (CONFIRMED name in the property table 0x0103A040 next to the tooltip property; INFERRED to
-- set the same text as SetTooltipText).
function methods:TooltipTextSet(text, ...)
    return ui.SetTooltipText(rawget(self, "__addr"), text, ...)
end
-- obj:SequentialFind("a", "b", ...) returns a component object (CONFIRMED usage:
-- settlement_captured.lua calls SetState on the result directly; wrapping it again with
-- UIComponent() returns the same object, so scripts that wrap it still work).
function methods:SequentialFind(...)
    local a = ui.SequentialFind(rawget(self, "__addr"), ...)
    if a == nil then return nil end
    return UIComponent(a)
end
function methods:LuaCall(name, ...)
    local env = __ntw_envs[rawget(self, "__addr")]
    local f = env and rawget(env, name)
    if type(f) == "function" then return f(...) end
    log("LuaCall: no function " .. tostring(name))
    return nil
end
-- SetEventCallback(event, fn): fn is a function or the NAME of one of the component's globals
-- (CONFIRMED: template.heading.lua passes "OnUpdate"). Mouse events bound this way make the
-- component react to the mouse like a layout binding does.
function methods:SetEventCallback(event, fn)
    local a = rawget(self, "__addr")
    local env = __ntw_envs[a]
    if env then
        local cbs = rawget(env, "__ntw_callbacks")
        if not cbs then cbs = {}; rawset(env, "__ntw_callbacks", cbs) end
        cbs[event] = fn
        ui.__MarkEvent(a, event)
    end
end

-- Properties: the layout's UserProperties (strings) plus values set at run time with
-- SetProperty (CONFIRMED names: maxValue, Value, Notify, stepSize, ...). INFERRED: numeric
-- strings read as numbers, other text as is (template.vslider.lua compares its "Inverted"
-- property both with 0 and with "true"); the second argument (true there) is UNKNOWN.
__ntw_props = {}
local function get_property(a, key, as_bool)
    local p = __ntw_props[a]
    if p and p[key] ~= nil then return p[key] end
    local v = ui.GetProperty(a, key)
    if type(v) == "string" then
        local n = tonumber(v)
        if n ~= nil then return n end
    end
    return v
end
local function set_property(a, key, value)
    local p = __ntw_props[a]
    if not p then p = {}; __ntw_props[a] = p end
    p[key] = value
end
function methods:GetProperty(key, as_bool) return get_property(rawget(self, "__addr"), key, as_bool) end
function methods:SetProperty(key, value) set_property(rawget(self, "__addr"), key, value) end

-- Shaders: ShaderTechniqueSet(name, ...) picks a component's pixel effect (e.g. "grey_over_time_t0",
-- "glow_pulse_t0") and ShaderVarsSet(a, b, c, d) / Component.ShaderVarsGet() its four variables
-- (CONFIRMED usage: template.RecruitmentCard.lua reads them back as start/end greyscale and
-- start/end time). INFERRED: the values are only stored; PROVISIONAL: our renderer draws no shader
-- effects yet.
__ntw_shader = {}
function methods:ShaderTechniqueSet(name, ...)
    local a = rawget(self, "__addr")
    local s = __ntw_shader[a] or { vars = { 0, 0, 0, 0 } }
    s.technique = name
    __ntw_shader[a] = s
end
function methods:ShaderVarsSet(a1, a2, a3, a4)
    local a = rawget(self, "__addr")
    local s = __ntw_shader[a] or { vars = { 0, 0, 0, 0 } }
    s.vars = { a1 or 0, a2 or 0, a3 or 0, a4 or 0 }
    __ntw_shader[a] = s
end
-- TextShaderTechniqueSet(name) / TextShaderVarsSet(a, b, c, d): the same for the component's text
-- (CONFIRMED calls in objectives_screens.lua); stored, not drawn (PROVISIONAL).
function methods:TextShaderTechniqueSet(name, ...)
    local a = rawget(self, "__addr")
    local s = __ntw_shader[a] or { vars = { 0, 0, 0, 0 } }
    s.text_technique = name
    __ntw_shader[a] = s
end
function methods:TextShaderVarsSet(a1, a2, a3, a4)
    local a = rawget(self, "__addr")
    local s = __ntw_shader[a] or { vars = { 0, 0, 0, 0 } }
    s.text_vars = { a1 or 0, a2 or 0, a3 or 0, a4 or 0 }
    __ntw_shader[a] = s
end
function __ntw_shader_vars(a)
    local s = __ntw_shader[a]
    if s == nil then return 0, 0, 0, 0 end
    return s.vars[1], s.vars[2], s.vars[3], s.vars[4]
end
-- Engine state the mouse UI does not use: drag-and-drop permission and input priorities
-- (CONFIRMED method names; no visible effect in our renderer).
function methods:SetMoveable(on) end
function methods:PropagatePriority(p) end

-- UIComponent:Layout(): arranges a list's children. INFERRED engine behaviour: the engine builds
-- one table per shown child ({Address, Width, Height, X, Y}) and passes the array to the
-- component's Lua Layout hook (template.list.lua's Layout(items) sets item.Y and adds each
-- Address to its selection manager), then moves every child to its Y below the list's top.
-- Without a hook the children are stacked top to bottom (PROVISIONAL).
function methods:Layout()
    local a = rawget(self, "__addr")
    local items = {}
    for i = 0, ui.ChildCount(a) - 1 do
        local c = ui.Find(a, i)
        if c ~= nil and ui.Visible(c) then
            local w, h = ui.Dimensions(c)
            items[#items + 1] = { Address = c, Width = w, Height = h, X = 0, Y = 0 }
        end
    end
    local env = __ntw_envs[a]
    local hook = env and rawget(env, "Layout")
    if type(hook) == "function" then
        hook(items)
    else
        local y = 0
        for _, it in ipairs(items) do it.Y = y; y = y + it.Height end
    end
    -- A hook that sets X places children horizontally too (CardGroup.lua lays unit cards out ten
    -- to a row with item.X / item.Y, INFERRED); hooks that leave every X at 0 keep each x.
    local left, top = ui.Position(a)
    local uses_x = false
    for _, it in ipairs(items) do
        if it.X ~= nil and it.X ~= 0 then uses_x = true end
    end
    for _, it in ipairs(items) do
        local x = ui.Position(it.Address)
        if uses_x then x = left + (it.X or 0) end
        ui.MoveTo(it.Address, x, top + (it.Y or 0))
    end
end

-- An unknown method name calls the component's own Lua global of that name, if it has one
-- (INFERRED: sp_load_game.lua calls list_box:Layout(), and template.list.lua defines a global
-- Layout in the list's environment; the same holds for ChildList, SelectedItem, ...).
local component_mt = {
    __index = function(self, key)
        local m = methods[key]
        if m then return m end
        return function(obj, ...)
            local a = type(obj) == "table" and rawget(obj, "__addr") or rawget(self, "__addr")
            local env = __ntw_envs[a]
            local f = env and rawget(env, key)
            if type(f) == "function" then return f(...) end
            log("UNKNOWN UIComponent:" .. tostring(key))
            return nil
        end
    end,
    __eq = function(a, b) return rawget(a, "__addr") == rawget(b, "__addr") end,
    __tostring = function(self) return "UIComponent(" .. tostring(rawget(self, "__addr")) .. ")" end,
}

function UIComponent(address)
    if address == nil then
        log("UIComponent(nil)")
        return nil
    end
    if type(address) == "table" and rawget(address, "__addr") then return address end
    return setmetatable({ __addr = address }, component_mt)
end

-- dofile inside a component script runs the file in that component's environment
-- (INFERRED: template scripts define Initialise/InitState for the component that loads them).
function dofile(path)
    local env = getfenv(2)
    local f, err = __ntw_loadfile(path)
    if not f then
        log("dofile: " .. tostring(err))
        return nil
    end
    setfenv(f, env)
    return f()
end

-- Resolves a dotted component path relative to `address`: "Parent", "Root", "" (itself) or a
-- child id (found anywhere below). Returns the address or nil.
local function walk_path(address, segments, count)
    local a = address
    for i = 1, count do
        local s = segments[i]
        if a == nil then return nil end
        if s == "Parent" then a = ui.Parent(a)
        elseif s == "Root" then a = comp.Root(a)
        elseif string.match(s, "^Children%[%d+%]$") then
            -- "Children[n]": the n-th child, 0-based (CONFIRMED path strings in template.listview.lua).
            a = ui.Find(a, tonumber(string.match(s, "%d+")))
        elseif s ~= "" then a = ui.Find(a, s) end
    end
    return a
end
local function split_path(path)
    local segs = {}
    for s in string.gmatch(path .. ".", "([^%.]*)%.") do segs[#segs + 1] = s end
    return segs
end
-- Component.Call("path.Method", ...): calls Method on the component the path names
-- (CONFIRMED strings: "Parent.Height", "Root.LuaCall", ".LuaCall", "vslider.Address", "Id").
local function component_call(address, path, ...)
    local segs = split_path(path)
    local target = walk_path(address, segs, #segs - 1)
    if target == nil then
        log("Component.Call: no component for " .. tostring(path))
        return nil
    end
    local obj = UIComponent(target)
    return obj[segs[#segs]](obj, ...)
end
-- Component.GetProperty("path.Key") / SetProperty("path.Key", value) (CONFIRMED strings:
-- "Parent.Value", "vslider.Notify", "ChildIndex").
local function component_get_property(address, path, as_bool)
    local segs = split_path(path)
    local target = walk_path(address, segs, #segs - 1)
    if target == nil then return nil end
    return get_property(target, segs[#segs], as_bool)
end
local function component_set_property(address, path, value)
    local segs = split_path(path)
    local target = walk_path(address, segs, #segs - 1)
    if target ~= nil then set_property(target, segs[#segs], value) end
end
local component_lua = {
    ShaderVarsGet = function(address) return __ntw_shader_vars(address) end,
    -- Component.RegisterTooltipObject(c): the component the engine shows as the tooltip (CONFIRMED
    -- call in the campaign root's SetTooltipMethod; INFERRED: the engine then shows and hides it).
    RegisterTooltipObject = function(address, obj)
        if type(obj) == "table" then obj = rawget(obj, "__addr") end
        __ntw_tooltip_object = obj
    end,
    -- Component.CursorPosition() → x, y, screen width, screen height (Utilities.PositionTooltip
    -- takes four results and keeps tooltips inside the last two; INFERRED). PROVISIONAL: screen
    -- pixels, while the campaign HUD's Position / MoveTo work in the 1280x960 scripts' frame.
    CursorPosition = function(address)
        local w, h = __ntw_screen_size()
        return __ntw_cursor_x, __ntw_cursor_y, w, h
    end,
    -- Component.Messages.<Name>: message ids passed to Notify functions (CONFIRMED usage
    -- Component.Messages.Move in template.TabGroup.lua); INFERRED: any name is its own id.
    Messages = setmetatable({}, { __index = function(t, k) return k end }),
    -- Component.SequentialFind("a", "b", ...): searched from the top root (INFERRED: audio.lua finds
    -- the options page's "voice_settings" panel, which is not inside the audio panel).
    -- It returns a component object, not an address (audio.lua calls methods on the result).
    SequentialFind = function(address, ...)
        local a = ui.SequentialFind(comp.Root(address), ...)
        if a == nil then return nil end
        return UIComponent(a)
    end,
    Call = component_call,
    GetProperty = component_get_property,
    SetProperty = component_set_property,
    -- Component.CallByAddress(address, "Method", ...) (CONFIRMED usage).
    CallByAddress = function(_, target, method, ...)
        local obj = UIComponent(target)
        if obj == nil then return nil end
        return obj[method](obj, ...)
    end,
}

-- Creates a component's environment: its own globals, falling back to the shared ones.
function __ntw_make_env(address)
    local env = setmetatable({}, { __index = _G })
    env.Address = address
    env.Component = setmetatable({}, {
        __index = function(t, key)
            local f = component_lua[key] or comp[key]
            if type(f) == "table" then return f end
            if f then
                return function(...) return f(address, ...) end
            end
            return function(...)
                log("UNKNOWN Component." .. tostring(key))
                return nil
            end
        end,
    })
    __ntw_envs[address] = env
    if __ntw_root_env == nil then __ntw_root_env = env end
    return env
end

-- INFERRED: modules the scripts require (the campaign HUD's Recruitment.lua, Agents.lua, ...;
-- Utilities.lua's PositionTooltip, which the front end's and the campaign's root.lua call) use
-- the global `Component` outside any component environment, so the engine must provide one there.
-- It is bound to the first component environment created (the layout's root): `next(__ntw_envs)`
-- would pick any environment, e.g. a destroyed unit card's.
Component = setmetatable({}, {
    __index = function(t, key)
        local env = __ntw_root_env
        if env == nil then return nil end
        return env.Component[key]
    end,
})

-- Runs a loaded chunk in a component's environment.
function __ntw_run_in(env, f)
    setfenv(f, env)
    return f()
end

-- Calls a component's event function: a callback set with SetEventCallback first, else the
-- function bound in the layout. Returns true if something was called.
function __ntw_fire(address, event, func_name, ...)
    local env = __ntw_envs[address]
    if not env then return false end
    local cbs = rawget(env, "__ntw_callbacks")
    local f = cbs and cbs[event]
    -- Layout bindings can also be a call on another component, e.g. "call Root.LuaCall, Quit" or
    -- "call Parent.LuaCall,PlayBattle" (CONFIRMED strings in the front-end layouts; meaning INFERRED:
    -- <target>.<method>, then comma-separated string arguments).
    if f == nil and func_name ~= nil then
        local target, method, rest = string.match(func_name, "^call%s+(%a+)%.(%a+)%s*,?(.*)$")
        if target then
            local addr
            if target == "Root" then addr = comp.Root(address)
            elseif target == "Parent" then addr = ui.Parent(address)
            else addr = address end
            local obj = UIComponent(addr)
            if obj == nil then return false end
            local args = {}
            for a in string.gmatch(rest, "[^,]+") do
                local s = string.match(a, "^%s*(.-)%s*$")
                if s ~= "" then args[#args + 1] = s end
            end
            obj[method](obj, unpack(args))
            return true
        end
    end
    if f == nil and func_name ~= nil and func_name ~= "" then f = env[func_name] end
    -- A callback given by name (SetEventCallback("OnUpdatePulse", "OnUpdate")).
    if type(f) == "string" then f = env[f] end
    if type(f) ~= "function" then return false end
    f(...)
    return true
end

-- Whether __ntw_call_if_defined(address, name, ...) would run a script. The host asks it before
-- InitState and skips the call's setup (the state table, the text measurement) when it says no;
-- a host that hooks __ntw_call_if_defined to supply a function of its own must answer for it here too.
function __ntw_has_handler(address, name)
    local env = __ntw_envs[address]
    return env ~= nil and type(rawget(env, name)) == "function"
end

-- Calls a named global of a component if it defines one (InitState, state enter / exit functions,
-- root functions); returns whether it did.
function __ntw_call_if_defined(address, name, ...)
    local env = __ntw_envs[address]
    local f = env and rawget(env, name)
    if type(f) ~= "function" then return false end
    f(...)
    return true
end

-- Tooltips (INFERRED engine side; the root scripts' part CONFIRMED in the front end's and the
-- campaign's root.lua): when the pointer rests on a component with tooltip text, the engine passes
-- it to the root layout's SetTooltipText(component, text), which creates the "Tooltip" template
-- once (Component.CreateComponentFromTemplate), registers it (Component.RegisterTooltipObject),
-- places it (Utilities.PositionTooltip) and has it show the text (its SetText). The engine then
-- shows the registered object and hides it when the pointer leaves.
function __ntw_tooltip(root, owner, text)
    local tip = __ntw_tooltip_object
    if text == nil or text == "" then
        if tip ~= nil then __ui.SetVisible(tip, false) end
        return
    end
    __ntw_call_if_defined(root, "SetTooltipText", owner, text)
    tip = __ntw_tooltip_object
    if tip == nil then return end
    __ui.SetVisible(tip, true)
    for k, v in pairs(package.loaded) do
        if type(k) == "string" and string.lower(k) == "utilities" and type(v) == "table" and v.PositionTooltip then
            v.PositionTooltip(tip)
            break
        end
    end
end

-- Like __ntw_call_if_defined, and returns the function's first result.
function __ntw_call_get(address, name, ...)
    local env = __ntw_envs[address]
    local f = env and rawget(env, name)
    if type(f) ~= "function" then return false, nil end
    return true, f(...)
end

-- Engine tables. Functions not implemented in Rust log UNKNOWN and return nil.
FrontEnd = stub_table("FrontEnd", __frontend)
CampaignUI = stub_table("CampaignUI")
BattleUI = stub_table("BattleUI")
mp_interface = stub_table("mp_interface")
system = stub_table("system", { ClearRequiredFiles = function() end })
-- Build flags such as `defined.demo` (nil in the retail game; INFERRED).
defined = {}

-- UIImage(path) loads an image file; image:SetComponentTexture(component, index) puts it on the
-- component's n-th image (CONFIRMED usage in sp_load_game.lua: UIImage("data/" .. portrait)).
-- image:Dimensions() → width, height in pixels (CONFIRMED use in template.map_image.lua).
-- UIImage.<Name>(...) are look-ups (FlagPath, IconFilename, ...) filled in __uiimage by Rust.
local function component_address(component)
    return type(component) == "table" and rawget(component, "__addr") or component
end
local image_methods = {
    SetComponentTexture = function(self, component, index)
        if self.path == nil or self.path == "" then return end
        local p = string.gsub(self.path or "", "^[Dd][Aa][Tt][Aa][/\]", "")
        ui.__SetImagePath(component_address(component), index or 0, p)
    end,
    Dimensions = function(self)
        if self.path == nil then return 0, 0 end
        local w, h = __ntw_image.size(self.path)
        if w == nil then
            log("UIImage: no file " .. tostring(self.path))
            return 0, 0
        end
        return w, h
    end,
    -- Reference counting of the original's texture manager: nothing to do here.
    UnManage = function() end,
    Release = function() end,
    Free = function() end,
}
-- UIPaletisedImage(path): an 8-bit palette image (a campaign map's <map>_lookup.tga) whose palette
-- the script recolours: Query(x, y) → palette index under a pixel, SetPaletteEntry(i, r, g, b, a)
-- (CONFIRMED calls in template.map_image.lua; see ui/image.rs). nil when the file is missing.
local paletised_methods = {
    SetComponentTexture = function(self, component, index)
        if self.key == nil then return end
        ui.__SetImagePath(component_address(component), index or 0, "__runtime/" .. self.key)
    end,
    Dimensions = function(self) return self.w or 0, self.h or 0 end,
    Query = function(self, x, y)
        if self.key == nil then return nil end
        return __ntw_image.pal_query(self.key, x, y)
    end,
    SetPaletteEntry = function(self, i, r, g, b, a)
        if self.key ~= nil then __ntw_image.pal_set(self.key, i, r, g, b, a) end
    end,
    Release = function(self)
        if self.key ~= nil then __ntw_image.pal_release(self.key) end
        self.key = nil
    end,
    UnManage = function() end,
}
function UIPaletisedImage(path)
    local key, w, h = __ntw_image.pal_load(tostring(path))
    if key == nil then
        log("UIPaletisedImage: no palette image " .. tostring(path))
        return nil
    end
    return setmetatable({ path = path, key = key, w = w, h = h }, { __index = paletised_methods })
end
UIImage = setmetatable(__uiimage or {}, {
    __call = function(_, path)
        return setmetatable({ path = path }, { __index = image_methods })
    end,
    __index = function(t, key)
        local f = function(...)
            log("UNKNOWN UIImage." .. tostring(key))
            return nil
        end
        rawset(t, key, f)
        return f
    end,
})

-- Cursor("busy") returns a cursor object; SetMode("normal") puts the normal cursor back
-- (CONFIRMED usage in root.lua's TransitionTo). The host reads __ntw_cursor.
__ntw_cursor = nil
-- The mouse position the host last reported (screen pixels, top-left origin).
__ntw_cursor_x, __ntw_cursor_y = 0, 0
function Cursor(name)
    if name ~= nil then __ntw_cursor = name end
    return {
        SetMode = function(self, mode)
            if mode == "normal" then __ntw_cursor = nil else __ntw_cursor = mode end
        end,
        -- Cursor():DistanceToBL() → x, y from the pointer's hot spot to the bottom-left corner of
        -- the cursor image (CONFIRMED call: Utilities.PositionTooltip adds it to the pointer position
        -- to place a tooltip under the cursor; meaning INFERRED). PROVISIONAL: a 32-pixel arrow.
        DistanceToBL = function(self) return 0, 32 end,
        Position = function(self) return __ntw_cursor_x, __ntw_cursor_y end,
    }
end

-- UISelectionManager(multi_select): the engine's selection helper used by list templates
-- (CONFIRMED methods from template.list.lua: Add, Remove, RemoveAll, Selected, ManageSelection,
-- DeselectAll, UnManage, CanBeSelected). INFERRED behaviour: items are addresses; selecting
-- calls the item's Lua Select(true/false), which sets its selected/unselected look
-- (template.campaign_save_game.lua). PROVISIONAL: modifier keys only matter for multi-select.
function UISelectionManager(multi)
    local m = { items = {}, selected = {}, multi = multi and true or false }
    local function notify(a, on)
        local env = __ntw_envs[a]
        local f = env and rawget(env, "Select")
        if type(f) == "function" then f(on) end
    end
    local function index_of(list, a)
        for i, v in ipairs(list) do if v == a then return i end end
        return nil
    end
    function m:Add(a) if not index_of(self.items, a) then table.insert(self.items, a) end end
    function m:Remove(a)
        local i = index_of(self.items, a); if i then table.remove(self.items, i) end
        local j = index_of(self.selected, a); if j then table.remove(self.selected, j) end
    end
    m.UnManage = m.Remove
    function m:RemoveAll() self.items = {}; self.selected = {} end
    function m:Selected()
        local out = {}
        for i, v in ipairs(self.selected) do out[i] = v end
        return out
    end
    function m:CanBeSelected(a) return index_of(self.items, a) ~= nil end
    function m:DeselectAll()
        for _, v in ipairs(self.selected) do notify(v, false) end
        self.selected = {}
    end
    function m:ManageSelection(a, toggle, range, extra)
        if a == nil then return false end
        local was = index_of(self.selected, a) ~= nil
        if self.multi and toggle and was then
            table.remove(self.selected, index_of(self.selected, a))
            notify(a, false)
            return true
        end
        if was and #self.selected == 1 then return false end
        if not (self.multi and toggle) then
            for _, v in ipairs(self.selected) do if v ~= a then notify(v, false) end end
            self.selected = {}
        end
        if not index_of(self.selected, a) then table.insert(self.selected, a) end
        notify(a, true)
        return true
    end
    return m
end

-- UICardManager(?, multi): the engine's card-group helper (CONFIRMED methods from the campaign
-- HUD's CardGroup.lua and the front end's army_box.lua: AddCard, RemoveCard, RemoveAll, Cards,
-- Selected, ManageSelection, SetAsActive, SetInactive, DragSelected). INFERRED behaviour: cards
-- are addresses; a selected card is put in its "Selected" state, the others in "Default" (the
-- states of template CampaignUnitCard), all "Inactive" while the group is inactive; after a
-- selection change the owning group's Lua SelectionChanged() runs. PROVISIONAL: a click selects
-- that card alone (the original's shift/ctrl rules are not read yet); the first argument is
-- UNKNOWN.
function UICardManager(_, multi)
    local m = { cards = {}, selected = {}, multi = multi and true or false, active = true }
    local function index_of(list, a)
        for i, v in ipairs(list) do if v == a then return i end end
        return nil
    end
    local function look(a)
        local want
        if not m.active then want = "Inactive"
        elseif index_of(m.selected, a) then want = "Selected"
        else want = "Default" end
        local cur = ui.CurrentState(a)
        -- Only on a change: SetState runs the state's enter function every time (CONFIRMED,
        -- 0x01035B30), also into the state the card is already in.
        if cur ~= want and cur ~= "UnSelectable" and cur ~= "NonInteractive" then ui.SetState(a, want) end
    end
    local function changed(a)
        local parent = a and ui.Parent(a)
        local env = parent and __ntw_envs[parent]
        local f = env and rawget(env, "SelectionChanged")
        if type(f) == "function" then f() end
    end
    function m:AddCard(a)
        if a ~= nil and not index_of(self.cards, a) then table.insert(self.cards, a); look(a) end
    end
    function m:RemoveCard(a)
        local i = index_of(self.cards, a); if i then table.remove(self.cards, i) end
        local j = index_of(self.selected, a); if j then table.remove(self.selected, j) end
    end
    function m:RemoveAll() self.cards = {}; self.selected = {} end
    function m:Cards()
        local out = {}
        for i, v in ipairs(self.cards) do out[i] = v end
        return out
    end
    function m:Selected()
        local out = {}
        for i, v in ipairs(self.selected) do out[i] = v end
        return out
    end
    function m:SetAsActive()
        self.active = true
        for _, c in ipairs(self.cards) do look(c) end
    end
    function m:SetInactive()
        self.active = false
        for _, c in ipairs(self.cards) do look(c) end
    end
    function m:ManageSelection(a, released)
        if a == nil or not index_of(self.cards, a) or not released then return false end
        self.selected = { a }
        for _, c in ipairs(self.cards) do look(c) end
        changed(a)
        return true
    end
    function m:DragSelected() end
    return m
end

out = setmetatable({}, {
    __index = function(t, channel)
        local f = function(...)
            local parts = {}
            for i = 1, select("#", ...) do
                parts[#parts + 1] = tostring((select(i, ...)))
            end
            log("out." .. tostring(channel) .. ": " .. table.concat(parts, " "))
        end
        rawset(t, channel, f)
        return f
    end,
})

function print(...)
    local parts = {}
    for i = 1, select("#", ...) do
        parts[#parts + 1] = tostring((select(i, ...)))
    end
    log("print: " .. table.concat(parts, "\t"))
end

-- UIPrefsInterface(game_core, mp, dropin): the options object (CONFIRMED class and method names,
-- registration 0x00DA99D0, method table 0x01459B10). CurrentOptions() returns one table per
-- section (CONFIRMED section names game/audio/graphics/ui/gamma/voicechat/controls, and the
-- field names read by Set*Options in the exe). Each field is stored under a key of the original's
-- preferences.script.txt; the field -> key pairing below is INFERRED from the names.
-- kind: "b" bool, "n" number, "!b" inverted bool, "diff" difficulty (slider 0..3 = easy..very
-- hard, stored as 1..-2: INFERRED from the loc texts difficulty_level_1..4 = Easy..Very Hard).
local pref_fields = {
    game = {
        { "campaign_difficulty", "campaign_difficulty", "diff" },
        { "battle_difficulty", "battle_difficulty", "diff" },
        { "autoresolve_difficulty", "autoresolve_difficulty", "diff" },
        { "battle_time_limit", "battle_time_limit", "n" },
        { "CPU_moves", "show_cpu_moves", "b" },
        { "city_management", "automanage_regions", "b" },
        { "limitless_ammo", "limitless_ammo", "b" },
        { "drop_in_battles", "allow_drop_in_battles", "b" },
        { "campaign_advice", "campaign_advice_level", "n" },
        { "battle_advice", "battle_advice_level", "n" },
    },
    ui = {
        { "minimised_ui", "minimised_ui", "b" },
        { "selection", "show_selection_markers", "b" },
        { "paths", "show_path_markers", "b" },
        { "target_zones", "show_target_zones", "b" },
        { "orders", "ui_order_button_mode", "n" },
        { "cards", "ui_card_mode", "n" },
        { "radar", "ui_radar_mode", "n" },
        { "land_ids", "ui_land_unit_ids", "n" },
        { "naval_ids", "ui_naval_unit_ids", "n" },
        { "PIP", "gfx_picture_in_picture", "b" },
    },
    audio = {
        { "master", "sound_master_volume", "n" },
        { "music", "sound_music_volume", "n" },
        { "speech", "sound_speech_volume", "n" },
        { "effects", "sound_sfx_volume", "n" },
        { "mute_master", "sound_master_enabled", "!b" },
        { "mute_music", "sound_music_enabled", "!b" },
        { "mute_speech", "sound_speech_enabled", "!b" },
        { "mute_effects", "sound_sfx_enabled", "!b" },
        { "sound_provider", "sound_provider", "n" },
        { "channels", "sound_channels", "n" },
        { "variation", "sound_variations", "n" },
        { "memory", "audio_memory_size_in_megabytes", "n" },
        { "caching", "sound_file_caching", "b" },
        { "subtitles", "subtitles", "b" },
    },
    gamma = {
        { "gamma", "gfx_gamma_setting", "n" },
        { "brightness", "gfx_brightness_setting", "n" },
    },
    voicechat = {
        { "volume", "voice_chat_volume", "n" },
        { "gain", "voice_chat_microphone_gain", "n" },
        { "boost", "voice_chat_microphone_gain_boost", "b" },
        { "quality", "voice_chat_quality", "n" },
        { "push_to_talk", "voice_chat_transmit_only_when_key_pressed", "b" },
    },
    graphics = {
        { "aliasing", "gfx_aa", "n" },
        { "filtering", "gfx_texture_filtering", "n" },
        { "texture_quality", "gfx_texture_quality", "n" },
        { "sky", "gfx_sky_quality", "n" },
        { "unit_detail", "gfx_unit_quality", "n" },
        { "ships", "gfx_ship_quality", "n" },
        { "building_detail", "gfx_building_quality", "n" },
        { "water", "gfx_water_quality", "n" },
        { "unit_size", "gfx_unit_scale", "n" },
        { "shadows", "gfx_shadow_quality", "n" },
        { "trees", "gfx_tree_quality", "n" },
        { "grass", "gfx_grass_quality", "n" },
        { "particle_effects", "gfx_effects_quality", "n" },
        { "shader", "gfx_shadermodel", "n" },
        { "HDR", "gfx_hdr", "b" },
        { "SSAO", "gfx_ssao", "b" },
        { "depth_of_field", "gfx_depth_of_field", "b" },
        { "distortion_fx", "gfx_distortion", "b" },
        { "hardware_shadows", "gfx_hardware_shadows", "b" },
        { "volumetric", "gfx_volumetric_effect", "b" },
        { "windowed", "gfx_fullscreen", "!b" },
        { "v-sync", "gfx_vsync", "b" },
    },
}
__ntw_pref_fields = pref_fields
local function pref_read(kind, text)
    if text == nil then return nil end
    if kind == "b" or kind == "!b" then
        local v = (text == "true" or (tonumber(text) or 0) ~= 0)
        if kind == "!b" then v = not v end
        return v
    end
    local n = tonumber(text)
    if n == nil then return nil end
    if kind == "diff" then return 1 - n end
    return n
end
local function pref_write(kind, value)
    if kind == "b" then return value and true or false end
    if kind == "!b" then return not value end
    if kind == "diff" then return 1 - (tonumber(value) or 0) end
    return tonumber(value)
end
local function section_options(name)
    local t = {}
    for _, f in ipairs(pref_fields[name] or {}) do
        t[f[1]] = pref_read(f[3], __prefs.get(f[2]))
    end
    return t
end
local function set_section(name, t)
    if type(t) ~= "table" then return end
    for _, f in ipairs(pref_fields[name] or {}) do
        local v = t[f[1]]
        if v ~= nil then
            local w = pref_write(f[3], v)
            if w ~= nil then __prefs.set(f[2], w) end
        end
    end
    __prefs.save()
end
function UIPrefsInterface(core, mp, dropin)
    local o = {}
    function o:CurrentOptions()
        local all = {}
        for name, _ in pairs(pref_fields) do all[name] = section_options(name) end
        -- The audio section carries the voice-chat settings too (CONFIRMED: SetAudioOptions reads
        -- "voicechat"; audio.lua hands options.voicechat to its voice panel).
        all.audio.voicechat = all.voicechat
        -- The selected entry of EnumerateScreenModes (PROVISIONAL: the only one listed).
        all.graphics.screen_mode = 0
        -- UNKNOWN: the key sets (custom_keys.keys.xml); an empty mapping keeps controls.lua running.
        -- controls: camera speeds and toggles from the preferences (INFERRED pairing), the key sets
        -- (custom_keys.keys.xml and the shipped ones) are UNKNOWN: empty, so no keys are listed.
        all.controls = {
            move_speed = tonumber(__prefs.get("camera_move_speed")),
            rotation_speed = tonumber(__prefs.get("camera_turn_speed")),
            mouse_scroll = pref_read("b", __prefs.get("ui_mouse_scroll")),
            free_cam = (tonumber(__prefs.get("default_camera_type")) or 0) ~= 0,
            CurrentKeyset = "total_war",
            Keysets = { total_war = {}, fps = {}, custom = {} },
            mapping = {},
        }
        return all
    end
    function o:SetGameOptions(t) set_section("game", t) end
    function o:SetUIOptions(t) set_section("ui", t) end
    function o:SetAudioOptions(t)
        set_section("audio", t)
        if type(t) == "table" and type(t.voicechat) == "table" then set_section("voicechat", t.voicechat) end
    end
    function o:SetGammaOptions(t) set_section("gamma", t) end
    function o:SetGraphicsOptions(t) set_section("graphics", t) end
    function o:SetVoiceChatOptions(t) set_section("voicechat", t) end
    function o:SetControlOptions(t) end
    function o:SetAudioVolume() end
    function o:SetVoiceChatGain() end
    function o:EnableTestVoiceChat() end
    function o:BattleDifficulty() return section_options("game").battle_difficulty end
    function o:LocalisationString(key) return FrontEnd.LocalisationString(key) end
    -- PROVISIONAL: no list of video modes / quality presets / sound devices yet.
    function o:AvailableGfxOptions() return {}, {} end
    -- AvailableGfxQualities() → {preset = available}, current preset (CONFIRMED shape: graphics.lua
    -- greys the unavailable ones and selects button_<current>). PROVISIONAL: all available, "custom".
    function o:AvailableGfxQualities()
        return { low = true, medium = true, high = true, very_high = true }, "custom"
    end
    function o:SelectGraphicQualityPreset() return section_options("graphics") end
    -- EnumerateScreenModes() → list of mode names. PROVISIONAL: only the preferences' x_res x y_res.
    function o:EnumerateScreenModes()
        -- Two lists (graphics.lua picks the second one in windowed mode; INFERRED: full-screen and
        -- windowed modes).
        local m = { (__prefs.get("x_res") or "1280") .. " x " .. (__prefs.get("y_res") or "960") }
        return m, m
    end
    function o:ClosestScreenModeToCurrent() return 0 end
    function o:SetScreenMode() return 0, false end
    function o:EnumerateSoundProviders() return {} end
    function o:SelectAudioQualityPreset() return section_options("audio") end
    function o:BuildKeyMappingTable() return {} end
    function o:GetKeyboardDefinitions() return {} end
    function o:LocalisedKeyString(k) return tostring(k or "") end
    function o:IsModifier() return false end
    function o:ModifierHeld() return false end
    function o:ModifierUsed() return false end
    return setmetatable(o, {
        __index = function(t, key)
            return function(...)
                log("UNKNOWN UIPrefsInterface:" .. tostring(key))
                return nil
            end
        end,
    })
end

-- Localisation.Get(key): a localised string (CONFIRMED global table with one function, registered
-- by 0x0103CB30 next to DirectoryUtils; the handler 0x0102CAE0 looks the key up in a string hash
-- table). INFERRED: the random localisation strings (technology_researching.lua asks for
-- technology_status_researched, a `random_localisation_strings` key).
Localisation = { Get = function(key) return __frontend.LocalisationString(tostring(key)) end }
-- DirectoryUtils.DeleteFiles(paths): the load-game page's Delete button (after its confirmation
-- box). Only saves in NapoleonRust's own save folder are deleted; the original's saves are read
-- only to us (`__ntw_delete_saves`, ui/frontend.rs).
DirectoryUtils = stub_table("DirectoryUtils", {
    DeleteFiles = function(paths)
        if __ntw_delete_saves == nil then
            log("DirectoryUtils.DeleteFiles refused: no save folder")
            return
        end
        local list = {}
        for _, p in ipairs(paths or {}) do list[#list + 1] = tostring(p) end
        __ntw_delete_saves(list)
    end,
    -- EnumerateDirectory(dir, "*<ext>") → {{FileName, Path, Date, DateString}, ...} (the file
    -- requester's list; fields CONFIRMED by file_requester.lua's reads). An empty list for a
    -- missing folder (INFERRED).
    EnumerateDirectory = function(dir, pattern)
        if __ntw_enumerate_directory == nil then return {} end
        return __ntw_enumerate_directory(tostring(dir), pattern and tostring(pattern) or "")
    end,
})

-- string.length: used by the original UI scripts (Utilities.lua); INFERRED to be string.len.
string.length = string.len

-- UIHistoricBattleSetup(prefs, info) / UIBattleSetup(info): the battle setup objects passed to
-- FrontEnd.StartBattle(setup:Address(), players, ...) (CONFIRMED usage in movie_panel.lua and
-- sp_episodic_campaign.lua). INFERRED/PROVISIONAL: RetrieveDetails() lists the alliances and
-- armies of the battle file; Address() hands the description table (with its Key) to StartBattle.
local function battle_setup(info)
    local o = { info = info }
    function o:Address() return self.info end
    -- SetDetails(team_setups, ...): the custom battle's armies (CONFIRMED call in sp_battle3.lua
    -- before StartBattle; the other arguments UNKNOWN). Kept on the description for StartBattle.
    function o:SetDetails(teams, ...)
        if type(self.info) == "table" then self.info.__teams = teams end
    end
    function o:RetrieveDetails()
        return { Alliances = FrontEnd.__BattleAlliances(self.info and self.info.File or ""), Key = self.info and self.info.Key }
    end
    return setmetatable(o, { __index = function(t, key)
        return function(...) log("UNKNOWN UIBattleSetup:" .. tostring(key)); return nil end
    end })
end
function UIHistoricBattleSetup(prefs, info) return battle_setup(info) end
-- UIBattleSetup(prefs, info) in sp_battle3.lua (CONFIRMED two arguments); one argument elsewhere.
function UIBattleSetup(a, b) return battle_setup(b or a) end

-- MPAvatar(presence, player_id): a player's online (Steam) avatar image (CONFIRMED uses in
-- template.battle_results_team_entry.lua and template.battle_prep_row_entry.lua). No Steam: an
-- object whose SetComponentTexture / Free do nothing (PLACEHOLDER: the default picture stays).
function MPAvatar()
    return { SetComponentTexture = function() end, Free = function() end, UnManage = function() end, Release = function() end }
end

-- UIComponent:CurrentStateUI(): the current state as an object (CONFIRMED name in the
-- component method table at 0x01464018; army_box.lua's SetUnitLimit). ImageMetrics() → the
-- state's image metrics as objects with Offset() → x, y, Dimensions() → w, h and
-- Set{X, Y, Width, Height, Colour = {R, G, B, A}} (field names CONFIRMED by that script; the
-- index order of the list INFERRED as the metrics' file order).
function methods:CurrentStateUI()
    local address = rawget(self, "__addr")
    return {
        ImageMetrics = function(_)
            local out = {}
            for i, m in ipairs(ui.__ImageMetricsList(address) or {}) do
                local idx = i - 1
                local cur = { x = m[1], y = m[2], w = m[3], h = m[4], c = m[5] }
                out[i] = {
                    Offset = function(_) return cur.x, cur.y end,
                    Dimensions = function(_) return cur.w, cur.h end,
                    Set = function(_, t)
                        t = t or {}
                        cur.x = t.X or cur.x
                        cur.y = t.Y or cur.y
                        cur.w = t.Width or cur.w
                        cur.h = t.Height or cur.h
                        ui.SetImageMetrics(address, idx, cur.x, cur.y, cur.w, cur.h)
                        if type(t.Colour) == "table" then
                            local c = t.Colour
                            ui.SetImageColour(address, idx, c.R or 255, c.G or 255, c.B or 255, c.A or 255)
                        end
                    end,
                }
            end
            return out
        end,
    }
end
