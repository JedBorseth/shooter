extends Node
## Earns XP from a match, as Call of Duty does: every point we score is XP,
## and the end of the match adds a bonus for the time played, more for a
## win (`Progression.match_bonus`). Kills count toward each weapon's
## attachments. Added by the match HUD (`tdm_hud.gd`).
##
## The match's score and kills go into `ProgressStore` when it ends, with
## the bonus. Leaving early keeps the score and kills made so far, without
## the bonus. `match_finished` carries what the match paid, for the
## end-of-match summary.

## `summary`: `score`, `bonus`, `outcome` ("win", "draw", "loss"),
## `seconds`, `xp_before`, `xp_after`, `kills` (plain weapon key to count),
## and `unlocks` (what the levels reached unlock, as `Progression.unlocks_at`
## lists them).
signal match_finished(summary: Dictionary)

var client: NetClient

var _phase := ""
var _playing_since := -1.0
var _score := 0
var _kills := {}
## Plain weapon key for each weapon's name, to count kills by.
var _weapon_names := {}


func _ready() -> void:
	var items := NetClient.class_items()
	for list: String in ["primaries", "secondaries"]:
		for weapon: Dictionary in items[list]:
			_weapon_names[String(weapon.name)] = String(weapon.key)
	if client != null:
		client.kill_reported.connect(_on_kill)


func _process(_delta: float) -> void:
	if client == null or not client.is_joined():
		return
	var info := client.match_info()
	var phase := String(info.phase)
	if phase == "playing":
		if _playing_since < 0.0:
			_reset()
			_playing_since = _now()
		_score = maxi(_score, _my_score())
	if phase != _phase:
		if phase == "ended" and _playing_since >= 0.0:
			_score = maxi(_score, _my_score())
			_finish(info)
		elif phase != "playing":
			_reset()
		_phase = phase


func _exit_tree() -> void:
	# Leaving mid-match: keep what we made, without the bonus.
	if _playing_since >= 0.0 and (_score > 0 or not _kills.is_empty()):
		ProgressStore.add(_score, _kills, not _ranked())
		if _ranked():
			Online.refresh_progress_soon()
	_reset()


func _on_kill(killer: int, victim: int, weapon: String, _headshot: bool) -> void:
	if _playing_since < 0.0 or client == null or killer != client.player_id() or killer == victim:
		return
	var key := weapon_key(weapon)
	if key != "":
		_kills[key] = int(_kills.get(key, 0)) + 1


## The plain weapon key for a weapon's name ("M4A1 Red Dot + Grip" -> "m4"),
## or "" for anything that is not a gun (a frag, the knife).
func weapon_key(weapon: String) -> String:
	if _weapon_names.has(weapon):
		return _weapon_names[weapon]
	# A variant's name is its weapon's name and its attachments.
	for name: String in _weapon_names:
		if weapon.begins_with(name + " "):
			return _weapon_names[name]
	return ""


func _finish(info: Dictionary) -> void:
	var outcome := _outcome(info)
	var seconds := _now() - _playing_since
	var bonus := Progression.match_bonus(seconds, outcome)
	var before := ProgressStore.xp()
	var level_before := ProgressStore.level()
	# A ranked server reports the match to the account itself; the game
	# shows the same sum now and takes the account's totals once it has.
	ProgressStore.add(_score + bonus, _kills, not _ranked())
	if _ranked():
		Online.refresh_progress_soon()
	var level_after := ProgressStore.level()
	var unlocks := []
	for level in range(level_before + 1, level_after + 1):
		unlocks.append_array(Progression.unlocks_at(level))
	var summary := {
		"score": _score,
		"bonus": bonus,
		"outcome": outcome,
		"seconds": seconds,
		"xp_before": before,
		"xp_after": ProgressStore.xp(),
		"kills": _kills.duplicate(),
		"unlocks": unlocks,
	}
	match_finished.emit(summary)
	_reset()


func _ranked() -> bool:
	return client != null and is_instance_valid(client) and client.is_ranked()


func _outcome(info: Dictionary) -> String:
	var winner := int(info.get("winner", -1))
	if winner < 0:
		return "draw"
	if info.get("free_for_all", false):
		return "win" if winner == client.player_id() else "loss"
	return "win" if winner == int(info.get("my_team", -2)) else "loss"


func _my_score() -> int:
	for entry: Dictionary in client.roster():
		if entry.get("me", false):
			return int(entry.get("score", 0))
	return 0


func _reset() -> void:
	_playing_since = -1.0
	_score = 0
	_kills = {}


func _now() -> float:
	return Time.get_ticks_msec() / 1000.0
