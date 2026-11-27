extends RefCounted
## A weapon's stat bars for the Create-a-Class item detail, as Black Ops 3's
## weapon select shows them: damage, fire rate, range, handling, and
## mobility, each 0 to 1. Damage (a shot's damage up close, every pellet
## counted, against 100 health) and range (where damage stops falling, against
## 80 m) are on fixed scales, so one big-hitting gun does not shrink the rest;
## the others run between the best and worst gun (the launcher left out). Read
## from the same definitions the game plays (`res://data/weapons/*.ron`,
## see `rust/sim/src/weapon/def.rs`), so the bars always match the guns.
##
## Load it with `preload`; everything here is static.

const DIR := "res://data/weapons/"
## Fields on a fixed scale: the value that fills the bar.
const FIXED := {"damage": 100.0, "falloff_end": 80.0}
## Left out of the bounds: not a gun.
const NOT_GUNS := ["rl4"]
## [label, field, higher is better] for each bar.
const BARS := [
	["DAMAGE", "damage", true],
	["FIRE RATE", "rpm", true],
	["RANGE", "falloff_end", true],
	["HANDLING", "ads_time", false],
	["MOBILITY", "move_scale", true],
]

static var _raw := {}
static var _bounds := {}


## [[label, value 0 to 1], ...] for weapon `key`, or [] if it has no file.
static func bars(key: String) -> Array:
	_load_all()
	var raw: Dictionary = _raw.get(key, {})
	if raw.is_empty():
		return []
	var out := []
	for bar: Array in BARS:
		var field: String = bar[1]
		var span: Vector2 = _bounds[field]
		var value := inverse_lerp(span.x, span.y, float(raw[field])) if span.y > span.x else 1.0
		if FIXED.has(field):
			value = float(raw[field]) / float(FIXED[field])
		if not bar[2]:
			value = 1.0 - value
		# Never empty: the worst weapon still shows a sliver.
		out.append([bar[0], lerpf(0.12, 1.0, clampf(value, 0.0, 1.0))])
	return out


static func _load_all() -> void:
	if not _raw.is_empty():
		return
	var dir := DirAccess.open(DIR)
	if dir == null:
		return
	for file in dir.get_files():
		if file.get_extension() != "ron":
			continue
		var raw := _read(FileAccess.get_file_as_string(DIR + file))
		if raw.size() == BARS.size():
			_raw[file.get_basename()] = raw
	for bar: Array in BARS:
		var field: String = bar[1]
		var low := INF
		var high := -INF
		for key: String in _raw:
			if key in NOT_GUNS:
				continue
			var raw: Dictionary = _raw[key]
			low = minf(low, raw[field])
			high = maxf(high, raw[field])
		_bounds[field] = Vector2(low, high)


## The few numbers the bars need, by regular expression (the files are RON).
static func _read(text: String) -> Dictionary:
	var out := {}
	var patterns := {
		"damage": "damage:\\s*\\(near:\\s*([0-9.]+)",
		"rpm": "rounds_per_minute:\\s*([0-9.]+)",
		"falloff_end": "falloff_end:\\s*([0-9.]+)",
		"ads_time": "ads_time:\\s*([0-9.]+)",
		"move_scale": "\\n\\s*move_scale:\\s*([0-9.]+)",
	}
	for field: String in patterns:
		var regex := RegEx.create_from_string(patterns[field])
		var found := regex.search(text)
		if found != null:
			out[field] = float(found.get_string(1))
	# Pellet guns state damage per pellet: count the pellets in.
	var pellets := RegEx.create_from_string("pellets:\\s*([0-9]+)").search(text)
	if pellets != null and out.has("damage"):
		out.damage = float(out.damage) * float(pellets.get_string(1))
	return out
