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
## finger or palm goes into it.

## The weapon files to fit.
const WEAPONS := ["m4", "ak47", "br3", "lsw6", "pdw9", "m40", "m870", "m1911", "laser"]
## How far (mm) a finger or a palm may go into its weapon. A glove gives a
## little; the palm is tested as a flat plate, which a hand wrapped round a
## narrow foregrip is not.
const MOST_FINGER_DEPTH := 1.0
const MOST_PALM_DEPTH := 4.5
## Fingers the fit cannot clear, how far (mm) they may go in instead, and why.
## Fix the model or the grip marker and remove the entry.
const KNOWN_DEPTHS := {
	"lsw6/right index": [4.0, "the box magazine reaches back over the trigger guard"],
	"lsw6+grip/right index": [4.0, "the box magazine reaches back over the trigger guard"],
	"lsw6/left middle finger": [2.0, "not yet diagnosed"],
	"lsw6+grip/left index": [6.5, "not yet diagnosed"],
	"pdw9+grip/left little finger": [9.5, "not yet diagnosed"],
	"m1911/left index": [6.0, "not yet diagnosed"],
}
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
		var takes_grip := source.contains("Grip,") or source.contains("Grip]")
		var variants := [key, key + "+grip"] if takes_grip else [key]
		if check:
			var stored := _stored_block(source)
			if stored.is_empty() or (takes_grip and not stored.contains("left_foregrip")):
				_failures.append("%s: no fitted hand poses; run tests/bake_hand_poses.gd" % key)
				continue
			for variant: String in variants:
				var depths := await _hold(player, arms, variant, "hand_depths")
				_check_depths(variant, depths)
			continue
		var fit := await _hold(player, arms, key, "fit_hands")
		var foregrip := {}
		if takes_grip:
			foregrip = await _hold(player, arms, key + "+grip", "fit_hands")
		if fit.is_empty() or (takes_grip and foregrip.is_empty()):
			_failures.append("%s: the hands did not hold it" % key)
			continue
		_check_depths(key, fit)
		if takes_grip:
			_check_depths(key + "+grip", foregrip)
		_write(path, source, _block(fit, foregrip))
	if not _failures.is_empty():
		for failure in _failures:
			printerr("FAIL: ", failure)
		quit(1)
		return
	print("PASS" if check else "Wrote the hand poses; rebuild the game to use them.")
	quit()


## Holds `key` (a weapon, or one with an attachment fitted) and returns the
## arms' `query` (`fit_hands` or `hand_depths`) for it.
func _hold(player: FpsPlayer, arms: Node, key: String, query: String) -> Dictionary:
	if not player.give_weapon(key):
		return {}
	# A pickup fills its slot; switch to it (the pistol is the second slot).
	var slot := "weapon_2" if key.begins_with("m1911") else "weapon_1"
	Input.action_press(slot)
	await _frames(2)
	Input.action_release(slot)
	await _frames(70)
	return arms.call(query)


## Fails for any finger or palm deeper into the weapon than allowed. A fit
## that fails still writes, so the failures can be looked at in the game.
func _check_depths(key: String, hands: Dictionary) -> void:
	if hands.is_empty():
		_failures.append("%s: the hands did not hold it" % key)
		return
	var depths: PackedFloat32Array = hands["finger_depths"]
	for i in depths.size():
		var known: Array = KNOWN_DEPTHS.get("%s/%s" % [key, FINGER_NAMES[i]], [MOST_FINGER_DEPTH])
		if depths[i] > known[0]:
			_failures.append("%s: the %s is %.1f mm into it" % [key, FINGER_NAMES[i], depths[i]])
	var palms: Vector2 = hands["palm_depth"]
	if max(palms.x, palms.y) > MOST_PALM_DEPTH:
		_failures.append("%s: a palm is %.1f mm into it" % [key, max(palms.x, palms.y)])


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


## The `hands` block already in a weapon file, or "".
func _stored_block(source: String) -> String:
	var start := source.find(BLOCK_START)
	if start < 0:
		return ""
	var end := source.find("    )),\n", start)
	return "" if end < 0 else source.substr(start, end + 8 - start)


func _write(path: String, source: String, block: String) -> void:
	var old := _stored_block(source)
	var updated := ""
	if not old.is_empty():
		updated = source.replace(old, block)
	else:
		# Before the closing parenthesis of the definition.
		var close := source.rfind(")")
		updated = source.substr(0, close) + block + source.substr(close)
	var file := FileAccess.open(ProjectSettings.globalize_path(path), FileAccess.WRITE)
	file.store_string(updated)


func _frames(n: int) -> void:
	for i in n:
		await physics_frame
