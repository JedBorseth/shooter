--[[
Quick play and parties.

The game puts a player, or a whole party, into Nakama's matchmaker with the
mode they want (string property `mode`, query `+properties.mode:<mode>`).
When the matchmaker makes a match, this hook starts a `shooter_match`
(shooter_match.lua), which gets a dedicated server from the allocator and
hands its address to the players. Parties use Nakama's realtime parties;
members queue together with the leader's ticket.

Nakama's matchmaker needs at least two tickets' worth of players, so a
player alone, or a party nobody else is searching with, would wait forever.
After a while searching the game stops and asks for a match of its own
(`quick_play_start`): bots fill the server, as Call of Duty fills a lobby.

RPCs for the game (a signed-in session):
  party_invite      { username, party_id }  sends that player an invite
                    (notification subject "party_invite", code 100, content
                    { party_id, from }) that their game shows.
  quick_play_start  { mode, users }  starts a match for the caller and the
                    party members in `users` (user ids, up to six), and tells
                    each of them (notification "quick_play_match", code 101,
                    content { match_id }) to join it.
]]

local nk = require("nakama")

local MODES = {
  tdm = true, dom = true, hardpoint = true, ctf = true, ffa = true, gungame = true, snd = true,
}
local INVITE_CODE = 100
local MATCH_CODE = 101
local PARTY_SIZE = 6

local function matchmaker_matched(_context, entries)
  local mode = "tdm"
  local users = {}
  for _, entry in ipairs(entries) do
    users[entry.presence.user_id] = true
    local wanted = entry.properties and entry.properties.mode
    if MODES[wanted] then
      mode = wanted
    end
  end
  return nk.match_create("shooter_match", { mode = mode, users = users })
end

local function party_invite(context, payload)
  if context.user_id == nil or context.user_id == "" then
    error({ "sign in first", 16 })
  end
  local ok, request = pcall(nk.json_decode, payload or "")
  if not ok or type(request) ~= "table" then
    error({ "payload must be a JSON object", 3 })
  end
  local username = request.username
  local party_id = request.party_id
  if type(username) ~= "string" or username == "" or #username > 64 then
    error({ "username is required", 3 })
  end
  if type(party_id) ~= "string" or not party_id:match("^[%w%-%.]+$") or #party_id > 80 then
    error({ "party_id is required", 3 })
  end
  local users = nk.users_get_username({ username })
  local user = users[1]
  if user == nil then
    error({ "No player has that name.", 5 })
  end
  if user.user_id == context.user_id then
    error({ "That is you.", 3 })
  end
  nk.notification_send(user.user_id, "party_invite", {
    party_id = party_id,
    from = context.username,
  }, INVITE_CODE, context.user_id, false)
  return nk.json_encode({ invited = user.username })
end

local function quick_play_start(context, payload)
  if context.user_id == nil or context.user_id == "" then
    error({ "sign in first", 16 })
  end
  local ok, request = pcall(nk.json_decode, payload or "")
  if not ok or type(request) ~= "table" then
    error({ "payload must be a JSON object", 3 })
  end
  local mode = MODES[request.mode] and request.mode or "tdm"
  local users = { [context.user_id] = true }
  local count = 1
  for _, id in ipairs(type(request.users) == "table" and request.users or {}) do
    if type(id) == "string" and not users[id] and count < PARTY_SIZE then
      users[id] = true
      count = count + 1
    end
  end
  local match_id = nk.match_create("shooter_match", { mode = mode, users = users })
  for id in pairs(users) do
    if id ~= context.user_id then
      nk.notification_send(id, "quick_play_match", { match_id = match_id }, MATCH_CODE, context.user_id, false)
    end
  end
  return nk.json_encode({ match_id = match_id })
end

nk.register_matchmaker_matched(matchmaker_matched)
nk.register_rpc(party_invite, "party_invite")
nk.register_rpc(quick_play_start, "quick_play_start")
