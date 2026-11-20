--[[
Player progression on the account: total XP and kills with each weapon, the
two numbers every unlock follows (rust/sim/src/progression.rs). Players read
their own progress; only this module writes it.

Storage: collection "progress", key "progress", owned by the player, readable
by the owner only, never writable by a client:
  { xp = 0, weapon_kills = { m4 = 12 }, imported = false, award_at = <ms> }

RPCs for the game (a signed-in session):
  progress_get       -> progress
  progress_import    { xp, weapon_kills }  once, into an account with no
                     progress yet: brings the progress a player earned before
                     accounts existed (the game's user://progress.cfg).
  progress_match     { xp, weapon_kills }  a match the game itself reports
                     (local bot matches and servers that do not report to
                     this stack). Capped by the time since the last award.

RPCs for trusted dedicated servers (called with the runtime HTTP key, never a
session):
  server_progress_get  { user_id }                     -> progress
  server_progress_add  { user_id, xp, weapon_kills }   -> progress

Call of Duty keeps rank and unlocks on its own servers (Demonware), so a game
cannot hand itself unlocks; matches the game reports itself are capped the way
its XP rate allows instead.
]]

local nk = require("nakama")

local M = {}

local COLLECTION = "progress"
local KEY = "progress"
-- Level 55 (rust/sim/src/progression.rs: 600 XP for level 2, 400 more each level).
local MAX_IMPORT_XP = 604800
local MAX_TOTAL_XP = 4294967295
-- A match the game reports itself: at most this much per minute since the last
-- award, counting at most 30 minutes. A strong bot match earns about 300 XP a
-- minute (score plus the 100 a minute match bonus, half again for a win).
local XP_PER_MINUTE = 1500
local KILLS_PER_MINUTE = 20
local MAX_ELAPSED_MINUTES = 30
-- One trusted server report: generous for a 10-minute match.
local MAX_SERVER_XP = 100000
local MAX_SERVER_KILLS = 500
local MAX_WEAPONS = 64

local function invalid(message)
  error({ message, 3 }) -- INVALID_ARGUMENT
end

local function decode(payload)
  if payload == nil or payload == "" then
    return {}
  end
  local ok, value = pcall(nk.json_decode, payload)
  if not ok or type(value) ~= "table" then
    invalid("payload must be a JSON object")
  end
  return value
end

local function whole(value, max)
  if type(value) ~= "number" or value ~= value then
    return 0
  end
  return math.max(0, math.min(math.floor(value), max))
end

-- Weapon kills from a payload: plain weapon keys ("m4") to whole counts.
local function clean_kills(kills, max_each)
  local out = {}
  if type(kills) ~= "table" then
    return out
  end
  local count = 0
  for weapon, n in pairs(kills) do
    if type(weapon) == "string" and #weapon <= 24 and weapon:match("^[a-z0-9_]+$") then
      local amount = whole(n, max_each)
      if amount > 0 then
        count = count + 1
        if count > MAX_WEAPONS then
          break
        end
        out[weapon] = amount
      end
    end
  end
  return out
end

local function read(user_id)
  local objects = nk.storage_read({ { collection = COLLECTION, key = KEY, user_id = user_id } })
  local object = objects[1]
  local value = object and object.value or {}
  local progress = {
    xp = whole(value.xp, MAX_TOTAL_XP),
    weapon_kills = clean_kills(value.weapon_kills, MAX_TOTAL_XP),
    imported = value.imported == true,
    award_at = whole(value.award_at, 2 ^ 53),
  }
  return progress, object and object.version or "*"
end

-- Writes `progress` if nobody wrote since `version` was read ("*": only if
-- there is no object yet). Returns false on a conflict.
local function write(user_id, progress, version)
  local ok = pcall(nk.storage_write, {
    {
      collection = COLLECTION,
      key = KEY,
      user_id = user_id,
      value = progress,
      version = version,
      permission_read = 1,
      permission_write = 0,
    },
  })
  return ok
end

-- Reads, changes with `change(progress)`, and writes, again on a conflict.
local function update(user_id, change)
  for _ = 1, 5 do
    local progress, version = read(user_id)
    change(progress)
    if write(user_id, progress, version) then
      return progress
    end
  end
  error({ "progress changed while saving; try again", 10 }) -- ABORTED
end

local function add(progress, xp, kills)
  progress.xp = math.min(progress.xp + xp, MAX_TOTAL_XP)
  for weapon, n in pairs(kills) do
    progress.weapon_kills[weapon] = math.min((progress.weapon_kills[weapon] or 0) + n, MAX_TOTAL_XP)
  end
end

-- What a client sees: no bookkeeping fields. An empty table encodes as a JSON
-- array in Lua, so weapon_kills is always sent as an object.
local function public(progress)
  local kills = progress.weapon_kills
  if next(kills) == nil then
    kills = nk.json_decode("{}")
  end
  return nk.json_encode({ xp = progress.xp, weapon_kills = kills })
end

local function session_user(context)
  if context.user_id == nil or context.user_id == "" then
    error({ "sign in first", 16 }) -- UNAUTHENTICATED
  end
  return context.user_id
end

-- Server RPCs answer only calls made with the runtime HTTP key.
local function server_only(context, payload)
  if context.user_id ~= nil and context.user_id ~= "" then
    error({ "server only", 7 }) -- PERMISSION_DENIED
  end
  local request = decode(payload)
  if type(request.user_id) ~= "string" or request.user_id == "" then
    invalid("user_id is required")
  end
  -- Fails for an unknown or malformed id.
  local ok = pcall(nk.account_get_id, request.user_id)
  if not ok then
    error({ "no such player", 5 }) -- NOT_FOUND
  end
  return request
end

function M.progress_get(context, _payload)
  local progress = read(session_user(context))
  return public(progress)
end

function M.progress_import(context, payload)
  local user_id = session_user(context)
  local request = decode(payload)
  local xp = whole(request.xp, MAX_IMPORT_XP)
  local kills = clean_kills(request.weapon_kills, 100000)
  return public(update(user_id, function(progress)
    if progress.imported or progress.xp > 0 or next(progress.weapon_kills) ~= nil then
      return
    end
    progress.imported = true
    add(progress, xp, kills)
  end))
end

function M.progress_match(context, payload)
  local user_id = session_user(context)
  local request = decode(payload)
  local now = nk.time()
  return public(update(user_id, function(progress)
    local since = progress.award_at
    if since == 0 then
      -- The first award counts from when the account was made.
      local account = nk.account_get_id(user_id)
      since = (account.user.create_time or 0) * 1000
    end
    local minutes = math.min(math.max(now - since, 0) / 60000, MAX_ELAPSED_MINUTES)
    local xp = whole(request.xp, math.floor(minutes * XP_PER_MINUTE))
    local kill_budget = math.floor(minutes * KILLS_PER_MINUTE)
    local kills = {}
    for weapon, n in pairs(clean_kills(request.weapon_kills, kill_budget)) do
      local amount = math.min(n, kill_budget)
      if amount > 0 then
        kills[weapon] = amount
        kill_budget = kill_budget - amount
      end
    end
    progress.imported = true
    progress.award_at = now
    add(progress, xp, kills)
  end))
end

function M.server_progress_get(context, payload)
  local request = server_only(context, payload)
  return public((read(request.user_id)))
end

function M.server_progress_add(context, payload)
  local request = server_only(context, payload)
  local xp = whole(request.xp, MAX_SERVER_XP)
  local kills = clean_kills(request.weapon_kills, MAX_SERVER_KILLS)
  return public(update(request.user_id, function(progress)
    progress.imported = true
    add(progress, xp, kills)
  end))
end

-- Players never write or delete progress themselves. Storage permissions
-- cannot stop the first write (there is no object yet to forbid it), so the
-- storage API refuses the collection outright.
local function refuse_progress(_context, payload)
  for _, object in ipairs(payload.objects or payload.object_ids or {}) do
    if object.collection == COLLECTION then
      error({ "progress is kept by the server", 7 }) -- PERMISSION_DENIED
    end
  end
  return payload
end

nk.register_req_before(refuse_progress, "WriteStorageObjects")
nk.register_req_before(refuse_progress, "DeleteStorageObjects")
nk.register_rpc(M.progress_get, "progress_get")
nk.register_rpc(M.progress_import, "progress_import")
nk.register_rpc(M.progress_match, "progress_match")
nk.register_rpc(M.server_progress_get, "server_progress_get")
nk.register_rpc(M.server_progress_add, "server_progress_add")

return M
