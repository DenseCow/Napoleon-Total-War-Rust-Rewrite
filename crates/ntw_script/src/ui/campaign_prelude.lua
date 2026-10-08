-- NapoleonRust campaign HUD prelude (our own code, NOT a game file). Runs after ui_prelude.lua
-- and after the CampaignUI functions were filled in by Rust (ui/campaign.rs).

-- The global `Component` of required modules (Recruitment.lua, Agents.lua, ...): see
-- `__ntw_root_env` in ui_prelude.lua.

-- The HUD root layout's globals are the modules' globals (INFERRED: the original root script
-- runs in the global table). Evidence: Recruitment.lua reads `g_review_panel` and `g_recruitment`,
-- and template.BuildingFrame.lua's SelectPassive reads `Construction.g_repair_construction_button`
-- (a `module(..., package.seeall)` table, so a global), all set only by layout.root.lua's
-- SETGLOBALs; without them the building slots never show their upgrade options. Our root runs
-- in its own environment, so a global missing from `_G` is looked up there (one level, no loop).
setmetatable(_G, {
    __index = function(t, key)
        local env = rawget(t, "__ntw_root_env")
        if env == nil then return nil end
        return rawget(env, key)
    end,
})

-- INFERRED: the engine's `loadfile` reads through its VFS (CoreUtils.NamespaceFile calls
-- `loadfile` on every `package.path` template, e.g. `data/ui/theatre_map.lua`, which only ships
-- as `ui\theatre_map.luac`).
loadfile = function(path)
    return __ntw_loadfile(path)
end


-- The engine initialises the HUD's card groups with their manager modules once the layout is
-- built (INFERRED: CardGroup.lua's Initialise(manager, multiple) is called by no script, and its
-- SelectionChanged forwards to manager.SelectionChanged, which Army.lua, Agents.lua, ... define).
-- PROVISIONAL: multiple selection only for unit cards.
function __ntw_campaign_ready(root)
    -- INFERRED: modules see a global `Address` outside component environments (Labels.lua
    -- creates the settlement labels under it); like `Component`, the HUD root's.
    rawset(_G, "Address", root)
    local groups = {
        { "UnitCardGroup", "army", true },
        { "AgentCardGroup", "agents", false },
        { "ConstructionCardGroup", "construction", false },
        { "RecruitmentCardGroup", "recruitment", false },
        { "SiegeEquipmentCardGroup", "siegeequipment", false },
    }
    for _, g in ipairs(groups) do
        local a = __ui.Find(root, g[1])
        local mod
        for k, v in pairs(package.loaded) do
            if type(k) == "string" and string.lower(k) == g[2] and type(v) == "table" then mod = v end
        end
        if a ~= nil and mod ~= nil then __ntw_call_if_defined(a, "Initialise", mod, g[3]) end
    end
end

-- Tooltips: the shared `__ntw_tooltip` (ui_prelude.lua); the campaign HUD's name for it.
__ntw_campaign_tooltip = function(...) return __ntw_tooltip(...) end

-- CampaignSettlement(address): the engine's settlement handle (CONFIRMED class name in the exe and
-- template.city_info_bar.lua's calls: Settlement(), LabelDetails(), Release()).
function CampaignSettlement(address)
    return {
        Settlement = function(self) return address end,
        LabelDetails = function(self) return CampaignUI.__LabelDetails(address) end,
        Release = function(self) end,
    }
end

-- CampaignCharacter(address): the engine's character handle (CONFIRMED class name: the list row
-- templates and agent_options.lua create one per character). Character()/Address() give the
-- character back. CanHarrass() is answered from the model (CONFIRMED the one use: `agent_options.lua:34`
-- pc 92-96 asks it of the SOURCE agent to pick the Sabotage Army button's state and its
-- `percent_dy`); other methods are UNKNOWN logging stubs.
function CampaignCharacter(address)
    local obj = {
        Character = function(self) return address end,
        Address = function(self) return address end,
        CanHarrass = function(self) return CampaignUI.__CanHarrass(address) end,
        Release = function(self) end,
    }
    return setmetatable(obj, {
        __index = function(t, key)
            return function(...)
                __ntw_log("UNKNOWN CampaignCharacter:" .. tostring(key))
                return nil
            end
        end,
    })
end

