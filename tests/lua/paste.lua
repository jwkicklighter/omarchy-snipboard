-- Exercise the real compositor script with an inert API: no desktop input.
local script = assert(arg[1])
for _, case in ipairs({
  { tags = {}, mods = "CTRL" },
  { mods = "CTRL" },
  { tags = { "unrelated", "terminal" }, mods = "CTRL SHIFT" },
  { tags = { "terminal*", "other" }, mods = "CTRL SHIFT" },
  { tags = { "notterminal" }, mods = "CTRL" },
}) do
  local sent, release = {}, nil
  hl = {
    get_active_window = function() return { tags = case.tags } end,
    dsp = { send_key_state = function(value) return value end },
    dispatch = function(value) sent[#sent + 1] = value end,
    timer = function(callback, options)
      assert(options.timeout == 50 and options.type == "oneshot")
      assert(#sent == 0, "Set up release before pressing")
      release = callback
    end,
  }
  assert(loadfile(script))()
  assert(#sent == 1 and sent[1].state == "down")
  assert(sent[1].mods == case.mods and sent[1].key == "V")
  assert(sent[1].window == nil, "Use focused surface like Omarchy")
  release()
  assert(#sent == 2 and sent[2].state == "up")
  assert(sent[2].mods == case.mods and sent[2].key == "V")
end
-- Cursor positioning waits for paste delivery, uses unmodified arrows, and
-- never navigates a different window if focus changes during the delay.
local file = assert(io.open(script))
local source = file:read("*a")
file:close()
for _, change_focus in ipairs({false, true}) do
  local sent, timers = {}, {}
  local active = { address = "original", tags = {"terminal"} }
  hl = {
    get_active_window = function() return active end,
    dsp = { send_key_state = function(value) return value end },
    dispatch = function(value) sent[#sent + 1] = value end,
    timer = function(callback, options)
      assert(options.type == "oneshot")
      timers[#timers + 1] = {callback = callback, timeout = options.timeout}
    end,
  }
  assert(load((source:gsub("local cursor_steps = 0", "local cursor_steps = 3"))))()
  assert(#sent == 1 and #timers == 1 and timers[1].timeout == 50)
  timers[1].callback()
  assert(#sent == 2 and #timers == 2 and timers[2].timeout == 150)
  if change_focus then active = {address = "different"} end
  timers[2].callback()
  assert(#sent == (change_focus and 2 or 8))
  for i = 3, #sent do
    assert(sent[i].key == "Left" and sent[i].mods == "")
    assert(sent[i].state == (i % 2 == 1 and "down" or "up"))
  end
end
local sent = false
hl.get_active_window = function() return nil end
hl.dispatch = function() sent = true end
assert(not pcall(assert(loadfile(script))))
assert(not sent, "No target must not inject keys")
