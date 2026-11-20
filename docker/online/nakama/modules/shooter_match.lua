--[[
The match handler for matchmade games (authoritative Nakama match). The
matchmaker hook in matchmaking.lua creates one for every match it makes; it
asks the allocator (rust/allocator) for a dedicated server and hands its
address to the matched players, who then leave this match and play on the
server. It ends once its players have taken the address (or after a minute
without them), or when no server could be had.

Messages to players (op codes):
  1  ADDRESS  {host, port, mode, map, private}
  2  FAILED   {reason}

Settings come from Nakama's runtime environment (compose.yml):
  ALLOCATOR_URL  where the allocator listens (http://allocator:7400)
  ALLOCATOR_KEY  its bearer key
]]

local nk = require("nakama")

local M = {}

local OP_ADDRESS = 1
local OP_FAILED = 2
-- Maps each mode is played on (Search and Destroy needs attack and defence
-- sides). Nuketown stays out: its generated assets are optional in a build.
local MAPS = {
  tdm = { "slamcity", "warehouse", "breakwater" },
  dom = { "slamcity", "warehouse", "breakwater" },
  hardpoint = { "slamcity", "warehouse", "breakwater" },
  ctf = { "slamcity", "warehouse", "breakwater" },
  ffa = { "slamcity", "warehouse", "breakwater" },
  gungame = { "slamcity", "warehouse", "breakwater" },
  snd = { "slamcity", "breakwater" },
}
-- Seconds the match waits for its players before giving up.
local JOIN_WINDOW = 60

M.MAPS = MAPS

local function allocate(context, state)
  local env = context.env or {}
  local url = env.ALLOCATOR_URL or "http://allocator:7400"
  local key = env.ALLOCATOR_KEY or ""
  local body = nk.json_encode({
    mode = state.mode,
    map = state.map,
    players = state.player_count,
    match_id = context.match_id,
    private = state.private,
    custom = state.custom,
    team_bots = state.team_bots,
    team_bot_skill = state.team_bot_skill,
  })
  local headers = {
    ["Content-Type"] = "application/json",
    ["Authorization"] = "Bearer " .. key,
  }
  -- The allocator waits for the server's health check: allow two minutes.
  local ok, code, _, answer = pcall(nk.http_request, url .. "/allocate", "POST", headers, body, 120000)
  if not ok then
    nk.logger_error(("allocator unreachable: %s"):format(tostring(code)))
    return nil, "The match server could not be started. Try again."
  end
  local decoded_ok, decoded = pcall(nk.json_decode, answer or "")
  if code ~= 200 or not decoded_ok or type(decoded) ~= "table" or not decoded.port then
    nk.logger_error(("allocator answered %s: %s"):format(tostring(code), tostring(answer)))
    if code == 503 then
      return nil, "Every match server is busy. Try again in a moment."
    end
    return nil, "The match server could not be started. Try again."
  end
  return {
    host = decoded.host,
    port = decoded.port,
    mode = state.mode,
    map = state.map,
    private = state.private,
  }
end

-- Whether `map` is one `mode` is played on.
local function plays_on(mode, map)
  for _, candidate in ipairs(MAPS[mode] or {}) do
    if candidate == map then
      return true
    end
  end
  return false
end

function M.match_init(_context, params)
  local mode = MAPS[params.mode] and params.mode or "tdm"
  local maps = MAPS[mode]
  local count = 0
  for _ in pairs(params.users or {}) do
    count = count + 1
  end
  local state = {
    mode = mode,
    -- A private match plays the host's map; matchmade ones a random one.
    map = plays_on(mode, params.map) and params.map or maps[math.random(#maps)],
    -- Private matches (matchmaking.lua's private_match_start): unranked,
    -- with the host's custom settings and bots per team; the allocator
    -- checks each string.
    private = params.private == true,
    custom = type(params.custom) == "string" and params.custom or "",
    team_bots = type(params.team_bots) == "string" and params.team_bots or "",
    team_bot_skill = type(params.team_bot_skill) == "string" and params.team_bot_skill or "",
    users = params.users or {},
    player_count = count,
    presences = {},
    -- Users who were sent the address.
    delivered = {},
    address = nil,
    failed = nil,
  }
  return state, 1, nk.json_encode({ mode = state.mode, map = state.map })
end

function M.match_join_attempt(_context, _dispatcher, _tick, state, presence, _metadata)
  -- Only the players this match was made for.
  return state, state.users[presence.user_id] == true, "not in this match"
end

-- Sends the address (or why there is none) to `presences`, or to everyone
-- in the match when nil.
local function deliver(dispatcher, state, presences)
  if state.address then
    dispatcher.broadcast_message(OP_ADDRESS, nk.json_encode(state.address), presences)
    for _, presence in pairs(presences or state.presences) do
      state.delivered[presence.user_id] = true
    end
  elseif state.failed then
    dispatcher.broadcast_message(OP_FAILED, nk.json_encode({ reason = state.failed }), presences)
  end
end

function M.match_join(_context, dispatcher, _tick, state, presences)
  for _, presence in ipairs(presences) do
    state.presences[presence.session_id] = presence
  end
  deliver(dispatcher, state, presences)
  return state
end

function M.match_leave(_context, _dispatcher, _tick, state, presences)
  for _, presence in ipairs(presences) do
    state.presences[presence.session_id] = nil
  end
  return state
end

function M.match_loop(context, dispatcher, tick, state, _messages)
  if not state.address and not state.failed then
    local address, reason = allocate(context, state)
    state.address = address
    state.failed = reason
    deliver(dispatcher, state, nil)
    if address then
      nk.logger_info(("match %s: %s on %s at %s:%d"):format(
        context.match_id, state.mode, state.map, address.host, address.port))
    end
  end
  if state.failed then
    return nil
  end
  -- Done once every player has the address, or the join window is over.
  local waiting = false
  for user_id in pairs(state.users) do
    if not state.delivered[user_id] then
      waiting = true
    end
  end
  if not waiting or tick > JOIN_WINDOW then
    return nil
  end
  return state
end

function M.match_terminate(_context, _dispatcher, _tick, state, _grace_seconds)
  return state
end

function M.match_signal(_context, _dispatcher, _tick, state, data)
  return state, data
end

return M
