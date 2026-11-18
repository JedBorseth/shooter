extends SceneTree
## Headless weapon model test. For every weapon rebuilt from the source packs
## (tools/build_weapons.py, models in metres): the view scene has its moving
## parts and markers, the spare magazine starts hidden, the muzzle is ahead of
## the sight and the grips below it, and the front sight's tip lies on the
## sight line (the view's -Z axis), so aimed shots land where the sights
## point. Seen from the eye when aimed, the iron sights make a clear picture:
## the line over the front sight runs free through the rear notch or
## aperture, and the front sight, not the receiver or the rear sight, is what
## the eye sees just under it, centred. Every weapon's maps carry real
## surface detail (a flat grey colour map and an empty normal map read as
## plastic). The red dot and the ACOG draw their modelled housings. Then
## every armory rack shows its weapon.
## godot --headless --path godot --script res://tests/weapon_models_test.gd

## Weapon view scene -> the model nodes its animations and reloads use.
const WEAPONS := {
	"m4":
	[
		"Body",
		"Magazine",
		"Magazine/Rounds",
		"MagazineSpare",
		"ChargingHandle",
		"BoltCarrier",
		"DustCover",
		"Selector",
		"Trigger",
		"RearSight"
	],
	"m870": ["Body", "Pump", "Pump/BoltCarrier", "Trigger"],
	"lsw6":
	[
		"Body",
		"Bipod",
		"Magazine",
		"Magazine/Rounds",
		"MagazineSpare",
		"ChargingHandle",
		"Selector",
		"Trigger",
		"MagRelease",
		"OpticRail"
	],
	"ak47":
	[
		"Body",
		"Magazine",
		"Magazine/Rounds",
		"MagazineSpare",
		"ChargingHandle",
		"Selector",
		"Trigger",
		"MagRelease",
		"OpticRail"
	],
	"pistol":
	[
		"Body",
		"Slide",
		"Slide/SightGlow",
		"Magazine",
		"Magazine/Rounds",
		"MagazineSpare",
		"Hammer",
		"SlideStop",
		"Safety",
		"Trigger",
		"MagRelease"
	],
	"pdw9":
	[
		"Body",
		"Magazine",
		"Magazine/Rounds",
		"MagazineSpare",
		"ChargingHandle",
		"BoltCarrier",
		"Selector",
		"Trigger",
		"MagRelease",
		"OpticRail"
	],
	"br3":
	[
		"Body",
		"Magazine",
		"Magazine/Rounds",
		"MagazineSpare",
		"ChargingHandle",
		"BoltCarrier",
		"BoltStop",
		"Safety",
		"Selector",
		"Trigger",
		"MagRelease",
		"OpticRail"
	],
	"m40":
	["Body", "Scope", "Bolt", "BoltStop", "Magazine", "Magazine/Rounds", "Safety", "Trigger"],
}
const MARKERS := ["Muzzle", "Ejection", "GripRight", "GripLeft"]
const CASINGS := "res://assets/weapons/shared/casings.glb"
const SUPPRESSOR := "res://assets/weapons/suppressor/suppressor.glb"
const FOREGRIP := "res://assets/weapons/foregrip/foregrip.glb"
## Weapons aimed with their own iron sights; the others carry a reflex sight
## (the scene's IronSights node) or, the M40, a scope on the sight line.
const IRON_SIGHTS := ["m4", "m870", "lsw6", "ak47", "pistol"]
## How far behind the muzzle the front sight sits, at most (the M4A1's post
## stands on the gas block, well back from its flash hider).
const FRONT_SIGHT_REACH := {"m4": 0.23}
## How far (metres) the front sight's tip may sit off the sight line.
const SIGHT_TOLERANCE := 0.0006
## Seen from the eye, how far below the front sight's tip (metres, at the
## front sight) the post must still be what the eye sees: a little way down
## either side of the line, and further down on it, so the receiver or the
## rear sight does not hide most of the post or bead.
const POST_BELOW := 0.001
const POST_SIDE := 0.0006
const POST_DEPTH := 0.0025

