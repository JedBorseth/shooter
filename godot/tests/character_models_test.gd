extends SceneTree
## Headless test of the character models from tools/build_character_assets.py:
## the soldier, the first-person arms, and the zombie keep the game's skeleton
## (every bone, its parent, and its rest pose match
## tests/data/soldier_skeleton.json, taken from the mannequin soldier the
## animations, IK, and hit zones were measured on); each draws one mesh with one
## surface and one material, plus a decimated far mesh for the third-person
## kits; triangle budgets hold; a soldier gets its team's atlas material with a
## per-character kit; a zombie wears the zombie kit; and every zombie body
## variant is retargeted onto the same skeleton with a compressed atlas.
## Run from the repo root:
##   godot --headless --path godot --script res://tests/character_models_test.gd

const SOLDIER_TRIANGLES := 26000
const ARMS_TRIANGLES := 26000
const ZOMBIE_TRIANGLES := 14000

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var reference: Array = JSON.parse_string(
		FileAccess.get_file_as_string("res://tests/data/soldier_skeleton.json")
	)
	_check(reference != null and reference.size() == 53, "the reference skeleton has 53 bones")

	for spec in [
		["res://assets/characters/soldier/soldier.glb", "Body", SOLDIER_TRIANGLES],
		["res://assets/characters/arms/arms.glb", "ArmsFP", ARMS_TRIANGLES],
		["res://assets/characters/zombie/zombie.glb", "Body", ZOMBIE_TRIANGLES],
		["res://assets/characters/zombie_b/zombie_b.glb", "Body", ZOMBIE_TRIANGLES],
		["res://assets/characters/zombie_c/zombie_c.glb", "Body", ZOMBIE_TRIANGLES],
		["res://assets/characters/zombie_d/zombie_d.glb", "Body", ZOMBIE_TRIANGLES],
		["res://assets/characters/zombie_e/zombie_e.glb", "Body", ZOMBIE_TRIANGLES],
	]:
		var path: String = spec[0]
		# Every kit's atlas imports compressed with mipmaps, like the rest.
		var atlas := load(path.replace(".glb", "_albedo.png")) as Texture2D
		var image := atlas.get_image() if atlas != null else null
		_check(
			image != null and image.is_compressed() and image.has_mipmaps(),
			"%s: its atlas is VRAM-compressed with mipmaps" % path
		)
		var scene := (load(path) as PackedScene).instantiate()
		var skeleton := scene.find_child("GeneralSkeleton", true, false) as Skeleton3D
		_check(skeleton != null, "%s has GeneralSkeleton" % path)
		if skeleton == null:
			continue
		if reference != null:
			_same_skeleton(path, skeleton, reference)
		var meshes: Array[MeshInstance3D] = []
		for child in skeleton.get_children():
			if child is MeshInstance3D:
				meshes.append(child)
		# Third-person kits add a decimated far mesh, BodyFar.
		var far: MeshInstance3D = null
		for m in meshes:
			if m.name == "BodyFar":
				far = m
		if far != null:
			meshes.erase(far)
			var far_triangles := far.mesh.get_faces().size() / 3
			_check(
				far_triangles * 3 <= spec[2],
				"%s: the far mesh is a third of the budget or less (%d)" % [path, far_triangles]
			)
			_check(
				far.mesh.get_surface_count() == 1, "%s: the far mesh draws in one surface" % path
			)
		elif spec[1] == "Body":
			_check(false, "%s has a far mesh" % path)
		_check(meshes.size() == 1, "%s has one near mesh (found %d)" % [path, meshes.size()])
		if meshes.is_empty():
			continue
		var mesh := meshes[0]
		_check(mesh.name == spec[1], "%s's mesh is %s (found %s)" % [path, spec[1], mesh.name])
		_check(mesh.mesh.get_surface_count() == 1, "%s draws in one surface" % path)
		var material := mesh.mesh.surface_get_material(0)
		_check(
			material != null and String(material.resource_name).begins_with("character_"),
			"%s's material is a character kit" % path
		)
		var arrays := mesh.mesh.surface_get_arrays(0)
		var triangles: int = (arrays[Mesh.ARRAY_INDEX] as PackedInt32Array).size() / 3
		_check(
			triangles <= spec[2],
			"%s: %d triangles within the budget of %d" % [path, triangles, spec[2]]
		)
		_check(arrays[Mesh.ARRAY_TANGENT] != null, "%s has tangents for its normal map" % path)
		print(
			(
				"%s: %d triangles, %d vertices"
				% [path, triangles, (arrays[Mesh.ARRAY_VERTEX] as PackedVector3Array).size()]
			)
		)
		scene.free()

	# A soldier in the tree gets its team's atlas material and a kit variant.
	var soldiers: Array[SoldierRig] = []
	for team in 2:
		var soldier := (
			(load("res://scenes/characters/soldier.tscn") as PackedScene).instantiate()
			as SoldierRig
		)
		root.add_child(soldier)
		soldier.set_team_index(team)
		soldiers.append(soldier)
	await process_frame
	var materials := []
	for soldier in soldiers:
		var body := soldier.find_child("Body", true, false) as MeshInstance3D
		var material := body.get_surface_override_material(0) as ShaderMaterial
		_check(material != null, "a soldier draws with the character shader")
		if material == null:
			continue
		_check(
			material.get_shader_parameter("albedo_texture") is Texture2D,
			"the soldier's material has its baked atlas"
		)
		_check(
			body.get_instance_shader_parameter("hidden_parts") != null, "each soldier picks its kit"
		)
		materials.append(material)
	if materials.size() == 2:
		_check(materials[0] != materials[1], "the teams draw with different materials")
	for soldier in soldiers:
		soldier.queue_free()
	await process_frame

	# A zombie wears its own kit: a whole body with nothing cut away under gear.
	var zombie := (load("res://scenes/zombies/zombie.tscn") as PackedScene).instantiate()
	root.add_child(zombie)
	await process_frame
	var zombie_body := zombie.find_child("Body", true, false) as MeshInstance3D
	_check(zombie_body != null, "a zombie has a body")
	if zombie_body != null:
		var zombie_material := zombie_body.get_surface_override_material(0) as ShaderMaterial
		_check(
			zombie_material != null and zombie_material.get_shader_parameter("zombie") == 1.0,
			"a zombie draws with the zombie palette"
		)
		var albedo: Texture2D = (
			zombie_material.get_shader_parameter("albedo_texture") if zombie_material else null
		)
		_check(
			albedo != null and albedo.resource_path.contains("/characters/zombie"),
			"a zombie uses a zombie atlas (got %s)" % (albedo.resource_path if albedo else "none")
		)
	zombie.queue_free()
	await process_frame

	if _failures.is_empty():
		print("character models test passed")
		quit(0)
	else:
		for failure in _failures:
			printerr("FAIL: ", failure)
		quit(1)


