-- NapoleonRust battle-script engine classes (our own code, NOT a game file).
--
-- The objects the original battle scripts and data/scripting_library.lua expect from the engine:
-- empire_battle, alliances / armies / units, unit controllers, battle_vector, battle_sound_effect,
-- events, camera and UI stand-ins. Method names are CONFIRMED from the exe's binding tables
-- (BATTLE_FIDELITY.md §16); argument meanings are INFERRED from how the scripts call them.
-- Methods without model behaviour log "UNKNOWN stub" once per call and return nil.

local nb = __nb
local function log(s) nb.log(tostring(s)) end
local unit_class_ref -- set below (collections are defined before the unit class)

-- A class whose unknown methods are logging stubs.
local function class(name)
    local c = {}
    c.__index = c
    setmetatable(c, { __index = function(_, k)
        return function(...) log("UNKNOWN stub " .. name .. ":" .. tostring(k)) return nil end
    end })
    return c
end

-- battle_vector ----------------------------------------------------------------------------------
battle_vector = class("battle_vector")
function battle_vector:new(x, y, z)
    return setmetatable({ x = x or 0, y = y or 0, z = z or 0 }, battle_vector)
end
function battle_vector:get_x() return self.x end
function battle_vector:get_y() return self.y end
function battle_vector:get_z() return self.z end
function battle_vector:set_x(v) self.x = v end
function battle_vector:set_y(v) self.y = v end
function battle_vector:set_z(v) self.z = v end
function battle_vector:set(x, y, z) self.x, self.y, self.z = x, y, z end
-- INFERRED: distance and length are 3D, length_xz ignores the height.
function battle_vector:distance(o)
    local dx, dy, dz = self.x - o.x, self.y - o.y, self.z - o.z
    return math.sqrt(dx * dx + dy * dy + dz * dz)
end
function battle_vector:length() return math.sqrt(self.x * self.x + self.y * self.y + self.z * self.z) end
function battle_vector:length_xz() return math.sqrt(self.x * self.x + self.z * self.z) end

-- Events (phase changes, command handlers) -------------------------------------------------------
local event_class = class("event")
function event_class:get_name() return self.name end
function event_class:get_bool1() return self.bool1 end
function event_class:get_string1() return self.string1 or "" end
function event_class:get_unit() return self.unit end
-- unit_index: the 1-based snapshot index of the unit the event is about (or nil); resolved below
-- once the unit class exists.
function nb.event(name, bool1, string1, unit_index)
    local u = unit_index and nb.unit_object(unit_index) or nil
    return setmetatable({ name = name, bool1 = bool1, string1 = string1, unit = u }, event_class)
end

-- Collections ------------------------------------------------------------------------------------
local collection = class("collection")
function collection:count() return #self.list end
function collection:item(i)
    if type(i) == "string" then
        -- CONFIRMED use: the scripts look units up by their battle-file script_name.
        for _, x in ipairs(self.list) do
            if getmetatable(x) == unit_class_ref and nb.unit(x.__i).script_name == i then return x end
        end
        return nil
    end
    return self.list[i]
end
local function make_collection(list) return setmetatable({ list = list }, collection) end

-- Units ------------------------------------------------------------------------------------------
local unit_class = class("unit")
unit_class_ref = unit_class
local unit_cache = {}
local function unit_object(i)
    local u = unit_cache[i]
    if not u then
        u = setmetatable({ __i = i }, unit_class)
        unit_cache[i] = u
    end
    return u
end
nb.unit_object = unit_object
-- Unit fields are read once per snapshot (nb.version() changes with every set_facts).
local field_cache, field_version = {}, -1
local function f(u)
    local v = nb.version()
    if v ~= field_version then field_cache, field_version = {}, v end
    local t = field_cache[u.__i]
    if not t then
        t = nb.unit(u.__i) or {}
        field_cache[u.__i] = t
    end
    return t
end
function unit_class:name() return f(self).name end
function unit_class:type() return f(self).name end
function unit_class:position() local t = f(self) return battle_vector:new(t.x, t.y, t.z) end
function unit_class:bearing() return f(self).bearing end
function unit_class:is_moving() return f(self).moving end
function unit_class:initial_number_of_men() return f(self).initial_men end
function unit_class:number_of_men_alive() return f(self).men end
function unit_class:is_leaving_battle() return f(self).leaving end
function unit_class:is_routing() return f(self).routing end
function unit_class:missile_range() return f(self).missile_range end
function unit_class:ammo_left() return f(self).ammo end
function unit_class:starting_ammo() return f(self).starting_ammo end
function unit_class:is_cavalry() return f(self).cavalry end
function unit_class:is_infantry() return f(self).infantry end
function unit_class:is_artillery() return f(self).artillery end
function unit_class:is_limbered_artillery() return false end
function unit_class:is_currently_garrisoned() return f(self).garrisoned end
function unit_class:can_perform_special_ability(_) return false end
function unit_class:current_special_ability() return nil end
-- INFERRED: the distance between the units' centres on the ground plane.
function unit_class:unit_distance(o)
    local a, b = f(self), f(o)
    local dx, dz = (a.x or 0) - (b.x or 0), (a.z or 0) - (b.z or 0)
    return math.sqrt(dx * dx + dz * dz)
