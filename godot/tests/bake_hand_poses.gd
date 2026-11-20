extends SceneTree
## Fits each weapon's hand poses to its model and writes them into its weapon
## file (`hands` in godot/data/weapons/*.ron), as a first-person animator
## poses the hands on every weapon: the player holds each weapon (and each
## weapon that takes a foregrip, with one fitted), and `FpArms.fit_hands`
## seats each hand on the surface and closes every finger onto it from the
## hold's shape (rust/game/src/character/grasp.rs). Re-run it after moving a
## weapon's grip markers, changing its model, or changing the gloves, then
## rebuild (the game reads the weapon files at build time):
##   godot --headless --path godot --script res://tests/bake_hand_poses.gd
## hand_poses_test.gd holds every weapon in its stored poses and fails if a
## sampled finger or palm goes into it (past the limits and exceptions below)
## or a weapon has no stored poses. It does not compare the poses with a
## fresh fit: re-run this after changing a model.
## Values set by hand after the fit go in the weapon file's `hand_tuning`
## field, which the game applies over `hands`; this tool writes only `hands`,
## so a re-bake keeps them (check that their reason still holds).

## The weapon files to fit.
const WEAPONS := ["m4", "ak47", "br3", "lsw6", "pdw9", "cq10", "m40", "m870", "m1911", "laser"]
## How far (mm) a finger or a palm may go into its weapon. A glove gives a
## little; the palm is tested as a flat plate, which a fist wrapped round a
## narrow foregrip is not (those read about 5 mm in).
const MOST_FINGER_DEPTH := 1.0
const MOST_PALM_DEPTH := 5.5
## How far (mm) an arm may fall short of putting its hand where its pose is.
const MOST_REACH_MISS := 5.0
## Fingers the fit cannot clear, how far (mm) they may go in instead, and why.
## Fix the model or the grip marker and remove the entry.
const KNOWN_DEPTHS := {
	"lsw6/left middle finger": [2.0, "not yet diagnosed"],
	"m1911/left index": [6.0, "not yet diagnosed"],
}
## How far (degrees, summed over its three joints) each of the firing hand's
## middle, ring, and little fingers must close: a hand round a pistol grip
## wraps it. A fit that stops early leaves the hand open beside the grip,
## fingers straight along the frame and the thumb standing up beside the
## slide (the M1911 after #200, in the aimed sight picture).
const LEAST_WRAP := 120.0
## Weapons whose firing hand the fit still leaves open, and why. The hand sits
## over the receiver instead of round the grip; fix the grip marker, the
## rake, or the seat, re-run the bake, and remove the entry. Every weapon
## closes its hand now: the grip markers sit on each model's grip, raked as
## its grip is (#215 found the cause on the pistol).
const KNOWN_OPEN := {}
const BLOCK_START := "    // Finger poses fitted to the model"
## `FpArms.hand_depths`'s `finger_depths`, in order.
const FINGER_NAMES := [
	"right index",
	"right middle finger",
	"right ring finger",
	"right little finger",
	"right thumb",
	"right index off the trigger",
	"left index",
	"left middle finger",
	"left ring finger",
	"left little finger",
	"left thumb",
]

## Check the stored poses instead of fitting and writing them.
var check := false
var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(scene)
	current_scene = scene
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	var player := scene.get_node("Player") as FpsPlayer
	var arms := player.get_node("Camera/Viewmodel/Arms")
	player.set_look(0.0, -0.05)
	await _frames(60)
	for key: String in WEAPONS:
		var path := "res://data/weapons/%s.ron" % key
		var source := FileAccess.get_file_as_string(path)
		if source.is_empty():
			_failures.append("%s: no weapon file" % key)
			continue
		if source.contains("\r"):
			_failures.append("%s: the weapon file has CRLF line ends" % key)
			continue
		var takes_grip := _takes_grip(source)
		var variants := [key, key + "+grip"] if takes_grip else [key]
		if check:
			var stored := _stored_block(source)
			if stored in ["", "?"] or (takes_grip and not stored.contains("left_foregrip: Some")):
				_failures.append("%s: no fitted hand poses; run tests/bake_hand_poses.gd" % key)
				continue
			for variant: String in variants:
				var depths := await _hold(player, arms, variant, "hand_depths")
				_check_depths(variant, depths)
			_check_wrap(key, _stored_right(stored))
			continue
		var fit := await _hold(player, arms, key, "fit_hands")
		# Checked as the game draws it: with the file's hand-set values.
		var drawn: Dictionary = arms.with_tuning(fit, false) if not fit.is_empty() else {}
		var foregrip := {}
		var drawn_grip := {}
		if takes_grip:
			foregrip = await _hold(player, arms, key + "+grip", "fit_hands")
			if not foregrip.is_empty():
				drawn_grip = arms.with_tuning(foregrip, true)
		if fit.is_empty() or (takes_grip and foregrip.is_empty()):
			_failures.append("%s: the hands did not hold it" % key)
			continue
		_check_depths(key, drawn)
		_check_wrap(key, drawn.get("right", PackedFloat32Array()))
		if takes_grip:
			_check_depths(key + "+grip", drawn_grip)
		if not _write(path, source, _block(fit, foregrip)):
			_failures.append("%s: cannot find where its `hands` field ends; not written" % key)
	if not _failures.is_empty():
		for failure in _failures:
			printerr("FAIL: ", failure)
		quit(1)
		return
	print("PASS" if check else "Wrote the hand poses; rebuild the game to use them.")
	quit()


