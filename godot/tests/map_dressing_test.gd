extends SceneTree
## The maps' set dressing (tools/map_dressing.py). In the Testing Area: its large props
## collide where they are drawn, the way down the range stays open, its meshes
## are set up to render cheaply (shadow twins, shadowless ground and detail,
## detail hidden at a distance), and Low drops the detail and uses two shadow
## splits (scripts/map_render.gd). In the Zombies arena: the barricades and the
## burnt-out car collide where they are drawn, and the way in from every window
## and gate stays clear.
## Run from the repo root:
##   godot --headless --path godot --script res://tests/map_dressing_test.gd

const GraphicsQuality := preload("res://scripts/graphics.gd")
const SCENE := "res://scenes/main.tscn"
const WORLD_LAYER := 1

## Rays that must hit a prop's collision: [name, from, to, the distance along
## the ray where its drawn face is, tolerance].
const MUST_HIT := [
	["container side", Vector3(25, 1.2, -4), Vector3(32, 1.2, -4), 4.5, 0.1],
	["tower leg", Vector3(26.6, 1.0, -22), Vector3(26.6, 1.0, -32), 3.99, 0.1],
	["tower cabin", Vector3(28.9, 5.0, -20), Vector3(28.9, 5.0, -32), 5.85, 0.1],
	["site office", Vector3(-28.4, 1.2, 24), Vector3(-28.4, 1.2, 32), 5.2, 0.1],
	["sandbag position", Vector3(-28.0, 0.4, -20), Vector3(-28.0, 0.4, -30), 4.75, 0.15],
	["steel target", Vector3(25, 1.2, 5), Vector3(32, 1.2, 5), 5.4, 0.1],
	["target stand", Vector3(25, 1.2, 20.2), Vector3(32, 1.2, 20.2), 5.05, 0.1],
	["gate car", Vector3(20, 0.7, 28.6), Vector3(32, 0.7, 28.6), 5.75, 0.25],
]
## Rays that must stay clear: the lanes the dressing must not block.
const MUST_MISS := [
	["spawn to the north wall", Vector3(2.7, 1.5, 20), Vector3(2.7, 1.5, -31)],
	["armoury line", Vector3(-4.8, 1.0, 8), Vector3(-4.8, 1.0, 26)],
	["through the culvert", Vector3(-10, 0.7, -12), Vector3(-10, 0.7, -24)],
	["vehicle route in from the gate", Vector3(21, 1.0, 30), Vector3(21, 1.0, 17)],
	["across the middle", Vector3(-25, 1.0, 0.5), Vector3(25, 1.0, 0.5)],
]