end
function unit_class:unit_in_range(o) return self:unit_distance(o) <= (f(self).missile_range or 0) end
function unit_class:deploy_reinforcement(b)
    nb.request("deploy_reinforcement", { unit = f(self).id, deploy = b and true or false })
end
local function unit_id(u) return f(u).id end

-- Unit controllers -------------------------------------------------------------------------------
local controller = class("unit_controller")
local function add(c, x)
    if type(x) ~= "table" then return end
    if getmetatable(x) == unit_class then
        c.units[#c.units + 1] = x
    elseif getmetatable(x) == collection then
        for _, u in ipairs(x.list) do add(c, u) end
    end
end
function controller:add_units(...) for _, x in ipairs({ ... }) do add(self, x) end end
function controller:add_group(...) for _, x in ipairs({ ... }) do add(self, x) end end
function controller:add_all_units() for _, u in ipairs(self.army_units) do add(self, u) end end
function controller:clear_all() self.units = {} end
local function ids(c)
    local t = {}
    for i, u in ipairs(c.units) do t[i] = unit_id(u) end
    return t
end
function controller:take_control() nb.request("take_control", { units = ids(self) }) end
function controller:release_control() nb.request("release_control", { units = ids(self) }) end
function controller:halt() nb.request("halt", { units = ids(self) }) end
function controller:fire_at_will(on)
    if on == nil then on = true end
    nb.request("fire_at_will", { units = ids(self), on = on and true or false })
end
local function attack(self, target, _, run)
    if type(target) ~= "table" or getmetatable(target) ~= unit_class then
        log("attack_unit: no target unit") return
    end
    nb.request("attack_unit", { units = ids(self), target = unit_id(target), run = run and true or false })
end
controller.attack_unit = attack
controller.attack_unit_q = attack
local function goto_(self, pos, run)
    if type(pos) ~= "table" then log("goto_location: no position") return end
    nb.request("move", { units = ids(self), x = pos.x, z = pos.z, run = run and true or false })
end
controller.goto_location = goto_
controller.goto_location_q = goto_
local function goto_aw(self, pos, angle, width, run)
    if type(pos) ~= "table" then log("goto_location_angle_width: no position") return end
    nb.request("move", { units = ids(self), x = pos.x, z = pos.z, run = run and true or false, angle = angle, width = width })
end
controller.goto_location_angle_width = goto_aw
controller.goto_location_angle_width_q = goto_aw
-- Orders the model does not carry out yet: passed to the game as "Other" (UNKNOWN effect).
for _, name in ipairs({ "withdraw", "withdraw_q", "kill", "rotate", "rotate_q", "step_forward", "step_backward",
    "change_move_speed", "increment_formation_width", "decrement_formation_width",
    "change_formation", "change_formation_q", "melee", "attack_location", "attack_location_q",
    "attack_building", "attack_building_q", "leave_building",
    "attack_line", "attack_line_q", "change_melee" }) do
    controller[name] = function(self) nb.request(name, { units = ids(self) }) end
end
-- set_invincible(b): command BCQ_UNIT_SET_INVINCIBLE (CONFIRMED).
function controller:set_invincible(on) nb.request("set_invincible", { units = ids(self), on = on and true or false }) end
-- skirmish(b): the binding 0x00613B10 wants a bool ("expecting bool argument", CONFIRMED).
function controller:skirmish(on)
    if type(on) ~= "boolean" then log("skirmish: expecting bool argument") end
    nb.request("skirmish", { units = ids(self), on = on and true or false })
end
-- The ability and deployable names have their spaces turned into '_' (CONFIRMED, 0x00613D50 /
-- 0x00645970).
local function ability_name(s) return (tostring(s or ""):gsub(" ", "_")) end
function controller:select_deployable_object(s)
    nb.request("select_deployable", { units = ids(self), name = ability_name(s) })
end
local function special(self, s) nb.request("special_ability", { units = ids(self), name = ability_name(s) }) end
controller.perform_special_ability = special
controller.perform_special_ability_q = special
local function shot(self, s) nb.request("shot_type", { units = ids(self), name = tostring(s or "") }) end
controller.change_shot_type = shot
controller.change_shot_type_q = shot
-- morale_behavior_*: command BCQ_UNIT_MORALE_CHANGE with mode 0 / 1 / 2 (CONFIRMED).
function controller:morale_behavior_fearless() nb.request("script_morale", { units = ids(self), mode = 0 }) end
function controller:morale_behavior_default() nb.request("script_morale", { units = ids(self), mode = 1 }) end
function controller:morale_behavior_rout() nb.request("script_morale", { units = ids(self), mode = 2 }) end
-- defend_building(building, run): run is the optional second argument (CONFIRMED 1..2 arguments).
local function defend(self, b, run)
    if type(b) ~= "table" or b.index == nil then
        log("defend_building: no building") return
    end
    nb.request("defend_building", { units = ids(self), building = b.index, run = run and true or false })
end
controller.defend_building = defend
controller.defend_building_q = defend
function controller:units_support_shot_type() return false end

-- Armies and alliances -----------------------------------------------------------------------------
local army_class = class("army")
function army_class:units() return make_collection(self.unit_list) end
function army_class:ships() return make_collection({}) end
function army_class:create_unit_controller()
    return setmetatable({ units = {}, army_units = self.unit_list }, controller)
end
function army_class:is_commander_alive() return true end -- UNKNOWN: not tracked here yet
function army_class:get_reinforcement_units()
    local t = {}
    for _, u in ipairs(self.unit_list) do if f(u).off_field then t[#t + 1] = u end end
    return make_collection(t)
end
local alliance_class = class("alliance")
function alliance_class:armies() return make_collection(self.army_list) end

local alliances_cache
local function alliances()
    if not alliances_cache then
        local list = {}
        for ai, armies in ipairs(nb.structure()) do
            local army_list = {}
            for ri, units in ipairs(armies) do
                local ul = {}
                for k, i in ipairs(units) do ul[k] = unit_object(i) end
                army_list[ri] = setmetatable({ unit_list = ul, index = ri }, army_class)
            end
            list[ai] = setmetatable({ army_list = army_list, index = ai }, alliance_class)
        end
        alliances_cache = make_collection(list)
    end
    return alliances_cache
end

-- Stand-ins: camera, UI components, subtitles, weather, sounds ---------------------------------------
local camera_class = class("camera")
-- The camera as the scripts last set it, else the battle's start view (nb.camera()).
local cam_pos, cam_target
local function cam_now()
    if not cam_pos then
        local px, py, pz, tx, ty, tz = nb.camera()
        cam_pos, cam_target = battle_vector:new(px, py, pz), battle_vector:new(tx, ty, tz)
    end
end
function camera_class:position() cam_now() return battle_vector:new(cam_pos.x, cam_pos.y, cam_pos.z) end
function camera_class:target() cam_now() return battle_vector:new(cam_target.x, cam_target.y, cam_target.z) end
-- INFERRED argument order (the scripts pass "Target" then "Camera" positions) and seconds.
function camera_class:move_to(target, position, seconds)
    if type(target) ~= "table" or type(position) ~= "table" then return end
    -- A degenerate view (no start view known) is ignored.
    if target.x == position.x and target.y == position.y and target.z == position.z then return end
    cam_pos, cam_target = battle_vector:new(position.x, position.y, position.z), battle_vector:new(target.x, target.y, target.z)
    nb.request("camera", { units = {}, target = target, position = position, seconds = seconds })
end
-- Known calls with no effect in our game yet (PROVISIONAL, not logged): camera locks.
function camera_class:enable_functionality() end
function camera_class:disable_functionality() end
local the_camera = setmetatable({}, camera_class)
local ui_class = class("ui_component")
function ui_class:set_visible(b) nb.request("ui_visible", { units = {}, name = self.name, visible = b and true or false }) end
-- set_highlight(b) sets the component's highlight flag (+0xD9, CONFIRMED 0x00615630). PROVISIONAL no-op:
-- our HUD draws no highlight yet.
function ui_class:set_highlight() end
local subtitles_class = class("subtitles")
local weather_class = class("weather")

battle_sound_effect = class("battle_sound_effect")
function battle_sound_effect:new() return setmetatable({}, battle_sound_effect) end
function battle_sound_effect:load(name) self.name = name end
function battle_sound_effect:play3D() log("sound " .. tostring(self.name)) end
function battle_sound_effect:is_playing() return false end
function battle_sound_effect:stop() end

-- empire_battle ------------------------------------------------------------------------------------
empire_battle = class("empire_battle")
local the_battle
function empire_battle:new()
    the_battle = the_battle or setmetatable({}, empire_battle)
    return the_battle
end
function empire_battle:out(s) log(s) end
function empire_battle:error(s) log("script error: " .. tostring(s)) end
function empire_battle:alliances() return alliances() end
local building_class = class("building")
function building_class:name() return self.key end
function building_class:position() return battle_vector:new(self.x, self.y, self.z) end
function building_class:health() return 100 end -- PROVISIONAL: buildings are not damaged in the model
function building_class:is_garrisoned() return nb.building_garrisoned(self.index) end
function building_class:currently_garrisoned() return nb.building_garrisoned(self.index) end
local buildings_cache
function empire_battle:buildings()
    if not buildings_cache then
        local list = {}
        for i = 1, nb.building_count() do
            local b = nb.building(i)
            b.index = i
            list[i] = setmetatable(b, building_class)
        end
        buildings_cache = make_collection(list)
    end
    return buildings_cache
end
function empire_battle:camera() return the_camera end
function empire_battle:weather() return setmetatable({}, weather_class) end
function empire_battle:subtitles() return setmetatable({}, subtitles_class) end
function empire_battle:ui_component(name) return setmetatable({ name = tostring(name) }, ui_class) end
-- Known calls with no effect in our game yet (PROVISIONAL, not logged): input and escape-key
-- capture, contextual advice, the advisor window.
function empire_battle:steal_input_focus() end
function empire_battle:release_input_focus() end
function empire_battle:steal_escape_key() end
function empire_battle:release_escape_key() end
function empire_battle:suspend_contextual_advice() end
function empire_battle:close_advisor() end
function empire_battle:register_singleshot_timer(name, ms) nb.timer(name, ms or 0, false) end
function empire_battle:register_repeating_timer(name, ms) nb.timer(name, ms or 0, true) end
function empire_battle:unregister_timer(name) nb.untimer(name) end
function empire_battle:register_battle_phase_handler(name) nb.phase_handler(name) end
function empire_battle:unregister_battle_phase_handler() nb.phase_handler(nil) end
function empire_battle:register_command_handler(name) nb.command_handler(name, true) end
function empire_battle:unregister_command_handler(name) nb.command_handler(name, false) end
function empire_battle:game_time() return nb.time() / 1000 end
function empire_battle:random_number(n) return nb.random(n) end
function empire_battle:register_unit_selection_handler(name) nb.selection_handler(name) end
function empire_battle:unregister_unit_selection_handler() nb.selection_handler(nil) end
function empire_battle:register_input_handler(name) nb.input_handler(name) end
function empire_battle:unregister_input_handler() nb.input_handler(nil) end
-- suppress_unit_voices(b) (0x00612DF0 → 0x006670F0): the units' speech. PROVISIONAL no-op: the
-- game plays no unit voices yet.
function empire_battle:suppress_unit_voices() end
function empire_battle:advice_finished() return true end
function empire_battle:show_advisor_message(s) log("advisor: " .. tostring(s)) end
function empire_battle:show_locatable_advisor_message(s) log("advisor: " .. tostring(s)) end
function empire_battle:end_battle_to_frontend() nb.request("end_battle", { units = {} }) end
-- Markers (battle:marker(name), binding 0x00612C30 → 0x0064BC10, CONFIRMED object layout: position
-- +0, rotation +0xC (set_rotation takes degrees, stored × π/180), scale +0x10, visible +0x14 (0 when
-- made), model +0x18 from the name). Every change sends the whole state to the game.
local marker_class = class("marker")
local marker_count = 0
local function marker_sync(m)
    nb.request("marker", { units = {}, id = m.id, name = m.name, x = m.x, y = m.y, z = m.z,
        rotation = m.rotation, scale = m.scale, visible = m.visible })
end
-- set_position takes a battle_vector or three numbers (CONFIRMED, 0x00615FE0).
function marker_class:set_position(a, b, c)
    if type(a) == "table" then
        self.x, self.y, self.z = a.x or 0, a.y or 0, a.z or 0
    elseif type(a) == "number" and type(b) == "number" and type(c) == "number" then
        self.x, self.y, self.z = a, b, c
    else
        log("ERROR marker:set_position: expecting a battle_vector") return
    end
    marker_sync(self)
end
function marker_class:set_rotation(d) self.rotation = tonumber(d) or 0; marker_sync(self) end
function marker_class:set_scale(s) self.scale = tonumber(s) or 1; marker_sync(self) end
function marker_class:show() self.visible = true; marker_sync(self) end
function marker_class:hide() self.visible = false; marker_sync(self) end
function empire_battle:marker(name)
    marker_count = marker_count + 1
    local m = setmetatable({ id = marker_count, name = tostring(name), x = 0, y = 0, z = 0, rotation = 0,
        scale = 1, visible = false }, marker_class)
    marker_sync(m)
    return m
end

function controller:guard_mode(on)
    if on == nil then on = true end
    nb.request("guard_mode", { units = ids(self), on = on and true or false })
end
