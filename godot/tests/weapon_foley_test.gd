extends SceneTree
## Headless test of the weapon handling sounds with the real player: a reload
## plays magazine out then in (and the bolt release after an empty one) inside
## the reload's time, a pump gun pumps back then forward and loads each shell,
## a bolt gun lifts, draws, pushes, and turns its bolt in order, and a switch
## and aiming each have their sound. Every recording the events name
## exists for every weapon.
##
## With FOLEY_TRACE set to a file path, it also writes every event with its
## time, for the review timelines (tools/audio_report.py).
## Run from the repo root:
##   godot --headless --path godot --script res://tests/weapon_foley_test.gd

const WEAPONS := ["m4", "br3", "ak47", "lsw6", "pdw9", "m1911", "m870", "m40"]

var _failures: PackedStringArray = []
var _player: FpsPlayer
var _rig: Node3D
var _time := 0.0
var _trace: Array = []
var _seen := {}
var _reload_starts: Array[float] = []
var _was_reloading := false


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_check_files()
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	_player = main.get_node("Player") as FpsPlayer
	_rig = _player.get_node("Camera/Viewmodel") as Node3D
	main.get_node("Sentry").set("enabled", false)
	await _frames(60)

	await _reload_case("m4", false)
	await _reload_case("m4", true)
	await _reload_case("lsw6", false)
	await _reload_case("m1911", true)
	await _pump_case()
	await _bolt_case()
	await _switch_and_aim_case()
	_write_trace()
	_finish()


## Every event's recordings load, for every weapon that makes it.
func _check_files() -> void:
	var need := {
		"m4": ["mag_out", "mag_in", "bolt_release", "charge", "dry"],
		"br3": ["mag_out", "mag_in", "bolt_release", "charge", "dry"],
		"ak47": ["mag_out", "mag_in", "charge", "dry"],
		"lsw6": ["mag_out", "mag_in", "charge", "dry"],
		"pdw9": ["mag_out", "mag_in", "charge", "dry"],
		"m1911": ["mag_out", "mag_in", "bolt_release", "dry"],
		"m870": ["pump_back", "pump_forward", "shell_in", "dry"],
		"m40": ["bolt_up", "bolt_back", "bolt_forward", "bolt_down", "mag_out", "mag_in", "dry"],
	}
	for weapon: String in need:
		for event: String in need[weapon]:
			var path := "res://assets/audio/weapons/%s/foley/%s_1.ogg" % [weapon, event]
			_check(ResourceLoader.exists(path), "%s has a %s recording" % [weapon, event])
	for shared in ["ads_in", "ads_out", "draw", "holster", "inspect", "pouch", "breath_hold"]:
		var path := "res://assets/audio/foley/%s_1.ogg" % shared
		_check(ResourceLoader.exists(path), "the shared %s layer exists" % shared)


func _reload_case(weapon: String, empty: bool) -> void:
	await _draw(weapon)
	var label := "%s_%s" % [weapon, "empty" if empty else "tactical"]
	var start := _time
	if empty:
		# Tap, so semi-automatic weapons empty too.
		var guard := 0
		while _player.magazine() > 0 and guard < 200:
			await _hold("fire", 3)
			await _frames(5)
			guard += 1
		await _frames(30)
	else:
		await _hold("fire", 12)
		await _frames(30)
		start = _time
	_mark(label + "_start")
	# An empty weapon reloads by itself; the key does nothing then.
	await _hold("reload", 2)
	await _until_reloaded()
	_mark(label + "_end")
	var events := _events_between(weapon, start, _time)
	var expected := ["mag_out", "mag_in"]
	if empty:
		expected.append("bolt_release")
	_check(
		_names(events) == expected,
		"%s reload plays %s in order, got %s" % [label, expected, _names(events)]
	)
	# The magazine is out early and seated part-way through: it seats when the
	# rules count its rounds (`mag_in` in the weapon file), 40 to 60 percent
	# into a reload, and its sound starts a little before that.
	if events.size() >= 2:
		var length := _time - _reload_began(start)
		start = _reload_began(start)
		_check(events[0][0] - start < length * 0.35, "%s: magazine out early" % label)
		_check(
			events[1][0] - start > length * 0.3 and events[1][0] - start < length * 0.8,
			"%s: magazine in late (%.2f of %.2f s)" % [label, events[1][0] - start, length]
		)