func _same_skeleton(path: String, skeleton: Skeleton3D, reference: Array) -> void:
	_check(
		skeleton.get_bone_count() == reference.size(),
		"%s keeps %d bones (has %d)" % [path, reference.size(), skeleton.get_bone_count()]
	)
	for bone: Dictionary in reference:
		var j := skeleton.find_bone(bone["name"])
		if j < 0:
			_check(false, "%s has bone %s" % [path, bone["name"]])
			continue
		var parent := skeleton.get_bone_parent(j)
		_check(
			("" if parent < 0 else String(skeleton.get_bone_name(parent))) == bone["parent"],
			"%s: %s keeps its parent" % [path, bone["name"]]
		)
		var rest := skeleton.get_bone_global_rest(j)
		var o: Array = bone["origin"]
		var m: Array = bone["basis"]
		var basis := Basis(
			Vector3(m[0], m[1], m[2]), Vector3(m[3], m[4], m[5]), Vector3(m[6], m[7], m[8])
		)
		_check(
			(
				rest.origin.distance_to(Vector3(o[0], o[1], o[2])) < 0.001
				and rest.basis.is_equal_approx(basis)
			),
			"%s: %s keeps its rest pose" % [path, bone["name"]]
		)


func _check(condition: bool, message: String) -> void:
	if not condition:
		_failures.append(message)
