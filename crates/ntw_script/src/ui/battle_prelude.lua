-- NapoleonRust battle HUD prelude (our own code, NOT a game file).
--
-- The engine side of the original battle UI scripts: BattleUI.* functions and the UICardManager
-- class. Names and call shapes are CONFIRMED from the scripts and the exe's binding table
-- (registrar 0x59EB40); return shapes are INFERRED from how the scripts use them
-- (analysis/battle/BATTLE_FLOW.md section 3). The game fills the table __battle every frame
-- (crates/ntw_script/src/ui/battle.rs) and drains __battle_requests.

local log = __ntw_log
local B = BattleUI

local function request(t)
    local q = __battle_requests
    q[#q + 1] = t
end

local function addr_of(c)
    if type(c) == "table" then return rawget(c, "__addr") end
    return c
end

-- The HUD root's environment: the first one created (the layout's top component runs first).
-- INFERRED, as for the campaign HUD: modules the HUD requires (battle_HUD_visibility_manager,
-- PanelManager, 3d_component_manager) use the global `Component` outside any component
-- environment, so the engine must provide one; we bind it to the root's.
local make_env = __ntw_make_env
function __ntw_make_env(address)
    local env = make_env(address)
    if __ntw_battle_root_env == nil then __ntw_battle_root_env = env end
    return env
end
Component = setmetatable({}, {
    __index = function(_, key)
        local env = __ntw_battle_root_env
        if env == nil then return nil end
        return env.Component[key]
    end,
})

-- Image lists given to CreateComponentFromTemplate (6th argument) and CreateFromLayout (4th):
-- "{<component id>:<n>}<path>" (CONFIRMED forms: review_DY.lua "{" .. CardID .. ":1}" .. Portrait
-- .. ".tga"; battle_hud.lua "{flagR_dy:1}" .. FlagPath .. "/HUD_right.tga"). INFERRED: <path>
-- replaces image <n> (1-based) of the component with that id in the new subtree.
local function find_in(a, id)
    local c = UIComponent(a)
    if c == nil then return nil end
    if c:Id() == id then return a end
    local f = c:Find(id)
    return f
end
local function apply_images(new, images)
    if new == nil or type(images) ~= "table" then return end
    for _, spec in ipairs(images) do
        local id, n, path = nil, nil, nil
        if type(spec) == "string" then id, n, path = string.match(spec, "^{([^:}]*):(%d+)}(.+)$") end
        if path then
            local target = find_in(new, id)
            if target ~= nil then
                path = string.gsub(path, "^[Dd][Aa][Tt][Aa][/\\]", "")
                __ui.__SetImagePath(addr_of(target), math.max(tonumber(n) - 1, 0), path)
            end
        end
    end
end
local create_from_template = __comp.CreateComponentFromTemplate
__comp.CreateComponentFromTemplate = function(a, template, id, parent, x, y, images, texts)
    local new = create_from_template(a, template, id, parent, x, y, images, texts)
    apply_images(new, images)
    return new
end
local create_from_layout = __comp.CreateFromLayout
__comp.CreateFromLayout = function(a, path, id, parent, images)
    local new = create_from_layout(a, path, id, parent)
    apply_images(new, images)
    -- A layout kept with its full-screen `root` wrapper (more than one top child, e.g.
    -- land_battle_orders) is authored in screen coordinates: put the wrapper at the screen origin
    -- (INFERRED: its orders bar then sits just above the unit cards, as in the original HUD).
    -- PROVISIONAL test for such a wrapper: full screen, with two or more children that are all
    -- screen-wide panels (mp_postbattle is full screen too but is one panel with small parts).
    local c = new and UIComponent(new)
    local wrapper = false
    if c ~= nil then
        local w, h = c:Dimensions()
        local n = c:ChildCount() or 0
        wrapper = w ~= nil and w >= 1280 and h >= 960 and n >= 2
        for i = 0, n - 1 do
            local ch = c:Find(i)
            if ch == nil or (UIComponent(ch):Width() or 0) < 1280 then wrapper = false end
        end
    end
    if wrapper then
        do
            c:MoveTo(0, 0)
            -- Only the file's first panel is shown (land_battle_orders, not land_battle_ordersOLD),
            -- and calls on the wrapper reach that panel's script (root.lua LuaCalls
            -- SetOrderButtonState on Find("orders")). Both INFERRED.
            local first = c:Find(0)
            local n = c:ChildCount() or 0
            for i = 1, n - 1 do
                local other = c:Find(i)
                if other ~= nil then UIComponent(other):SetVisible(false) end
            end
            local wenv, fenv = __ntw_envs[new], first and __ntw_envs[first]
            if wenv ~= nil and fenv ~= nil then
                for k, v in pairs(fenv) do
                    if type(v) == "function" and rawget(wenv, k) == nil then rawset(wenv, k, v) end
                end
            end
        end
    end
    return new