const ZOMBIES := "res://scenes/zombies_arena.tscn"
const ZOMBIES_HIT := [
	["south barricade", Vector3(-5, 0.6, 18), Vector3(-5, 0.6, 24), 5.13, 0.1],
	["east barricade", Vector3(18, 0.6, 13.5), Vector3(24, 0.6, 13.5), 5.13, 0.1],
	["burnt-out car", Vector3(-15, 0.7, -4), Vector3(-25, 0.7, -4), 6.85, 0.2],
]
## From each window's climb-in point and each gate, 6 m into the yard.
const ZOMBIES_MISS := [
	["north-west window", Vector3(-14, 1.0, -22.9), Vector3(-14, 1.0, -16.9)],
	["north-east window", Vector3(14, 1.0, -22.9), Vector3(14, 1.0, -16.9)],
	["south-west window", Vector3(-14, 1.0, 22.9), Vector3(-14, 1.0, 16.9)],
	["south-east window", Vector3(14, 1.0, 22.9), Vector3(14, 1.0, 16.9)],
	["west windows", Vector3(-22.9, 1.0, -17), Vector3(-16.9, 1.0, -17)],
	["west window, south", Vector3(-22.9, 1.0, 22), Vector3(-16.9, 1.0, 22)],
	["east windows", Vector3(22.9, 1.0, -17), Vector3(16.9, 1.0, -17)],
	["east window, south", Vector3(22.9, 1.0, 22), Vector3(16.9, 1.0, 22)],
	["west gate", Vector3(-23, 1.0, 4), Vector3(-17, 1.0, 4)],
	["east gate", Vector3(23, 1.0, 4), Vector3(17, 1.0, 4)],
]

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var settings := root.get_node_or_null(^"Settings")
	settings.path = "user://map_dressing_test_settings.cfg"
	settings.set_graphics_preset(GraphicsQuality.Preset.MEDIUM)
	var scene: Node = (load(SCENE) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	root.add_child(scene)
	current_scene = scene
	for i in 5:
		await physics_frame
	var space := (scene as Node3D).get_world_3d().direct_space_state
	for check: Array in MUST_HIT:
		var hit := _ray(space, check[1], check[2])
		var distance: float = (
			INF if hit.is_empty() else (hit["position"] as Vector3).distance_to(check[1])
		)
		_check(
			absf(distance - float(check[3])) <= float(check[4]),
			(
				"the %s collides where it is drawn (%.2f m along the ray, want %.2f)"
				% [check[0], distance, check[3]]
			)
		)
	for check: Array in MUST_MISS:
		var hit := _ray(space, check[1], check[2])
		_check(
			hit.is_empty(),
			(
				"the %s is clear%s"
				% [check[0], "" if hit.is_empty() else " (hit %s)" % hit["position"]]
			)
		)

	var meshes: Array[MeshInstance3D] = []
	for node in scene.find_children("map_*", "MeshInstance3D", true, false):
		meshes.append(node as MeshInstance3D)
	var names := meshes.map(func(m: MeshInstance3D) -> String: return String(m.name))
	for wanted: String in [
		"map_testing_area",
		"map_testing_area_ground",
		"map_testing_area_shadow",
		"map_testing_dressing_n",
		"map_testing_dressing_s",
		"map_testing_dressing_ground",
		"map_testing_dressing_shadow",
		"map_testing_dressing_se_detail",
	]:
		_check(wanted in names, "the map has a %s mesh" % wanted)
	for mesh in meshes:
		var name := String(mesh.name)
		var casts := mesh.cast_shadow
		if name.ends_with("_shadow"):
			_check(
				mesh.visible and casts == GeometryInstance3D.SHADOW_CASTING_SETTING_SHADOWS_ONLY,
				"%s is a visible shadow-only twin" % name
			)
		else:
			_check(
				casts == GeometryInstance3D.SHADOW_CASTING_SETTING_OFF,
				"%s leaves its shadow to the twin or casts none" % name
			)
		if name.ends_with("_detail"):
			_check(
				mesh.visibility_range_end > 0.0 and mesh.visible,
				"%s hides beyond a distance and shows on Medium" % name
			)

	var sun := scene.get_node("Sun") as DirectionalLight3D
	var four := sun.directional_shadow_mode
	settings.set_graphics_preset(GraphicsQuality.Preset.LOW)
	await process_frame
	_check(
		sun.directional_shadow_mode == DirectionalLight3D.SHADOW_PARALLEL_2_SPLITS,
		"Low gives the sun two shadow splits"
	)
	var details := meshes.filter(
		func(m: MeshInstance3D) -> bool: return m.name.ends_with("_detail")
	)
	_check(
		details.all(func(m: MeshInstance3D) -> bool: return not m.visible),
		"Low hides the dressing's detail"
	)
	settings.set_graphics_preset(GraphicsQuality.Preset.MEDIUM)
	await process_frame
	_check(sun.directional_shadow_mode == four, "Medium restores the sun's own splits")

	scene.queue_free()
	await process_frame
	await _zombies()

	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(settings.path))
	print("Map dressing test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _zombies() -> void:
	var scene: Node = (load(ZOMBIES) as PackedScene).instantiate()
	var director := scene.get_node_or_null("Director")
	if director != null:
		director.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	current_scene = scene
	for i in 5:
		await physics_frame
	var space := (scene as Node3D).get_world_3d().direct_space_state
	for check: Array in ZOMBIES_HIT:
		var hit := _ray(space, check[1], check[2])
		var distance: float = (
			INF if hit.is_empty() else (hit["position"] as Vector3).distance_to(check[1])
		)
		_check(
			absf(distance - float(check[3])) <= float(check[4]),
			(
				"the %s collides where it is drawn (%.2f m along the ray, want %.2f)"
				% [check[0], distance, check[3]]
			)
		)
	for check: Array in ZOMBIES_MISS:
		var hit := _ray(space, check[1], check[2])
		_check(
			hit.is_empty(),
			(
				"the way in from the %s is clear%s"
				% [check[0], "" if hit.is_empty() else " (hit %s)" % hit["position"]]
			)
		)
	var names := scene.find_children("map_zombies_dressing*", "MeshInstance3D", true, false).map(
		func(m: Node) -> String: return String(m.name)
	)
	_check("map_zombies_dressing_shadow" in names, "the arena's dressing has a shadow twin")
	scene.queue_free()
	await process_frame


func _ray(space: PhysicsDirectSpaceState3D, from: Vector3, to: Vector3) -> Dictionary:
	var query := PhysicsRayQueryParameters3D.create(from, to, WORLD_LAYER)
	return space.intersect_ray(query)


func _check(ok: bool, what: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", what])
	if not ok:
		_failures += 1
