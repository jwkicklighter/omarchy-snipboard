do
-- Start with Lua code: hyprctl treats an argument starting with -- as a flag.
-- Use the compositor key-state path used by Omarchy's universal clipboard
-- bindings, so held physical modifiers do not merge into a virtual keyboard.
local window = assert(hl.get_active_window(), "No active paste target")
local mods = "CTRL"
for _, tag in ipairs(window.tags or {}) do
  if tag:gsub("%*$", "") == "terminal" then
    -- Ghostty's default Shift+Insert reads the primary selection. Ctrl+Shift+V
    -- reads the ordinary clipboard in Ghostty, Foot and the other terminals.
    mods = "CTRL SHIFT"
    break
  end
end

local function send(state)
  hl.dispatch(hl.dsp.send_key_state({ mods = mods, key = "V", state = state }))
end

-- Register the release before pressing, so a timer setup error cannot leave V
-- down. The short, separate release mirrors Omarchy's stuck-key workaround.
hl.timer(function() send("up") end, { timeout = 50, type = "oneshot" })
send("down")
end