## Least share of sampled texels whose normal leans off flat, and of
## neighbouring samples whose brightness steps, for a weapon's maps to count
## as detailed. The source-repacked guns sit at 0.12 to 0.61 and 0.16 to
## 0.48; the M4A1's flat source maps were 0.003 and 0.015.
const NORMAL_DETAIL := 0.08
const ALBEDO_DETAIL := 0.08
## Sample every this many texels.
const DETAIL_STRIDE := 8

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for key: String in WEAPONS:
		var view := (
			(load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene).instantiate() as Node3D
		)
		root.add_child(view)
		await process_frame
		_check_view(key, view)
		view.queue_free()
		await process_frame
		# Other players' weapons draw the merged model, not the parts.
		var world := (
			(load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene).instantiate() as Node3D
		)
		world.set("world_model", true)
		root.add_child(world)
		await process_frame
		_check(
			(world.get_node("Model/Mesh/World") as Node3D).visible,
			"%s: third person draws the merged model" % key
		)
		_check(
			not (world.get_node("Model/Mesh/Body") as Node3D).visible,
			"%s: third person hides the separate parts" % key
		)
		var magazine := world.get_node_or_null("Model/Mesh/Magazine") as Node3D
		_check(
			magazine == null or not magazine.visible,
			"%s: third person draws the magazine in the merged model" % key
		)
		# Every copy of a gun shares the imported material, so racks and other
		# players' weapons do not each carry their own.
		var merged := world.get_node("Model/Mesh/World") as MeshInstance3D
		_check(
			merged.get_surface_override_material(0) == null,
			"%s: third person keeps the shared material" % key
		)
		_check(merged.lod_bias < 1.0, "%s: third person switches to coarser LODs sooner" % key)
		world.queue_free()
		await process_frame
	await _check_extended_magazine()
	for key: String in WEAPONS:
		_check_surface_detail(key)
	_check_casings()
	_check_attachments()
	await _check_optics()
	await _check_racks()
	print("weapon models test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check_view(key: String, view: Node3D) -> void:
	var model := view.get_node("Model") as Node3D
	_check(
		model.transform.is_equal_approx(Transform3D.IDENTITY),
		"%s: the model sits on the view's origin, unscaled" % key
	)
	if key in IRON_SIGHTS:
		_check(
			view.get_node_or_null("Sights") == null, "%s: aims with the model's own sights" % key
		)
	elif key != "m40":
		var sights := view.get_node_or_null("Sights")
		_check(
			sights != null and sights.get("style") == "reflex", "%s: carries a reflex sight" % key
		)
		_check(
			sights != null and (sights.get("glow") as Color).a > 0.0,
			"%s: the reflex sight has a lit dot" % key
		)
	for part: String in WEAPONS[key]:
		_check(view.has_node("Model/Mesh/" + part), "%s: has %s" % [key, part])
	_check(
		not (view.get_node("Model/Mesh/World") as Node3D).visible,
		"%s: first person hides the merged model" % key
	)
	if "MagazineSpare" in WEAPONS[key]:
		var spare := view.get_node_or_null("Model/Mesh/MagazineSpare") as Node3D
		_check(spare != null and not spare.visible, "%s: the spare magazine starts hidden" % key)
	for marker: String in MARKERS:
		_check(view.has_node(marker), "%s: has the %s marker" % [key, marker])
	var muzzle := (view.get_node("Muzzle") as Node3D).position
	_check(
		muzzle.z < -0.1 and muzzle.y < 0.0,
		"%s: the muzzle is ahead of and below the sights (%s)" % [key, muzzle]
	)
	for grip: String in ["GripRight", "GripLeft"]:
		var at := (view.get_node(grip) as Node3D).position
		_check(
			at.y < -0.02 and absf(at.x) < 0.01,
			"%s: %s is under the sight line (%s)" % [key, grip, at]
		)
	if key not in IRON_SIGHTS:
		return
	# A pistol's front sight rides on its slide.
	var sight_part := "Model/Mesh/Slide" if view.has_node("Model/Mesh/Slide") else "Model/Mesh/Body"
	var reach: float = FRONT_SIGHT_REACH.get(key, 0.12)
	var tip := _front_sight_tip(view.get_node(sight_part) as MeshInstance3D, view, muzzle.z, reach)
	_check(
		absf(tip.y) < SIGHT_TOLERANCE,
		"%s: the front sight's tip is on the sight line (%.4f m off)" % [key, tip.y]
	)
	_check_sight_picture(key, view, tip.z)


## Looks down the sights from where the eye is when aimed (the view sits at
## `ads_position` from the camera).
func _check_sight_picture(key: String, view: Node3D, front_z: float) -> void:
	var tris := _view_triangles(view)
	var eye := -(view.get("ads_position") as Vector3)
	# Just over the front sight, the line is clear all the way to it.
	var over: Variant = _first_hit(tris, eye, Vector3(0.0, 0.0005, front_z))
	_check(
		over == null or (over as Vector3).z < front_z - 0.002,
		"%s: the sight line runs clear through the rear sight to the front" % key
	)
	# Just under the tip, and either side of the line, the eye sees the front
	# sight itself.
	for at: Vector2 in [
		Vector2(-POST_SIDE, -POST_BELOW),
		Vector2(0.0, -POST_BELOW),
		Vector2(POST_SIDE, -POST_BELOW),
		Vector2(0.0, -POST_DEPTH),
	]:
		var hit: Variant = _first_hit(tris, eye, Vector3(at.x, at.y, front_z))
		var seen := "nothing"
		if hit != null:
			seen = "%.0f mm from it" % (((hit as Vector3).z - front_z) * 1000.0)
		_check(
			hit != null and absf((hit as Vector3).z - front_z) < 0.008,
			(
				"%s: %.1f mm beside the line and %.1f mm under the tip, the eye sees the front sight (%s)"
				% [key, at.x * 1000.0, -at.y * 1000.0, seen]
			)
		)


## Every triangle of the first-person model, in the view's space, in
## millimetres: Geometry3D takes a triangle smaller than about 3 mm across in
## metres for one edge-on to the ray, and sights are made of those.
func _view_triangles(view: Node3D) -> PackedVector3Array:
	var tris := PackedVector3Array()
	for node: Node in view.get_node("Model").find_children("*", "MeshInstance3D", true, false):
		var instance := node as MeshInstance3D
		if not instance.is_visible_in_tree():
			continue
		var to_view := view.global_transform.affine_inverse() * instance.global_transform
		for v in instance.mesh.get_faces():
			tris.append(to_view * v * 1000.0)
	return tris


## The nearest point (metres) where the ray from `from` toward `to` meets one
## of `tris` (millimetres).
func _first_hit(tris: PackedVector3Array, from_m: Vector3, to_m: Vector3) -> Variant:
	var from := from_m * 1000.0
	var dir := (to_m - from_m).normalized()
	var best: Variant = null
	var best_d := INF
	for i in range(0, tris.size(), 3):
		var hit: Variant = Geometry3D.ray_intersects_triangle(
			from, dir, tris[i], tris[i + 1], tris[i + 2]
		)
		if hit != null:
			var d := from.distance_to(hit as Vector3)
			if d < best_d:
				best_d = d
				best = (hit as Vector3) / 1000.0
	return best


## The highest point of the front sight near the centre line in the last
## stretch before the muzzle: the post's or bead's tip, in the view's space.
func _front_sight_tip(body: MeshInstance3D, view: Node3D, muzzle_z: float, reach: float) -> Vector3:
	var to_view := view.global_transform.affine_inverse() * body.global_transform
	var top := Vector3(0.0, -INF, 0.0)
	var mesh := body.mesh
	for surface in mesh.get_surface_count():
		var vertices: PackedVector3Array = mesh.surface_get_arrays(surface)[Mesh.ARRAY_VERTEX]
		for v in vertices:
			var p := to_view * v
			if absf(p.x) < 0.0015 and p.z < muzzle_z + reach and p.y > -0.05 and p.y > top.y:
				top = p
	return top


## An extended magazine is longer, and still seats where the standard one does.
func _check_extended_magazine() -> void:
	for key: String in ["m4", "ak47", "pdw9"]:
		var view := (
			(load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene).instantiate() as Node3D
		)
		root.add_child(view)
		await process_frame
		var mag := view.get_node("Model/Mesh/Magazine") as MeshInstance3D
		var before := _bounds_in(mag, view)
		view.call("apply_attachment", "extmag")
		await process_frame
		var after := _bounds_in(mag, view)
		_check(
			after.size.y > before.size.y * 1.25,
			(
				"%s: the extended magazine is longer (%.0f to %.0f mm)"
				% [key, before.size.y * 1000.0, after.size.y * 1000.0]
			)
		)
		_check(
			absf(after.end.y - before.end.y) < 0.002,
			"%s: the extended magazine's top stays in the well" % key
		)
		view.call("apply_attachment", "")
		await process_frame
		_check(
			absf(_bounds_in(mag, view).size.y - before.size.y) < 0.001,
			"%s: taking it off restores the magazine" % key
		)
		view.queue_free()
		await process_frame


func _bounds_in(mesh: MeshInstance3D, space: Node3D) -> AABB:
	return space.global_transform.affine_inverse() * mesh.global_transform * mesh.get_aabb()


## Every weapon names a modelled fired case, and the case is its real size.
func _check_surface_detail(key: String) -> void:
	var base := ProjectSettings.globalize_path("res://assets/weapons/%s/%s" % [key, key])
	var normal := Image.load_from_file(base + "_normal.png")
	var albedo := Image.load_from_file(base + "_albedo.png")
	var leaning := 0
	var steps := 0
	var samples := 0
	for y in range(0, normal.get_height(), DETAIL_STRIDE):
		for x in range(0, normal.get_width() - DETAIL_STRIDE, DETAIL_STRIDE):
			var n := normal.get_pixel(x, y)
			var off := maxf(maxf(absf(n.r - 0.5), absf(n.g - 0.5)), absf(n.b - 1.0))
			leaning += int(off > 0.05)
			var step := absf(
				(
					albedo.get_pixel(x + DETAIL_STRIDE, y).get_luminance()
					- albedo.get_pixel(x, y).get_luminance()
				)
			)
			steps += int(step > 0.02)
			samples += 1
	_check(
		float(leaning) / samples >= NORMAL_DETAIL,
		"%s: normal map has detail (%.3f)" % [key, float(leaning) / samples]
	)
	_check(
		float(steps) / samples >= ALBEDO_DETAIL,
		"%s: colour map has detail (%.3f)" % [key, float(steps) / samples]
	)


func _check_casings() -> void:
	var casings := (load(CASINGS) as PackedScene).instantiate()
	for key: String in WEAPONS:
		var view := (load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene).instantiate()
		var case_name: String = view.get("shell_model")
		var mesh := casings.find_child(case_name) as MeshInstance3D
		_check(mesh != null, "%s ejects a modelled case (%s)" % [key, case_name])
		if mesh != null:
			var length := mesh.get_aabb().size.y
			_check(
				length > 0.018 and length < 0.072,
				"%s: the %s is %.1f mm long" % [key, case_name, length * 1000.0]
			)
		view.free()
	casings.free()


## The suppressor and foregrip are built in metres, mounting at their origin.
func _check_attachments() -> void:
	for path: String in [SUPPRESSOR, FOREGRIP]:
		var model := (load(path) as PackedScene).instantiate() as Node3D
		var body := model.find_child("Body") as MeshInstance3D
		_check(body != null, "%s has a Body" % path)
		if body != null:
			var box := body.get_aabb()
			var long := maxf(box.size.y, box.size.z)
			_check(long > 0.08 and long < 0.2, "%s is %.0f mm long" % [path, long * 1000.0])
		model.free()


func _check_optics() -> void:
	# The optics draw their modelled housings (tools/weapon_optics.py) round
	# the glass and reticle built in code: the red dot's in its sight mesh,
	# the ACOG's eye guard there and the rest in its body.
	var view := (load("res://scenes/weapons/m4_view.tscn") as PackedScene).instantiate() as Node3D
	root.add_child(view)
	await process_frame
	for optic: Array in [["reddot", "SightMesh", 2000], ["acog", "SightBody", 5000]]:
		view.call("apply_attachment", optic[0])
		await process_frame
		var mesh := view.get_node_or_null("Attachment/Optic/%s" % optic[1]) as MeshInstance3D
		var triangles := 0 if mesh == null else mesh.mesh.get_faces().size() / 3
		_check(
			triangles >= optic[2],
			"%s: draws its modelled housing (%d triangles)" % [optic[0], triangles]
		)
	view.queue_free()
	await process_frame


func _check_racks() -> void:
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	await process_frame
	for rack: Node in main.get_node("Armory").get_children():
		var display := rack.get_node("Display") as Node3D
		var meshes := display.find_children("*", "MeshInstance3D", true, false)
		_check(meshes.size() > 0, "%s shows its weapon" % rack.name)
		var shadows := meshes.filter(
			func(m: MeshInstance3D) -> bool:
				return m.cast_shadow != GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		)
		_check(shadows.is_empty(), "%s: the weapon on it casts no shadow" % rack.name)
	main.queue_free()
	await process_frame


func _check(condition: bool, label: String) -> void:
	if condition:
		print("  ok: ", label)
	else:
		_failures += 1
		push_error("FAIL: " + label)