-- Component.LockPriority(p) returns the previous lock; PanelManager keeps it as the panel's
-- PriorityLock (CONFIRMED use: "Priority lock on panel open. Was: " .. PriorityLock), and
-- UnLockPriority() clears it (same INFERRED behaviour as the battle HUD's). Input priority
-- itself has no effect in our renderer yet (PROVISIONAL).
do
    local priority_lock = 0
    __comp.LockPriority = function(_, p)
        local prev = priority_lock
        priority_lock = tonumber(p) or priority_lock
        return prev
    end
    __comp.UnLockPriority = function()
        priority_lock = 0
    end
end

-- CampaignUI.ConstructBuildingTree(slot, parent) → the tree's height: the building browser's tree
-- view (building_browser.lua calls it with the selected entry's slot and its tree component).
-- The node list comes from CampaignUI.__BuildingTreeNodes (see `building_tree` in ui/campaign.rs,
-- CONFIRMED structure of 0x009B8830 / 0x009B9120). Each node is a `building_browser_node`
-- template named after its level key, with the picture "{<key>:1}<icon>", its state, its tooltip
-- and Initialise(key, region_key, faction_key, slot_key) (CONFIRMED four arguments).
-- Layout INFERRED from 0x0099A6A0 / 0x00A17F60: each parent centred over its children's row,
-- siblings 10 apart, rows 20 apart, 8 from the top, the whole tree centred in the parent.
-- Links (0x009D4C80, CONFIRMED names "vertical node link" / "horizontal node link" from the
-- `general_purpose_pixel` template, black): straight down, or an elbow when the x differ.
-- PROVISIONAL: the nodes are children of the tree component, not of their parent node.
CampaignUI.ConstructBuildingTree = function(slot, parent)
    local t = CampaignUI.__BuildingTreeNodes(slot)
    if t == nil or parent == nil then return 0 end
    local p = UIComponent(parent)
    local px, py = p:Position()
    local pw = p:Width()
    local W, H, GAPX, GAPY, TOP = 64, 48, 10, 20, 8
    local nodes = t.nodes
    local kids, roots = {}, {}
    for i = 1, #nodes do kids[i] = {} end
    for i, n in ipairs(nodes) do
        if n.parent == 0 then table.insert(roots, i) else table.insert(kids[n.parent], i) end
    end
    local width = {}
    local function measure(i)
        local sum = 0
        for k, c in ipairs(kids[i]) do
            sum = sum + measure(c) + (k > 1 and GAPX or 0)
        end
        width[i] = math.max(W, sum)
        return width[i]
    end
    local total = 0
    for k, r in ipairs(roots) do total = total + measure(r) + (k > 1 and GAPX or 0) end
    local pos, bottom = {}, 0
    local function place(i, left, top)
        pos[i] = { left + (width[i] - W) / 2, top }
        bottom = math.max(bottom, top + H)
        local sum = 0
        for k, c in ipairs(kids[i]) do sum = sum + width[c] + (k > 1 and GAPX or 0) end
        local x = left + (width[i] - sum) / 2
        for _, c in ipairs(kids[i]) do
            place(c, x, top + H + GAPY)
            x = x + width[c] + GAPX
        end
    end
    local x = math.max(0, (pw - total) / 2)
    for _, r in ipairs(roots) do
        place(r, x, TOP)
        x = x + width[r] + GAPX
    end
    local function pixel(name, ax, ay, w, h)
        local a = Component.CreateComponentFromTemplate("general_purpose_pixel", name, parent, 0, 0)
        if a == nil then return end
        local c = UIComponent(a)
        c:MoveTo(px + ax, py + ay)
        c:Resize(math.max(1, w), math.max(1, h))
        c:SetImageColour(0, 0, 0, 0, 255)
    end
    for i, n in ipairs(nodes) do
        if n.parent ~= 0 then
            local fx, fy = pos[n.parent][1] + W / 2, pos[n.parent][2] + H
            local tx, ty = pos[i][1] + W / 2, pos[i][2]
            if fx == tx then
                pixel("vertical node link", fx, fy, 1, ty - fy)
            else
                local mid = fy + math.floor((ty - fy) / 3)
                pixel("vertical node link", fx, fy, 1, mid - fy)
                pixel("horizontal node link", math.min(fx, tx), mid, math.abs(tx - fx) + 1, 1)
                pixel("vertical node link", tx, mid, 1, ty - mid)
            end
        end
    end
    for i, n in ipairs(nodes) do
        local a = Component.CreateComponentFromTemplate("building_browser_node", n.key, parent, 0, 0, { "{" .. n.key .. ":1}" .. n.image })
        if a ~= nil then
            local c = UIComponent(a)
            c:MoveTo(px + pos[i][1], py + pos[i][2])
            c:SetState(n.state)
            c:SetTooltipText(n.tooltip)
            c:LuaCall("Initialise", n.key, t.region_key, t.faction_key, t.slot_key)
        end
    end
    return bottom + TOP
end