end

-- Calls a global function of whichever component environment defines it.
function __ntw_battle_call_global(name, ...)
    for _, env in pairs(__ntw_envs) do
        local f = rawget(env, name)
        if type(f) == "function" then
            __ntw_call_in(rawget(env, "Address"), f, ...)
            return true
        end
    end
    return false
end

-- State -------------------------------------------------------------------------------------

B.IsReplay = function() return false end
B.IsSpectator = function() return false end
B.IsMultiplayer = function() return false end
B.IsTutorial = function() return false end
B.IsCampaignBattle = function() return false end
B.IsMinimisedHUD = function() return false end
B.Valid = function() return true end
-- "Returns true if we're in deployment (not default deployment) mode" / "in conflict mode".
B.HasEnteredDeployment = function() return __battle.phase == "deployment" end
B.IsConflict = function() return __battle.phase ~= "deployment" end
-- Described as "Returns true if we're in conflict mode" (INFERRED: true once deployment is over).
B.IsDeploymentOrConflict = function() return __battle.phase ~= "deployment" end
-- Battle time in seconds (the scripts compare it with TotalTime and with their own timestamps).
B.ElapsedBattleTime = function() return __battle.elapsed or 0 end
B.Time = function() return __battle.elapsed or 0 end
-- "Returns the current time in seconds" (CONFIRMED 0x005D4BF0): timeGetTime() * 0.001f as a float,
-- fractional, unlike CampaignUI.WindowsTime (whole seconds); see host.rs `battle_windows_time`.
B.WindowsTime = __ntw_battle_windows_time
-- "Returns the current time multiplier of the battle".
B.TickPeriod = function() return __battle.speed or 1 end
B.ScreenSize = function() return FrontEnd.ScreenSize() end
B.NumHumansRequestingNextPhase = function() return 0 end
B.UISkin = function() return "" end

-- BattleDetails(): IsMultiplayer, NavalBattle, TotalTime (seconds; < 0 = no limit), plus the
-- player's faction (CONFIRMED field reads in root.lua, battle_hud_timer.lua, battle_hud.lua).
B.BattleDetails = function()
    local total = __battle.total or 0
    return {
        IsMultiplayer = false,
        NavalBattle = __battle.naval or false,
        TotalTime = (total > 0) and total or -1,
        FlagPath = __battle.player_flag,
        Name = __battle.battle_name,
    }
end

-- Speed controls ------------------------------------------------------------------------------
B.Pause = function() request({ kind = "speed", value = 0 }) end
B.Slow = function() request({ kind = "speed", value = 0.4 }) end
B.Play = function() request({ kind = "speed", value = 1 }) end
B.Fwd = function() request({ kind = "speed", value = 2 }) end
B.Ffwd = function() request({ kind = "speed", value = 4 }) end
B.CycleBattleSpeed = function() request({ kind = "cycle_speed" }) end

-- Deployment and results ----------------------------------------------------------------------
B.InformOfDeploymentFinished = function() request({ kind = "deployment_finished" }) end
B.InformOfDeploymentCountdownBegun = function() end
B.DeploymentFinishYesStart = function() request({ kind = "deployment_finished" }) end
B.ExitBattle = function() request({ kind = "exit" }) end
B.PostBattleDismissEndBattle = function() request({ kind = "end_battle" }) end
B.PostBattleDismissContinueBattle = function() request({ kind = "continue" }) end
B.InformOfBattleSummaryDismiss = function() request({ kind = "summary_dismissed" }) end
B.MPResultsReady = function() return true end

-- Selection and orders ------------------------------------------------------------------------