func _pump_case() -> void:
	await _draw("m870")
	var start := _time
	_mark("m870_fire_start")
	await _hold("fire", 2)
	await _frames(50)
	var cycle := _names(_events_between("m870", start, _time))
	_check(
		cycle == ["pump_back", "pump_forward"], "the pump racks back then forward, got %s" % [cycle]
	)
	await _hold("fire", 2)
	await _frames(50)
	start = _time
	_mark("m870_reload_start")
	await _hold("reload", 2)
	await _until_reloaded()
	var loads := _names(_events_between("m870", start, _time))
	_check(loads.count("shell_in") == 2, "two shells go in after two shots, got %s" % [loads])


func _bolt_case() -> void:
	await _draw("m40")
	var start := _time
	_mark("m40_fire_start")
	await _hold("fire", 2)
	await _frames(80)
	var cycle := _names(_events_between("m40", start, _time))
	_check(
		cycle == ["bolt_up", "bolt_back", "bolt_forward", "bolt_down"],
		"the bolt lifts, draws, pushes, and turns down, got %s" % [cycle]
	)


func _switch_and_aim_case() -> void:
	await _draw("m4")
	var start := _time
	_mark("switch_start")
	Input.action_press("weapon_2")
	await _frames(2)
	Input.action_release("weapon_2")
	await _frames(70)
	var names := _names(_events_between("", start, _time))
	_check("holster" in names and "draw" in names, "a switch holsters and draws, got %s" % [names])
	start = _time
	_mark("aim_start")
	await _hold("aim", 40)
	await _frames(30)
	names = _names(_events_between("", start, _time))
	_check(names == ["ads_in", "ads_out"], "aiming in and out, got %s" % [names])


## When the reload that ended last began: the first frame after `after` the
## player was reloading (recorded by `_frames`).
func _reload_began(after: float) -> float:
	for t: float in _reload_starts:
		if t >= after:
			return t
	return after


func _until_reloaded() -> void:
	var guard := 0
	while _player.is_reloading() and guard < 900:
		await _frames(1)
		guard += 1


func _draw(weapon: String) -> void:
	_player.refill_all_ammo()
	if weapon == "m1911":
		Input.action_press("weapon_2")
		await _frames(2)
		Input.action_release("weapon_2")
	else:
		_player.give_weapon(weapon)
	await _frames(90)
	_player.refill_all_ammo()
	await _frames(10)


func _hold(action: String, frames: int) -> void:
	Input.action_press(action)
	await _frames(frames)
	Input.action_release(action)


## Events played by `weapon`'s view (any view for "") from `start` to `end`:
## [time, name] pairs.
func _events_between(weapon: String, start: float, end: float) -> Array:
	var out := []
	for entry: Array in _trace:
		if entry[0] >= start and entry[0] <= end and (weapon == "" or entry[1] == weapon):
			if entry[2] != "mark":
				out.append([entry[0], entry[2]])
	return out


func _names(events: Array) -> Array:
	return events.map(func(e: Array) -> String: return e[1])


func _mark(label: String) -> void:
	_trace.append([_time, label, "mark"])


## Steps `count` sixtieths of a second (one frame at a time, whatever the
## tick rate), collecting what every view's foley played.
func _frames(count: int) -> void:
	var ticks := Engine.physics_ticks_per_second
	for i in count * ticks / 60:
		await process_frame
		_time += 1.0 / ticks
		var reloading := _player.is_reloading()
		if reloading and not _was_reloading:
			_reload_starts.append(_time)
		_was_reloading = reloading
		for view in _rig.get_children():
			if not view is WeaponView:
				continue
			var foley := view.get_node_or_null("Foley")
			if foley == null:
				continue
			var heard: Array = foley.call("events_heard")
			var key := str(view.get_instance_id())
			var before: int = _seen.get(key, 0)
			# The list keeps the last 64; count from what we have seen.
			for event in heard.slice(mini(before, heard.size())):
				_trace.append([_time, String(view.get("weapon")), String(event)])
			_seen[key] = heard.size()


func _write_trace() -> void:
	var path := OS.get_environment("FOLEY_TRACE")
	if path.is_empty():
		return
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string(JSON.stringify(_trace, "  "))
	print("foley trace written to ", path)


func _check(condition: bool, message: String) -> void:
	if not condition:
		_failures.append(message)
		push_error("FAIL: " + message)


func _finish() -> void:
	if _failures.is_empty():
		print("weapon_foley_test: all checks passed")
		quit(0)
	else:
		print("weapon_foley_test: %d failures" % _failures.size())
		quit(1)
