class_name MatchStings
extends Node
## Match stings, from the match and round state the client reports: a sting
## when a match goes live, its result (win, loss, or draw) when it ends, a
## warning at 30 seconds left, a call at halftime and at overtime, and in
## Search and Destroy the start and result of every round. No announcer:
## short drum, riser, and brass cues (`tools/build_world_audio.py stings`).
## The match HUD adds one.

const STINGS := "res://assets/audio/ui/%s.ogg"
## Seconds left when the time warning plays.
const WARNING_AT := 30.0

var client: Node
## Stings played, by name, in order (tests).
var played: PackedStringArray = []

var _phase := ""
var _period := "regulation"
var _round_phase := ""
var _round_number := -1
var _seconds_left := INF
var _player: AudioStreamPlayer


func _ready() -> void:
	_player = AudioStreamPlayer.new()
	_player.bus = &"UI"
	_player.volume_db = -6.0
	add_child(_player)


func _process(_delta: float) -> void:
	if client == null or not client.has_method("match_info"):
		return
	var info: Dictionary = client.match_info()
	var phase: String = info.get("phase", "")
	if phase != _phase:
		var was := _phase
		_phase = phase
		if phase == "playing" and was in ["countdown", "lobby"]:
			play("match_start")
		elif phase == "ended" and was == "playing":
			play(_result(info))
		if phase != "playing" and phase != "ended":
			_period = "regulation"
		_seconds_left = INF
	if phase == "playing":
		var period: String = info.get("period", "regulation")
		if period != _period:
			_period = period
			if period == "overtime":
				play_first(["overtime", "time_warning"])
			elif period == "second_half":
				play_first(["halftime", "round_start"])
		var left: float = info.get("seconds_left", INF)
		if info.get("mode", "") != "snd" and _seconds_left > WARNING_AT and left <= WARNING_AT:
			play("time_warning")
		_seconds_left = left
		if info.get("mode", "") == "snd" and client.has_method("round_info"):
			_follow_round(client.round_info(), info)


func _follow_round(round: Dictionary, info: Dictionary) -> void:
	var round_phase: String = round.get("phase", "")
	var number: int = round.get("number", 0)
	if round_phase == _round_phase and number == _round_number:
		return
	var was := _round_phase
	_round_phase = round_phase
	_round_number = number
	if round_phase == "live" and was == "freeze":
		play("round_start")
	elif round_phase == "over" and was == "live":
		var winner: int = round.get("winner", -1)
		play("round_win" if winner == info.get("my_team", -2) else "round_loss")


## The match result sting: in a free-for-all, whether we won; in team modes,
## whether our team did.
func _result(info: Dictionary) -> String:
	var winner: int = info.get("winner", -1)
	if winner < 0:
		return "draw"
	if info.get("free_for_all", false) or info.get("mode", "") == "gungame":
		return "win" if winner == client.player_id() else "loss"
	return "win" if winner == info.get("my_team", -2) else "loss"


## Plays the first of `stings` that exists.
func play_first(stings: Array) -> void:
	for sting: String in stings:
		if ResourceLoader.exists(STINGS % sting):
			play(sting)
			return


func play(sting: String) -> void:
	var path := STINGS % sting
	if not ResourceLoader.exists(path):
		return
	_player.stream = load(path)
	_player.play()
	played.append(sting)