-- The selected unit ids, in card order.
local function selected_ids()
    local out = {}
    for _, u in ipairs(__battle.units or {}) do
        if u.selected then out[#out + 1] = u.id end
    end
    return out
end
__ntw_battle_selected_ids = selected_ids

-- Every Current_Selection_* order and the named formation/fire orders become an "order"
-- request with the binding name and its (boolean) argument.
local order_names = {
    "Current_Selection_Halt", "Current_Selection_Walks", "Current_Selection_Runs",
    "Current_Selection_Start_Firing_At_Will", "Current_Selection_Stop_Firing_At_Will",
    "Current_Selection_Enable_Fire_At_Will", "Current_Selection_Enable_Melee",
    "Current_Selection_Enable_Guard", "Current_Selection_Enable_Fire_And_Advance",
    "Current_Selection_Enable_Skirmish", "Current_Selection_Enable_Square_Formation",
    "Current_Selection_Enable_Wedge_Formation", "Current_Selection_Enable_Diamond_Formation",
    "Current_Selection_Enable_Loose_Formation", "Current_Selection_Enable_Limber",
    "Current_Selection_Rotate_Left", "Current_Selection_Rotate_Right",
    "Current_Selection_Move_Forwards", "Current_Selection_Move_Backwards",
    "Current_Selection_Increase_Rank", "Current_Selection_Increase_File",
    "Current_Selection_Rally_Units", "Current_Selection_Enable_Inspire_Unit",
    "Current_Selection_Special_ability", "Land_Unit_Withdraw", "Fire_At_Will",
    "CurrentSelectionWalks", "CurrentSelectionRuns", "CurrentSelectionHalt",
    "CancelOrderForSelection",
    "Single_Line_Standard", "Single_Line_Cavalry_Left_Flank", "Single_Line_Cavalry_Right_Flank",
    "Double_Line_Standard", "Double_Line_Screened", "Triple_Line_Standard",
    "Triple_Line_Grand_Battery", "Triple_Line_Integrated_Artillery", "Column_Infantry_Vanguard",
    "Crescent_Attack", "Crescent_Envelop",
}
for _, name in ipairs(order_names) do
    B[name] = function(arg)
        if type(arg) ~= "boolean" then arg = nil end
        request({ kind = "order", name = name, arg = arg })
    end
end

local function select_category(cat)
    local ids = {}
    for _, u in ipairs(__battle.units or {}) do
        if cat == nil or u.category == cat then ids[#ids + 1] = u.id end
    end
    request({ kind = "select", ids = ids })
end
B.SelectAllInfantry = function() select_category("infantry") end
B.SelectAllCavalry = function() select_category("cavalry") end
B.SelectAllArtillery = function() select_category("artillery") end
B.SelectionChanged = function() end
B.IsUnitSelectable = function() return true end
B.ZoomToUnit = function(u) request({ kind = "zoom", id = u }) end
B.CameraZoomToSelection = function() request({ kind = "zoom" }) end
B.ZoomToGeneral = function() request({ kind = "zoom" }) end
B.MouseMovedOntoCard = function() end
B.MouseMovedOffCard = function() end
B.ExplicitlyCancelMouseHeld = function() end
B.RequestMouseInterruptType = function() end
B.ReleaseMouseCursorInterrupt = function() end
B.EnableShortcutHandler = function() end
B.SetSelectionProxy = function() end
B.UpdateBattleLocks = function() end
B.TriggerPanelOpenEvent = function() end
B.TriggerPanelClosedEvent = function() end
B.TriggerAdviceForPanel = function() end
B.TriggerMessageDropEvent = function() end
B.TriggerMessageOpenedEvent = function() end
B.DismissCurrentAdvice = function() end
-- { Land = {...}, Naval = {...} } (CONFIRMED reads in 3d_component_manager.lua). PROVISIONAL: no
-- floating unit ids over the 3D units yet.
B.UnitListForBattleIds = function() return { Land = {}, Naval = {} } end
B.CombatRatioDetails = function() return {} end

-- "Update the killometer bar": the player's share of the balance of power (INFERRED 0..1).
B.GetHealthStatus = function() return __battle.balance or 0.5 end
B.WindDirection = function() return 0 end

-- Strings: random_localisation_strings / ui text (the engine's loc lookups).
B.LocalisationString = function(key)
    return FrontEnd.LocalisationString(tostring(key)) or ""
end
B.UILocalisationString = function(key)
    return FrontEnd.UILocalisationString(tostring(key)) or ""
end

-- PostBattleInfo(): is_from_campaign / is_drop_in_battle read by root.lua's DismissBattleResult;
-- is_multiplayer, is_draw, player_wins, winning_teams / losing_teams (lists of team entries) and
-- player_unit_statistics read by mp_postbattle.lua; team entry fields read by
-- template.battle_results_team_entry.lua (display_name, men_deployed, losses, enemy_killed,
-- flags_path, portrait, skill, skill_change, human_player, local_player, mp_index); unit
-- statistic fields read by template.mp_postbattle_entry.lua (name, CustomName, deployed, lost,
-- kills, start_xp, end_xp) and mp_postbattle.lua (icon_name). All names CONFIRMED; the values'
-- meanings INFERRED (skill: multiplayer only, empty here).
B.PostBattleInfo = function()
    local teams = {}
    for i, r in ipairs(__battle.results or {}) do
        teams[i] = {
            display_name = r.name, men_deployed = r.men_start, losses = r.men_start - r.men_alive,
            enemy_killed = r.kills, flags_path = r.flag, portrait = nil, skill = 0, skill_change = 0,
            human_player = (i == 1), local_player = (i == 1), mp_index = i - 1, achievements = {},
        }
    end
    local won = __battle.player_won
    local draw = (won == nil)
    local winning, losing = {}, {}
    if won == false then
        winning[1], losing[1] = teams[2], teams[1]
    else
        winning[1], losing[1] = teams[1], teams[2]
    end
    local stats = {}
    for i, u in ipairs(__battle.units or {}) do
        stats[i] = {
            name = u.name, CustomName = "", deployed = u.max_men, lost = u.max_men - u.men,
            kills = u.kills, start_xp = u.experience, end_xp = u.experience,
            icon_name = (u.portrait or "") .. ".tga",
        }
    end
    return {
        is_from_campaign = false,
        is_drop_in_battle = false,
        is_multiplayer = false,
        is_draw = draw,
        player_wins = (won == true),
        winning_teams = winning,
        losing_teams = losing,
        player_unit_statistics = stats,
    }
end

-- UICardManager --------------------------------------------------------------------------------
-- The engine's card manager (CONFIRMED methods from review_DY.lua: SetAsActive, SetInactive,
-- AddCard, RemoveCard, RemoveAll, PositionCards, Selected, SelectCardList, DeselectAll,
-- DeselectCard, ManageSelection, ManageGroupSelection, DragSelected, RemoveFromGroup, IsCtrlHeld).
-- Card layout and selection rules are PROVISIONAL: cards in a row, left to right, wrapping.
local card_mt = {}
card_mt.__index = card_mt

function UICardManager(_land)
    local m = setmetatable({ cards = {}, active = false }, card_mt)
    __ntw_card_manager = m
    return m
end

function card_mt:SetAsActive() self.active = true end
function card_mt:SetInactive() self.active = false end
function card_mt:AddCard(card) self.cards[#self.cards + 1] = addr_of(card) end
function card_mt:RemoveCard(card)
    local a = addr_of(card)
    for i, c in ipairs(self.cards) do
        if c == a then table.remove(self.cards, i) return end
    end
end
function card_mt:RemoveAll() self.cards = {} end
function card_mt:IsCtrlHeld() return false end
function card_mt:RemoveFromGroup() end
function card_mt:DragSelected() end
function card_mt:ManageGroupSelection() end

-- review_DY.lua places each card itself (x/y from the card widths, wrapping at the tab group
-- width), then calls PositionCards(bool). The engine's own extra layout is UNKNOWN
-- (PROVISIONAL: the script's positions are kept).
function card_mt:PositionCards() end

-- The unit id of a card (cards are created with the id "card_<unit id>").
local function card_unit(card)
    local c = UIComponent(addr_of(card))
    if c == nil then return nil end
    return tonumber(string.match(c:Id() or "", "^card_(%d+)$"))
end
__ntw_card_unit = card_unit

function card_mt:Selected()
    local out = {}
    for _, a in ipairs(self.cards) do
        local id = card_unit(a)
        for _, u in ipairs(__battle.units or {}) do
            if u.id == id and u.selected then out[#out + 1] = UIComponent(a) end
        end
    end
    return out
end
function card_mt:ManageSelection(card)
    local id = card_unit(card)
    if id then request({ kind = "select", ids = { id } }) end
end
function card_mt:SelectCardList(list)
    local ids = {}
    for _, c in ipairs(list or {}) do
        local id = card_unit(c)
        if id then ids[#ids + 1] = id end
    end
    request({ kind = "select", ids = ids })
end
function card_mt:DeselectAll() request({ kind = "select", ids = {} }) end
function card_mt:DeselectCard() end

-- The card list the engine hands to CreateCards: { CardID, Portrait, ... } per player unit.
function __ntw_battle_card_list()
    local list = {}
    for i, u in ipairs(__battle.units or {}) do
        list[i] = { CardID = "card_" .. u.id, Portrait = u.portrait, Address = nil, UnitId = u.id }
    end
    return list
end

-- The fields of a card's info table that follow its unit (the rest are constants).
local function fill_card_info(info, u)
    info.Men, info.NumMen, info.Guns, info.NumGuns = u.men, u.max_men, u.guns, u.max_guns
    info.IsArtillery, info.HasAmmo, info.AmmoRemainingAsPercent = u.is_artillery, u.has_ammo, u.ammo_percent
    info.Experience = u.experience
    info.RoutingState, info.WaveringState = u.routing, u.wavering
    info.WalkingState, info.RunningState, info.FiringState = u.walking, u.running, u.firing
    info.MeleeState, info.UnderFireState = u.melee, u.under_fire
    return info
end

-- The table a card's Update(info) reads (CONFIRMED field names in template.battleunitcard.lua).
function __ntw_battle_card_info(u)
    return fill_card_info({
        WithdrawingState = false,
        Inactive = false, Hidden = false, Garrisoned = false, Obstructed = false,
        OnFire = false, Sinking = false, Repairing = false,
        SailHealth = 1, HullHealth_l = 1, HullHealth_r = 1, SunkUnary = 0, BurnUnary = 0,
    }, u)
end

-- Each card's info table by unit id, made once and refreshed in place every frame (no table per
-- card per frame). Safe because the card's Update never keeps its argument: it reads the fields
-- and copies the ones it compares into its own `previous_details` table (CONFIRMED in the
-- bytecode of template.BattleUnitCard.lua's Update, source line 297: only GETTABLE on the
-- argument, SETTABLE into the upvalue; no SETUPVAL or SETGLOBAL of it).
local card_infos = {}

-- Engine steps -------------------------------------------------------------------------------

-- Creates the player's unit cards: CreateCards(list, state) in the cards panel (CONFIRMED global
-- of review_DY.lua), then calls each card's SetInitialState. Its first parameter is the card's
-- naval flag (CONFIRMED, template.BattleUnitCard.lua bytecode at source line 55: stored as
-- `m_naval`, given to `ship_damage:PropagateVisibility`); the card compares each Update with its
-- own `previous_details` table, not with anything passed here. PLACEHOLDER: we pass the info table
-- and the list entry; what the engine passes is not traced (BACKLOG §7 Battle UI).
function __ntw_battle_create_cards()
    local list = __ntw_battle_card_list()
    __ntw_battle_call_global("CreateCards", list, {})
    __ntw_battle_cards = {}
    card_infos = {}
    for _, entry in ipairs(list) do
        local u
        for _, x in ipairs(__battle.units or {}) do
            if x.id == entry.UnitId then u = x end
        end
        local m = __ntw_card_manager
        local addr = nil
        if m ~= nil then
            for _, a in ipairs(m.cards) do
                if UIComponent(a):Id() == entry.CardID then addr = a end
            end
        end
        if addr ~= nil and u ~= nil then
            local info = __ntw_battle_card_info(u)
            for k, v in pairs(info) do entry[k] = v end
            UIComponent(addr):LuaCall("SetInitialState", info, entry)
            __ntw_battle_cards[u.id] = addr
        end
    end
end

-- Every frame, for each unit with a card (the host runs this as that card's script, ui/battle.rs
-- `update_cards`, so its LuaCall is a plain call): the card's Update(info) with its unit's state
-- (CONFIRMED field names), and the Selected / Default state from the selection (PROVISIONAL: how
-- the engine marks selected cards is UNKNOWN; the template has a "Selected" state).
function __ntw_battle_update_card(addr, u)
    local c = UIComponent(addr)
    local info = card_infos[u.id]
    if info == nil then
        info = __ntw_battle_card_info(u)
        card_infos[u.id] = info
    else
        fill_card_info(info, u)
    end
    c:LuaCall("Update", info)
    local st = c:CurrentState()
    -- Only on a change, never every frame: SetState runs the state's enter function each time
    -- (CONFIRMED, 0x01035B30), and BattleUnitCard's "Selected" / "Default" states have the enter
    -- functions Selected / Unselected.
    if u.selected and st == "Default" then
        c:SetState("Selected")
    elseif not u.selected and st == "Selected" then
        c:SetState("Default")
    end
end

-- The order buttons' states from the selection, through root.lua's SetOrderButtonState(id, state)
-- (CONFIRMED: it forwards to land_hud_orders.lua, which takes Inactive / Unselected / Selected /
-- Locked). Which buttons the engine enables for which units is PROVISIONAL: halt, run, melee,
-- fire at will, withdraw and the unit controls for any selection; groups, group formations and
-- the special abilities stay inactive (not done).
local order_state_cache = {}
function __ntw_battle_update_orders()
    local sel, running, faw, ammo = 0, true, true, false
    for _, u in ipairs(__battle.units or {}) do
        if u.selected then
            sel = sel + 1
            running = running and u.running
            faw = faw and u.fire_at_will
            ammo = ammo or u.has_ammo
        end
    end
    local function st(on, selected)
        if not on then return "Inactive" end
        if selected then return "Selected" end
        return "Unselected"
    end
    local any = sel > 0
    local wanted = {
        button_halt = st(any), button_movespeed = st(any, running), button_melee = st(any, false),
        button_fire_at_will = st(any and ammo, faw), button_withdraw = st(any, false),
        button_group = "Inactive", button_groupforms = "Inactive",
        button_ability2_DY = "Inactive", button_ability3_DY = "Inactive",
        button_ability4_DY = "Inactive", button_ability5_DY = "Inactive",
        UC_button_turn_left = st(any), UC_button_turn_right = st(any),
        UC_button_move_forwards = st(any), UC_button_move_backwards = st(any),
        UC_button_increase_rank = st(any), UC_button_increase_file = st(any),
    }
    for id, s in pairs(wanted) do
        if order_state_cache[id] ~= s then
            order_state_cache[id] = s
            local env = __ntw_battle_root_env
            local f = env and rawget(env, "SetOrderButtonState")
            if type(f) == "function" then __ntw_call_in(rawget(env, "Address"), f, id, s, "", false) end
        end
    end
end

-- A left click that ended on component `addr`: a click on a unit card (or inside one) selects
-- its unit; with `add`, adds it to the selection (PROVISIONAL engine side of the card manager).
function __ntw_battle_click(addr, add)
    local a = addr
    while a ~= nil do
        local c = UIComponent(a)
        local id = tonumber(string.match(c:Id() or "", "^card_(%d+)$"))
        if id ~= nil then
            local ids = { id }
            if add then
                for _, s in ipairs(selected_ids()) do
                    if s ~= id then ids[#ids + 1] = s end
                end
            end
            request({ kind = "select", ids = ids })
            return true
        end
        a = c:Parent()
    end
    return false
end

-- Component.LockPriority(p) returns the previous lock (INFERRED: PanelManager stores the result
-- as the panel's PriorityLock and logs "Priority lock on panel open. Was: ..."). Input priority
-- itself has no effect in our renderer yet.
local priority_lock = 0
__comp.LockPriority = function(_, p)
    local prev = priority_lock
    priority_lock = tonumber(p) or priority_lock
    return prev
end
__comp.UnLockPriority = function()
    priority_lock = 0
end

-- MPAvatar(index): the player's online (Steam) avatar image for the results' team entries
-- (CONFIRMED use in template.battle_results_team_entry.lua). No Steam: an object whose
-- SetComponentTexture / Free do nothing (PLACEHOLDER: the entry keeps its default picture).
function MPAvatar()
    return { SetComponentTexture = function() end, Free = function() end }
end