## Holds `key` (a weapon, or one with an attachment fitted) and returns the
## arms' `query` (`fit_hands` or `hand_depths`) for it, or {} if the player
## never ends up holding it, ready.
func _hold(player: FpsPlayer, arms: Node, key: String, query: String) -> Dictionary:
	if not player.give_weapon(key):
		return {}
	# A pickup fills its slot; switch to it (the pistol is the second slot).
	var slot := "weapon_2" if key.begins_with("m1911") else "weapon_1"
	Input.action_press(slot)
	await _frames(2)
	Input.action_release(slot)
	for i in 300:
		if player.weapon_key() == key and player.weapon_ready():
			break
		await _frames(1)
	if player.weapon_key() != key or not player.weapon_ready():
		return {}
	# Past the raise (a pump gun's first raise racks it), with both grip
	# markers at rest for a while, so the hands are on their grips.
	var settled := 0
	for i in 600:
		settled = settled + 1 if arms.hands_settled() else 0
		if settled >= 10:
			break
		await _frames(1)
	if settled < 10:
		return {}
	return arms.call(query)


## Whether the weapon file lists the foregrip among its attachments.
func _takes_grip(source: String) -> bool:
	var line := RegEx.create_from_string("(?m)^\\s*attachments:\\s*\\[([^\\]]*)\\]").search(source)
	return (
		line != null and RegEx.create_from_string("\\bGrip\\b").search(line.get_string(1)) != null
	)


## Fails for any finger or palm deeper into the weapon than allowed. A fit
## that fails still writes, so the failures can be looked at in the game.
func _check_depths(key: String, hands: Dictionary) -> void:
	if hands.is_empty():
		_failures.append("%s: the hands did not hold it" % key)
		return
	var depths: PackedFloat32Array = hands.get("finger_depths", PackedFloat32Array())
	var palms: Vector2 = hands.get("palm_depth", Vector2(INF, INF))
	# Every finger and both palms measured: a missing or unmeasured one fails.
	var measured := depths.size() == FINGER_NAMES.size() and palms.is_finite()
	for d in depths:
		measured = measured and is_finite(d) and d > -1000.0
	if not measured:
		_failures.append("%s: not every finger and palm was measured" % key)
		return
	for i in depths.size():
		var known: Array = KNOWN_DEPTHS.get("%s/%s" % [key, FINGER_NAMES[i]], [MOST_FINGER_DEPTH])
		if depths[i] > known[0]:
			_failures.append("%s: the %s is %.1f mm into it" % [key, FINGER_NAMES[i], depths[i]])
	var miss: Vector2 = hands.get("reach_miss", Vector2.ZERO)
	if max(miss.x, miss.y) > MOST_REACH_MISS:
		_failures.append("%s: a hand is %.1f mm off its grip" % [key, max(miss.x, miss.y)])
	if max(palms.x, palms.y) > MOST_PALM_DEPTH:
		_failures.append("%s: a palm is %.1f mm into it" % [key, max(palms.x, palms.y)])


## Fails if the firing hand's lower three fingers (`right`: 15 curls, index
## to thumb, three joints each) do not close round the grip, unless the
## weapon is a listed exception.
func _check_wrap(key: String, right: PackedFloat32Array) -> void:
	if right.size() != 15:
		_failures.append("%s: no firing-hand curls to check" % key)
		return
	if key in KNOWN_OPEN:
		return
	for f in [1, 2, 3]:
		var wrap := right[f * 3] + right[f * 3 + 1] + right[f * 3 + 2]
		if wrap < LEAST_WRAP:
			_failures.append(
				(
					"%s: the firing hand's %s closes only %.0f degrees round the grip"
					% [key, FINGER_NAMES[f], wrap]
				)
			)


