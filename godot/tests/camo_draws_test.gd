extends SceneTree
## Camo test: a camo paints a weapon without adding a draw call, in first
## person and third person, and taking it off puts the weapon's own
## materials back. Needs a renderer for the draw count (skipped headless).
## godot --path godot --script res://tests/camo_draws_test.gd

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	var stage := Node3D.new()
	root.add_child(stage)
	var camera := Camera3D.new()
	stage.add_child(camera)
	camera.position = Vector3(0.0, 0.0, 1.2)
	camera.make_current()
	var light := DirectionalLight3D.new()
	stage.add_child(light)
	var scene := load("res://scenes/weapons/m4_view.tscn") as PackedScene
	var first := scene.instantiate()
	stage.add_child(first)
	var world := scene.instantiate()
	world.set(&"world_model", true)
	stage.add_child(world)
	await _frames(5)
	# Headless draws nothing (and never posts a frame): no count there.
	var drawing := DisplayServer.get_name() != "headless"
	var before: int = await _draws() if drawing else 0
	var materials := _materials(world)
	first.call(&"apply_camo", "gold")
	world.call(&"apply_camo", "gold")
	await _frames(5)
	var painted := _materials(world)
	_check(painted != materials, "third person: the camo replaces the weapon's material")
	_check(_camo_on(first), "first person: the camo is on in the weapon's own material")
	if drawing:
		var after: int = await _draws()
		_check(after == before, "a camo adds no draw call (%d, then %d)" % [before, after])
	first.call(&"apply_camo", "none")
	world.call(&"apply_camo", "none")
	await _frames(2)
	_check(_materials(world) == materials, "the factory finish puts the materials back")
	_check(not _camo_on(first), "and turns the first-person camo off")
	print("Camo test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _draws() -> int:
	await RenderingServer.frame_post_draw
	return RenderingServer.get_rendering_info(
		RenderingServer.RENDERING_INFO_TOTAL_DRAW_CALLS_IN_FRAME
	)


## Every surface's material under `node`, in order.
func _materials(node: Node) -> Array:
	var out := []
	for mesh: MeshInstance3D in node.find_children("*", "MeshInstance3D", true, false):
		for surface in mesh.get_surface_override_material_count():
			out.append(mesh.get_active_material(surface))
	return out


func _camo_on(node: Node) -> bool:
	for material: Variant in _materials(node):
		if (
			material is ShaderMaterial
			and (material as ShaderMaterial).get_shader_parameter("camo_on") == true
		):
			return true
	return false


func _frames(count: int) -> void:
	for i in count:
		await process_frame
