extends SceneTree
## Headless weapon model test. For every weapon rebuilt from the source packs
## (tools/build_weapons.py, models in metres): the view scene has its moving
## parts and markers, the spare magazine starts hidden, the muzzle is ahead of
## the sight and the grips below it, and the front sight's tip lies on the
## sight line (the view's -Z axis), so aimed shots land where the sights
## point. Then every armory rack shows its weapon.
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
## Weapons aimed with their own iron sights; the others carry a reflex sight
## (the scene's IronSights node) or, the M40, a scope on the sight line.
const IRON_SIGHTS := ["m4", "m870", "lsw6", "ak47", "pistol"]
## How far behind the muzzle the front sight sits, at most (the M4A1's post
## stands on the gas block, well back from its flash hider).
const FRONT_SIGHT_REACH := {"m4": 0.23}
## How far (metres) the front sight's tip may sit off the sight line.
const SIGHT_TOLERANCE := 0.0006

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
		absf(tip) < SIGHT_TOLERANCE,
		"%s: the front sight's tip is on the sight line (%.4f m off)" % [key, tip]
	)


## Height of the highest point of the body near the centre line in the last
## stretch before the muzzle: the front sight post's tip.
func _front_sight_tip(body: MeshInstance3D, view: Node3D, muzzle_z: float, reach: float) -> float:
	var to_view := view.global_transform.affine_inverse() * body.global_transform
	var top := -INF
	var mesh := body.mesh
	for surface in mesh.get_surface_count():
		var vertices: PackedVector3Array = mesh.surface_get_arrays(surface)[Mesh.ARRAY_VERTEX]
		for v in vertices:
			var p := to_view * v
			if absf(p.x) < 0.0015 and p.z < muzzle_z + reach and p.y > -0.05:
				top = maxf(top, p.y)
	return top


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