## The firing hand's stored curls (index to thumb, three joints each) from a
## weapon file's `hands` block, or an empty array if they cannot be read.
func _stored_right(block: String) -> PackedFloat32Array:
	var line := RegEx.create_from_string("right: \\(fingers: \\(([^s]*)\\), spread").search(block)
	var out := PackedFloat32Array()
	if line == null:
		return out
	for number in RegEx.create_from_string("-?[0-9.]+").search_all(line.get_string(1)):
		out.append(float(number.get_string()))
	return out


## The weapon file's `hands` block for a fit.
func _block(fit: Dictionary, foregrip: Dictionary) -> String:
	var lines := [
		BLOCK_START + " by tests/bake_hand_poses.gd:",
		"    // curl in degrees per joint (index, middle, ring, little, thumb), each",
		"    // finger's spread, and the hand's seat off its marker (m). Re-run it",
		"    // after moving the grips or changing the model.",
		"    hands: Some((",
		"        right: %s," % _hand(fit, "right"),
		"        index_off: %s," % _joints(fit["index_off"], 0),
		"        left: %s," % _hand(fit, "left"),
	]
	if not foregrip.is_empty():
		lines.append("        left_foregrip: Some(%s)," % _hand(foregrip, "left"))
	lines.append("    )),")
	return "\n".join(lines) + "\n"


## One hand of a fit ("right" or "left") as a weapon file's `HandDef`.
func _hand(fit: Dictionary, side: String) -> String:
	var flex: PackedFloat32Array = fit[side]
	var spread: PackedFloat32Array = fit[side + "_spread"]
	var seat: Vector3 = fit[side + "_seat"]
	var fingers := []
	for f in 5:
		fingers.append(_joints(flex, f * 3))
	var spreads := []
	for s in spread:
		spreads.append(_angle(s))
	return (
		"(fingers: (%s), spread: Some((%s)), seat: (%s, %s, %s))"
		% [
			", ".join(fingers),
			", ".join(spreads),
			_metres(seat.x),
			_metres(seat.y),
			_metres(seat.z),
		]
	)


func _joints(flex: PackedFloat32Array, at: int) -> String:
	return "(%s, %s, %s)" % [_angle(flex[at]), _angle(flex[at + 1]), _angle(flex[at + 2])]


func _angle(a: float) -> String:
	return str(int(a)) if is_equal_approx(a, roundf(a)) else "%.1f" % a


## Metres to the tenth of a millimetre, with no trailing zeros.
func _metres(m: float) -> String:
	var text := ("%.4f" % m).rstrip("0").rstrip(".")
	return "0" if text in ["-0", ""] else text


## The `hands` field already in a weapon file, with the comment lines just
## above it, or "" if it has none. "?" if it has one whose end cannot be
## found (the field closes with `)),` alone on a line).
func _stored_block(source: String) -> String:
	var field := RegEx.create_from_string("(?m)^[ \\t]*hands:").search(source)
	if field == null:
		return ""
	var start := field.get_start()
	# Take the comment lines directly above it with it.
	while start > 0:
		var line_start := source.rfind("\n", start - 2) + 1
		if not source.substr(line_start, start - line_start).strip_edges().begins_with("//"):
			break
		start = line_start
	var end := RegEx.create_from_string("(?m)^[ \\t]*\\)\\),[ \\t]*$").search(
		source, field.get_end()
	)
	if end == null:
		return "?"
	return source.substr(start, end.get_end() + 1 - start)


## Writes `block` in place of the file's `hands` field, or before the
## definition's closing parenthesis if it has none. False, writing nothing,
## if it has one whose end cannot be found.
func _write(path: String, source: String, block: String) -> bool:
	var old := _stored_block(source)
	var updated := ""
	if old == "?":
		return false
	if not old.is_empty():
		updated = source.replace(old, block)
	else:
		var close := source.rfind(")")
		updated = source.substr(0, close) + block + source.substr(close)
	var file := FileAccess.open(ProjectSettings.globalize_path(path), FileAccess.WRITE)
	file.store_string(updated)
	return true


func _frames(n: int) -> void:
	for i in n:
		await physics_frame
