extends SceneTree
## Measures each two-handed weapon's cross-section where a third-person
## soldier's support hand holds it, on the model other players see (`World`),
## and writes it into the weapon file's grips (`section` in
## godot/data/weapons/*.ron). The support palm rests on that surface, both on
## the drawn soldier and in the server's arm hit zones
## (`shooter_sim::hitbox::carry::Grips::support_contact`). Re-run it after
## changing a weapon's third-person model or moving its `GripLeft` marker,
## then rebuild (the game reads the weapon files at build time):
##   godot --headless --path godot --script res://tests/bake_support_sections.gd
## support_sections_test.gd measures again and fails if a stored section is
## missing or more than TOLERANCE off.

## The weapons held in both hands with the palm on a handguard or a pump.
const WEAPONS := ["m4", "ak47", "br3", "lsw6", "pdw9", "cq10", "kr12", "dm7", "m40", "m870", "sg12"]
## The section counts the cut points within this far of where the hand holds
## across the weapon (m) and up or down, so sights and rails well above a
## handguard, and a magazine well below it, do not count (the hand holds it
## from below). The same window the first-person hands use (`rig.rs`).
const ACROSS := 0.045
const UP_DOWN := 0.035
## How far (m) a stored section's centre or half extents may be from a fresh
## measurement.
const TOLERANCE := 0.002

## Check the stored sections instead of writing them.
var check := false
var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for key: String in WEAPONS:
		var hold := HitboxRig.support_grip(key)
		if hold == Vector3.ZERO:
			_failures.append("%s: no two-handed hold" % key)
			continue
		var section := _measure(key, hold)
		if section.is_empty():
			_failures.append("%s: nothing to hold at %s" % [key, hold])
			continue
		print("%s: hold %s centre %s half %s" % [key, hold, section["centre"], section["half"]])
		var path := "res://data/weapons/%s.ron" % key
		var source := FileAccess.get_file_as_string(path)
		if check:
			_check(key, source, section)
		else:
			_write(path, source, section)
	if _failures.is_empty():
		print("support sections: ok")
		quit(0)
	else:
		for failure in _failures:
			push_error(failure)
		quit(1)


## The section of `key`'s third-person model across the barrel at `hold`: its
## centre and half extents (x, y; weapon space), or empty if nothing is there.
func _measure(key: String, hold: Vector3) -> Dictionary:
	var scene := load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene
	var view := scene.instantiate() as Node3D
	var world := view.get_node_or_null("Model/Mesh/World") as MeshInstance3D
	var out := {}
	if world != null and world.mesh != null:
		var to_view := Transform3D.IDENTITY
		var node: Node = world
		while node != view:
			to_view = (node as Node3D).transform * to_view
			node = node.get_parent()
		var lo := Vector2(INF, INF)
		var hi := Vector2(-INF, -INF)
		var faces := world.mesh.get_faces()
		for i in range(0, faces.size(), 3):
			for e in 3:
				var a: Vector3 = to_view * faces[i + e]
				var b: Vector3 = to_view * faces[i + (e + 1) % 3]
				var da := a.z - hold.z
				var db := b.z - hold.z
				var point: Vector3
				if da == 0.0:
					point = a
				elif da * db < 0.0:
					point = a.lerp(b, da / (da - db))
				else:
					continue
				if absf(point.x - hold.x) < ACROSS and absf(point.y - hold.y) < UP_DOWN:
					lo = Vector2(minf(lo.x, point.x), minf(lo.y, point.y))
					hi = Vector2(maxf(hi.x, point.x), maxf(hi.y, point.y))
		# Under 5 mm across is a sling loop or a stray edge, not a handguard.
		if hi.x - lo.x >= 0.005 and hi.y - lo.y >= 0.005:
			out = {"centre": (lo + hi) * 0.5, "half": (hi - lo) * 0.5}
	view.free()
	return out


func _format(section: Dictionary) -> String:
	var c: Vector2 = section["centre"]
	var h: Vector2 = section["half"]
	return (
		"section: Some((centre: (%s, %s), half: (%s, %s)))"
		% [_num(c.x), _num(c.y), _num(h.x), _num(h.y)]
	)


func _num(v: float) -> String:
	var s := "%.4f" % v
	if s == "-0.0000":
		s = "0.0000"
	return s


func _grips_line(source: String) -> RegExMatch:
	var re := RegEx.create_from_string(
		r"(?m)^    grips: Some\(\((.*?)(, section: Some\(.*\)\))?\)\),$"
	)
	return re.search(source)


func _write(path: String, source: String, section: Dictionary) -> void:
	var found := _grips_line(source)
	if found == null:
		_failures.append("%s: no grips line" % path)
		return
	var line := "    grips: Some((%s, %s)),\n" % [found.get_string(1), _format(section)]
	var updated := source.substr(0, found.get_start()) + line.trim_suffix("\n")
	updated += source.substr(found.get_end())
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string(updated)
	file.close()
	print("wrote ", path)


func _check(key: String, source: String, section: Dictionary) -> void:
	var found := _grips_line(source)
	if found == null or found.get_string(2).is_empty():
		_failures.append("%s: no stored section; run tests/bake_support_sections.gd" % key)
		return
	var re := RegEx.create_from_string(
		r"centre: \(([-\d.]+), ([-\d.]+)\), half: \(([-\d.]+), ([-\d.]+)\)"
	)
	var m := re.search(found.get_string(2))
	if m == null:
		_failures.append("%s: unreadable section %s" % [key, found.get_string(2)])
		return
	var stored := [
		Vector2(float(m.get_string(1)), float(m.get_string(2))),
		Vector2(float(m.get_string(3)), float(m.get_string(4))),
	]
	var fresh := [section["centre"], section["half"]]
	for i in 2:
		var off: float = (stored[i] - fresh[i]).length()
		if off > TOLERANCE:
			_failures.append(
				(
					"%s: stored section %s is %.1f mm from the model's %s; re-run the bake"
					% [key, _format(section), off * 1000.0, found.get_string(2)]
				)
			)
