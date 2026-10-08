-- NapoleonRust scripting prelude (our own code, NOT a game file).
--
-- The original engine provides a number of global tables that the shipped scripts use
-- (W3 §6.2: UIComponent, Component, CampaignUI, FrontEnd, BattleUI, UIImage, mp_interface, out).
-- We have no UI yet, so this file defines them as harmless stand-ins that only write a log line.
-- Everything here is UNKNOWN behaviour: a stub that lets the scripts run without crashing.
--
-- __ntw_log(text) is a Rust function that appends to ScriptState.log.

local log = __ntw_log

-- A "null UI component": every method call returns the same null object, so chains such as
-- UIComponent(m_root:Find("x")):SetVisible(false) run and do nothing.
local null_mt = {}
local null = setmetatable({}, null_mt)
null_mt.__index = function(_, method)
    return function(...)
        log("UNKNOWN UI stub: component:" .. tostring(method))
        return null
    end
end
null_mt.__tostring = function() return "ntw_null_component" end

function UIComponent(address)
    return null
end

-- A table whose every field is a logging stub function returning nil.
local function stub_table(name)
    return setmetatable({}, {
        __index = function(t, key)
            local f = function(...)
                log("UNKNOWN stub: " .. name .. "." .. tostring(key))
                return nil
            end
            rawset(t, key, f)
            return f
        end,
    })
end

Component = stub_table("Component")
CampaignUI = stub_table("CampaignUI")
FrontEnd = stub_table("FrontEnd")
BattleUI = stub_table("BattleUI")
UIImage = stub_table("UIImage")
mp_interface = stub_table("mp_interface")

-- out.ting(...), out.tom(...), ...: the developers' named log channels (W3 §6.3). Any name works.
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
