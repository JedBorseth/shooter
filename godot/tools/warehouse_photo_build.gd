@tool
extends RefCounted
## Rebuilds Warehouse from res://tools/warehouse_photo_layout.json, which
## tools/map_blockout/warehouse_photo_layout.py writes from the reference
## photo measurements. Run it in the editor (for example through the Godot AI
## MCP bridge's execute_gdscript):
##
##     load("res://tools/warehouse_photo_build.gd").build(EditorInterface.get_edited_scene_root())
##
## then save the scene. Shell walls, floors, roofs, and the CMU platform are
## Cyclops blocks so they stay editable with the Cyclops tools; everything
## else lives under PhotoPass and is rebuilt on every run.

const LAYOUT := "res://tools/warehouse_photo_layout.json"
## Clip boxes fitted to each prop model by tools/build_warehouse_clip.py.
const CLIP := "res://assets/props/warehouse_photo/clip.json"
const PHOTO := "res://assets/props/warehouse_photo/"
const BLOCK := preload("res://addons/cyclops_level_builder/nodes/cyclops_block.gd")
const MATS := "res://materials/warehouse/"
## The baked lightmap (tools/bake_lightmaps.sh res://scenes/warehouse.tscn).
const LIGHTMAP := "res://scenes/warehouse.lmbake"
const BAKED_LIGHTS := "res://scripts/baked_lights.gd"
const KEEP_SHELL := []


static var _root: Node3D
static var _data: Dictionary
static var _scenes := {}
static var _clip := {}
static var _log: PackedStringArray


static func build(root: Node3D, parts: Array = ["shell", "structure", "lighting", "utilities", "machines", "stock", "patches", "boxes", "detail", "effects", "clip", "gameplay"]) -> String:
	_root = root
	_log = PackedStringArray()
	_data = JSON.parse_string(FileAccess.get_file_as_string(LAYOUT))
	_clip = JSON.parse_string(FileAccess.get_file_as_string(CLIP)) if FileAccess.file_exists(CLIP) else {}
	_scenes = {}
	# Effects attach to the high-bays (flicker): rebuilt lights need them again.
	if "lighting" in parts and not "effects" in parts:
		parts = parts + ["effects"]
	# And the office lamps the effects add are baked lights under Lighting.
	if "effects" in parts and not "lighting" in parts:
		parts = parts + ["lighting"]
	if "shell" in parts:
		_clear_old()
		_build_shell()
	var pass_node := _group(root, "PhotoPass")
	for part in ["structure", "lighting", "utilities", "machines", "stock", "patches", "boxes", "detail", "effects"]:
		if part in parts:
			var g := _group(pass_node, part.capitalize())
			for c in g.get_children():
				g.remove_child(c)
				c.free()
			match part:
				"structure": _build_structure(g)
				"lighting": _build_lighting(g)
				"utilities": _build_utilities(g)
				"machines": _build_machines(g)
				"stock": _build_stock(g)
				"patches": _build_patches(g)
				"boxes": _build_boxes(g)
				"detail": _build_detail(g)
				"effects": _build_effects(g)
	if "clip" in parts:
		_clip_dressing()
	if "gameplay" in parts:
		_build_gameplay()
	_log.append("done")
	return "\n".join(_log)


# --- helpers ---------------------------------------------------------------

static func _group(parent: Node, name: String) -> Node3D:
	var g := parent.get_node_or_null(name) as Node3D
	if g == null:
		g = Node3D.new()
		g.name = name
		parent.add_child(g)
		g.owner = _root
	return g


static func _own(node: Node) -> void:
	node.owner = _root
	if node.scene_file_path != "":
		return
	for c in node.get_children():
		_own(c)


static func _mat(key: String) -> Material:
	var names := {"slab": "slab_polished", "tiltup": "tiltup_wall", "panel_white": "panel_white",
		"steel": "steel_deck", "yellow": "safety_yellow", "curtain": "strip_curtain",
		"yard": "yard", "cmu": "cmu", "cap": "cap", "deck": "roof_deck"}
	return load(MATS + names[key] + ".tres")


static func _block(parent: Node, name: String, a: Vector3, b: Vector3, mat: Material) -> Node3D:
	var block = BLOCK.new()
	parent.add_child(block)
	block.owner = _root
	block.name = name
	block.materials.append(mat)
	var bounds := AABB(a, b - a)
	var vol := ConvexVolume.new()
	vol.init_block(bounds, Transform2D.IDENTITY, 0)
	vol.translate(-bounds.position)
	block.mesh_vector_data = vol.to_mesh_vector_data()
	block.global_transform = Transform3D(Basis(), bounds.position)
	return block


static func _scene(model: String) -> PackedScene:
	if _scenes.has(model):
		return _scenes[model]
	var path := PHOTO + model + ".gltf"
	if not ResourceLoader.exists(path):
		path = _data.fallback.get(model, "")
	var ps: PackedScene = load(path) if path != "" and ResourceLoader.exists(path) else null
	if ps == null:
		_log.append("missing model " + model)
	_scenes[model] = ps
	return ps


static func _local_aabb(node: Node3D) -> AABB:
	var out := AABB()
	var first := true
	for m in node.find_children("*", "MeshInstance3D", true, false):
		var xf: Transform3D = node.global_transform.affine_inverse() * m.global_transform
		var a: AABB = xf * m.get_aabb()
		out = a if first else out.merge(a)
		first = false
	return out


## Instance `model` fitted to the envelope `fit` (w, d, h) with its footprint
## centred on (x, z), its base at y, turned `rot` degrees about Y.
static func _place(parent: Node, name: String, model: String, pos: Vector3, rot: float, fit: Array,
		clamp_lo := 0.7, clamp_hi := 1.4, collide := true) -> Node3D:
	var ps := _scene(model)
	if ps == null:
		return null
	var holder := StaticBody3D.new() if collide else Node3D.new()
	holder.name = name
	parent.add_child(holder)
	holder.owner = _root
	var inst: Node3D = ps.instantiate()
	holder.add_child(inst)
	inst.owner = _root
	var box := _local_aabb(inst)
	var s := Vector3.ONE
	if fit.size() == 3 and box.size.x > 0.01:
		s = Vector3(fit[0] / box.size.x, fit[2] / box.size.y, fit[1] / max(box.size.z, 0.01))
		s = s.clamp(Vector3.ONE * clamp_lo, Vector3.ONE * clamp_hi)
	inst.scale = s
	var c := box.get_center() * s
	inst.position = Vector3(-c.x, -box.position.y * s.y, -c.z)
	holder.position = pos
	holder.rotation_degrees.y = rot
	if collide and not _add_clip(holder, inst):
		var shape := CollisionShape3D.new()
		var bs := BoxShape3D.new()
		bs.size = box.size * s
		shape.shape = bs
		shape.position = Vector3(0, bs.size.y / 2, 0)
		holder.add_child(shape)
		shape.owner = _root
	return holder


## Gives `body` the clip boxes fitted to its model `inst` (a child of the body),
## so it collides with what players see instead of one box around the whole
## model. Returns false when the model has no clip boxes.
static func _add_clip(body: Node3D, inst: Node3D) -> bool:
	var boxes: Array = _clip.get(inst.scene_file_path, [])
	if boxes.is_empty():
		return false
	var xf: Transform3D = body.global_transform.affine_inverse() * inst.global_transform
	var scale := xf.basis.get_scale()
	var turn := xf.basis.orthonormalized()
	var k := 0
	for b in boxes:
		var lo := Vector3(b[0], b[1], b[2])
		var hi := Vector3(b[3], b[4], b[5])
		var shape := CollisionShape3D.new()
		shape.name = "Clip%02d" % k
		k += 1
		var bs := BoxShape3D.new()
		bs.size = (hi - lo) * scale
		shape.shape = bs
		shape.transform = Transform3D(turn, xf * ((lo + hi) / 2))
		body.add_child(shape)
		shape.owner = _root
	return true


## The older dressing (offices, yard, cubicles) is placed in the scene, not
## from the layout: refit its colliders to its models too. Bodies without a
## model of their own (the dispatch office's roof slabs, whose ceiling meshes
## are siblings) keep the collision they were authored with.
static func _clip_dressing() -> void:
	var n := 0
	var dressing := _root.get_node_or_null("WarehouseDressing")
	if dressing == null:
		_log.append("clip: no older dressing")
		return
	for body: StaticBody3D in dressing.find_children("*", "StaticBody3D", true, true):
		var model: Node3D = null
		for c in body.get_children():
			if c is Node3D and c.scene_file_path != "":
				model = c
				break
		if model == null or not _clip.has(model.scene_file_path):
			continue
		for c in body.get_children():
			if c is CollisionShape3D:
				body.remove_child(c)
				c.free()
		_add_clip(body, model)
		n += 1
	_log.append("clip: refit %d dressing bodies" % n)


static func _multimesh(parent: Node, name: String, model: String, xforms: Array) -> Array:
	var made := []
	var ps := _scene(model)
	if ps == null or xforms.is_empty():
		return made
	var inst: Node3D = ps.instantiate()
	var meshes := inst.find_children("*", "MeshInstance3D", true, false)
	for i in meshes.size():
		var m: MeshInstance3D = meshes[i]
		var local := Transform3D.IDENTITY
		var n: Node = m
		while n != inst:
			local = (n as Node3D).transform * local
			n = n.get_parent()
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.mesh = m.mesh
		mm.instance_count = xforms.size()
		for k in xforms.size():
			mm.set_instance_transform(k, xforms[k] * local)
		var mmi := MultiMeshInstance3D.new()
		mmi.name = "%s%d" % [name, i]
		mmi.multimesh = mm
		# A lightmap cannot hold a multimesh: lit by the lightmap's probes.
		mmi.gi_mode = GeometryInstance3D.GI_MODE_DYNAMIC
		parent.add_child(mmi)
		mmi.owner = _root
		made.append(mmi)
	inst.free()
	return made


static func _v(a: Array) -> Vector3:
	return Vector3(a[0], a[1], a[2])


# --- shell -----------------------------------------------------------------

static func _clear_old() -> void:
	var shell := _root.get_node("Shell")
	for c in shell.get_children():
		if not (c.name in KEEP_SHELL):
			shell.remove_child(c)
			c.free()
	# The 6v6 rebuild places everything from the layout: the sketch-era cover
	# and the scene-placed dressing (offices, yard, cubicles) go.
	for group in ["BlockoutCover", "WarehouseDressing"]:
		var old := _root.get_node_or_null(group)
		if old:
			_root.remove_child(old)
			old.free()


static func _build_shell() -> void:
	var shell := _root.get_node("Shell")
	for f in _data.floors:
		_block(shell, f[0], Vector3(f[1], f[5], f[3]), Vector3(f[2], f[6], f[4]), _mat(f[7]))
	for w in _data.walls:
		var name: String = w[0]
		var x0: float = w[1]; var x1: float = w[2]; var z0: float = w[3]; var z1: float = w[4]
		var h: float = w[5]
		var mat := _mat(w[6])
		var along_x := (x1 - x0) >= (z1 - z0)
		var start := x0 if along_x else z0
		var end := x1 if along_x else z1
		var cursor := start
		var openings: Array = w[7].duplicate()
		openings.sort_custom(func(a, b): return a[0] < b[0])
		var k := [0]
		var seg := func(s: float, e: float, y0: float, y1: float) -> void:
			if e - s < 0.01:
				return
			k[0] += 1
			var a := Vector3(s, y0, z0) if along_x else Vector3(x0, y0, s)
			var b := Vector3(e, y1, z1) if along_x else Vector3(x1, y1, e)
			_block(shell, "%s%02d" % [name, k[0]], a, b, mat)
		for o in openings:
			seg.call(cursor, o[0], 0.0, h)
			seg.call(o[0], o[1], o[2], h)
			cursor = o[1]
		seg.call(cursor, end, 0.0, h)
	for p in _data.platforms:
		_block(shell, p[0], Vector3(p[1], p[5], p[3]), Vector3(p[2], p[6], p[4]), _mat(p[7]))
	for r in _data.roofs:
		_block(shell, r[0], Vector3(r[1], r[5], r[3]), Vector3(r[2], r[6], r[4]), _mat("deck"))
	_log.append("shell: %d blocks" % shell.get_child_count())


# --- structure ---------------------------------------------------------------

static func _in_machine(x: float, z: float, margin := 0.4) -> bool:
	for m in _data.machines:
		var yaw: float = m[3]
		var w: float = m[4]; var d: float = m[5]
		var ex := w / 2 + margin if absf(fmod(yaw, 180.0) - 90.0) < 45.0 else d / 2 + margin
		var ez := d / 2 + margin if absf(fmod(yaw, 180.0) - 90.0) < 45.0 else w / 2 + margin
		if absf(x - m[1]) < ex and absf(z - m[2]) < ez:
			return true
	for p in _data.platforms:
		if x > p[1] - margin and x < p[2] + margin and z > p[3] - margin and z < p[4] + margin:
			return true
	if _in_hole(x, z):
		return true
	# Stock, rooms, stairs, and rails: a column never stands inside them.
	for p in _data.props:
		var r: float = maxf(p.fit[0], p.fit[1]) / 2.0 + margin
		if absf(x - p.x) < r and absf(z - p.z) < r:
			return true
	for b in _data.get("boxes", []):
		if b.lo[1] < 2.0 and x > b.lo[0] - margin and x < b.hi[0] + margin and z > b.lo[2] - margin and z < b.hi[2] + margin:
			return true
	return false


## Inside a hole in the roof (the breach over the mid stockpile).
static func _in_hole(x: float, z: float) -> bool:
	for h in _data.get("roof_holes", []):
		if x >= h[0] and x <= h[1] and z >= h[2] and z <= h[3]:
			return true
	return false


static func _build_structure(g: Node3D) -> void:
	var deck: float = _data.deck_y
	var jb: float = _data.joist_bottom
	var girder_bottom := jb - 0.35
	var girder_depth := deck - girder_bottom
	var hall: Array = _data.hall
	var wing: Array = _data.wing
	var spans := [[hall, _data.girder_x, _data.column_z]]
	if not _data.wing_girder_x.is_empty():
		spans.append([wing, _data.wing_girder_x, _data.wing_column_z])
	var col_n := 0
	var girders := []
	var joists := []
	for span in spans:
		var area: Array = span[0]
		var gx: Array = span[1]
		var cz: Array = span[2]
		for x in gx:
			for z in cz:
				if _in_machine(x, z) or [x, z] in _data.get("skip_columns", []):
					continue
				col_n += 1
				_place(g, "Column%02d" % col_n, "steel_column_10", Vector3(x, 0, z), 0, [0.36, 0.36, girder_bottom], 0.05, 2.0)
			var zs := [area[2]] + cz + [area[3]]
			for i in zs.size() - 1:
				var length: float = zs[i + 1] - zs[i]
				var xf := Transform3D(Basis(Vector3.UP, -PI / 2) * Basis.from_scale(Vector3(length / 10.0, girder_depth / 1.4, 1)),
					Vector3(x, girder_bottom, (zs[i] + zs[i + 1]) / 2))
				if not _in_hole(x, (zs[i] + zs[i + 1]) / 2):
					girders.append(xf)
		var xs := [area[0]] + gx + [area[1]]
		for i in xs.size() - 1:
			var length: float = xs[i + 1] - xs[i]
			var z: float = area[2] + 0.9
			while z < area[3] - 0.5:
				if not _in_hole((xs[i] + xs[i + 1]) / 2, z):
					joists.append(Transform3D(Basis.from_scale(Vector3(length / 10.0, (deck - jb) / 1.0, 1)),
						Vector3((xs[i] + xs[i + 1]) / 2, jb, z)))
				z += _data.joist_spacing
	for c in _data.get("extra_columns", []):
		if _in_machine(c[0], c[1]):
			continue
		col_n += 1
		_place(g, "Column%02d" % col_n, "steel_column_10", Vector3(c[0], 0, c[1]), 0, [0.36, 0.36, girder_bottom], 0.05, 2.0)
	_multimesh(g, "Girders", "girder_10", girders)
	_multimesh(g, "Joists", "joist_10", joists)
	# Hanger rods and wires dropping from the deck (photos 3, 4, 9).
	var rods := []
	var rng := RandomNumberGenerator.new()
	rng.seed = 77
	for i in 420:
		var x := rng.randf_range(hall[0] + 0.5, hall[1] - 0.5)
		var z := rng.randf_range(hall[2] + 0.5, hall[3] - 0.5)
		var length := rng.randf_range(0.6, 1.2) if rng.randf() < 0.55 else rng.randf_range(1.5, deck - 3.5)
		if _in_hole(x, z):
			continue
		rods.append(Transform3D(Basis.from_scale(Vector3(1, length, 1)), Vector3(x, deck - length, z)))
	_multimesh(g, "HangerRods", "hanger_rod", rods)
	# Windows and doors.
	var n := 0
	for w in _data.windows:
		n += 1
		var face: String = w[6]
		var rot: float = {"S": 0.0, "N": 180.0, "W": -90.0, "E": 90.0}[face]
		var off: Vector3 = {"S": Vector3(0, 0, 0.08), "N": Vector3(0, 0, -0.08), "W": Vector3(-0.08, 0, 0), "E": Vector3(0.08, 0, 0)}[face]
		_place(g, "Window%02d" % n, w[0], Vector3(w[1], w[5], w[2]) + off, rot, [w[3], 0.12, w[4]], 0.2, 6.0, false)
	n = 0
	for d in _data.doors:
		n += 1
		var rot: float = {"S": 0.0, "N": 180.0, "W": -90.0, "E": 90.0}[d[3]]
		_place(g, "Door%02d" % n, d[0], Vector3(d[1], 0, d[2]), rot, [], 1.0, 1.0, true)
	_log.append("structure: %d columns, %d girders, %d joists, %d rods" % [col_n, girders.size(), joists.size(), rods.size()])


# --- lighting -----------------------------------------------------------------

static func _build_lighting(g: Node3D) -> void:
	var env := WorldEnvironment.new()
	env.name = "WarehouseEnvironment"
	env.environment = load(MATS + "warehouse_environment.tres")
	g.add_child(env)
	env.owner = _root
	var y: float = _data.light_y
	var fixtures := []
	var n := 0
	# The hall is lit by its lightmap: the high-bays and their uplights are
	# baked (direct light, shadows, and bounce) and removed in the game
	# (scripts/baked_lights.gd), as on the other maps. Drawn live they cost
	# about 14 ms a frame on Medium on the reference card, and SDFGI's bounce
	# another 5 (docs/performance.md). Players and props that are not
	# lightmapped take the light from the lightmap's probes.
	var baked := Node3D.new()
	baked.name = "BakedLights"
	baked.set_script(load(BAKED_LIGHTS))
	g.add_child(baked)
	baked.owner = _root
	# A LightmapGI bakes what is under its parent: it sits at the scene's root.
	var lightmap := _root.get_node_or_null("LightmapGI") as LightmapGI
	if lightmap == null:
		lightmap = LightmapGI.new()
		lightmap.name = "LightmapGI"
		_root.add_child(lightmap)
		lightmap.owner = _root
	lightmap.max_texture_size = 4096
	# Half the texel density the meshes ask for: the full hall at full
	# density runs the reference card out of memory while baking.
	lightmap.texel_scale = 0.7
	# The kraft paper and cardboard bounce a strong orange; SDFGI, which lit
	# the hall before, gave the bounce half its weight.
	lightmap.bounce_indirect_energy = 0.6
	# Finer probes, for the strewn clutter and players between the stacks.
	lightmap.generate_probes_subdiv = LightmapGI.GENERATE_PROBES_SUBDIV_16
	if lightmap.light_data == null and ResourceLoader.exists(LIGHTMAP):
		lightmap.light_data = load(LIGHTMAP)
	for p in _data.lights:
		n += 1
		fixtures.append(Transform3D(Basis(), Vector3(p[0], y, p[1])))
		var spot := SpotLight3D.new()
		spot.name = "HighBay%02d" % n
		spot.position = Vector3(p[0], y - 0.05, p[1])
		spot.rotation_degrees = Vector3(-90, 0, 0)
		spot.light_color = Color(1.0, 0.975, 0.93)
		spot.light_energy = 11.0
		spot.light_indirect_energy = 1.0
		spot.light_volumetric_fog_energy = 0.6
		spot.spot_range = 13.5
		spot.spot_angle = 62.0
		spot.spot_angle_attenuation = 1.2
		spot.spot_attenuation = 0.9
		spot.light_specular = 0.6
		spot.shadow_enabled = true
		spot.shadow_bias = 0.04
		spot.shadow_normal_bias = 1.5
		spot.distance_fade_enabled = true
		spot.distance_fade_begin = 60.0
		spot.distance_fade_shadow = 24.0
		spot.distance_fade_length = 12.0
		spot.light_bake_mode = Light3D.BAKE_STATIC
		# An LED high-bay's panel is about half a metre across: soft baked
		# shadows, as the live shadow maps' filtering gave.
		spot.light_size = 0.5
		baked.add_child(spot)
		spot.owner = _root
		# High-bays spill light upward, which is what lets the deck and joists
		# read: photo 10 shows a light grey deck washed around every fixture.
		var up := SpotLight3D.new()
		up.name = "Uplight%02d" % n
		up.position = Vector3(p[0], y + 0.15, p[1])
		up.rotation_degrees = Vector3(90, 0, 0)
		up.light_color = spot.light_color
		up.light_energy = 2.4
		up.spot_range = 5.0
		up.spot_angle = 85.0
		up.light_volumetric_fog_energy = 0.0
		up.light_bake_mode = Light3D.BAKE_STATIC
		baked.add_child(up)
		up.owner = _root
	# Each lamp sits in its fixture's housing: kept out of the bake, the
	# housing would shadow its own light off the whole hall.
	for mmi: MultiMeshInstance3D in _multimesh(g, "Fixtures", "high_bay_led", fixtures):
		mmi.gi_mode = GeometryInstance3D.GI_MODE_DISABLED
	g.add_child(_dust(Vector3(13.5, 4.2, 3.5), Vector3(61, 8, 59), 700))
	g.get_child(-1).owner = _root
	var hall: Array = _data.hall
	var probes := [[Vector3(4, 4.6, 3.5), Vector3(42, 9.4, 59)], [Vector3(31, 4.6, 3.5), Vector3(26, 9.4, 59)]]
	for i in probes.size():
		var rp := ReflectionProbe.new()
		rp.name = "Probe%d" % i
		rp.position = probes[i][0]
		rp.size = probes[i][1]
		rp.box_projection = true
		rp.interior = true
		rp.max_distance = 80.0
		rp.ambient_mode = ReflectionProbe.AMBIENT_DISABLED
		# About 1.5 ms on Medium on the reference card: High only.
		rp.add_to_group(&"graphics_high_only", true)
		g.add_child(rp)
		rp.owner = _root
	_log.append("lighting: %d high-bays" % n)


## Paper dust drifting through the hall, visible where the high-bays catch it.
static func _dust(center: Vector3, size: Vector3, amount: int) -> GPUParticles3D:
	var p := GPUParticles3D.new()
	p.name = "DustMotes"
	p.position = center
	p.amount = amount
	p.lifetime = 24.0
	p.preprocess = 24.0
	p.visibility_aabb = AABB(-size / 2, size)
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_BOX
	pm.emission_box_extents = size / 2
	pm.gravity = Vector3(0, -0.004, 0)
	pm.direction = Vector3(1, 0.2, 0.3)
	pm.spread = 180.0
	pm.initial_velocity_min = 0.01
	pm.initial_velocity_max = 0.05
	pm.turbulence_enabled = true
	pm.turbulence_noise_strength = 0.4
	pm.turbulence_noise_scale = 6.0
	pm.turbulence_influence_min = 0.02
	pm.turbulence_influence_max = 0.06
	pm.scale_min = 0.5
	pm.scale_max = 1.6
	var fade := Gradient.new()
	fade.set_color(0, Color(1, 1, 1, 0))
	fade.add_point(0.15, Color(1, 1, 1, 1))
	fade.add_point(0.85, Color(1, 1, 1, 1))
	fade.set_color(fade.get_point_count() - 1, Color(1, 1, 1, 0))
	var ramp := GradientTexture1D.new()
	ramp.gradient = fade
	pm.color_ramp = ramp
	p.process_material = pm
	var quad := QuadMesh.new()
	quad.size = Vector2(0.004, 0.004)
	var mat := StandardMaterial3D.new()
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	mat.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	mat.vertex_color_use_as_albedo = true
	mat.albedo_color = Color(1.0, 0.96, 0.88, 0.5)
	# No proximity fade: it makes the renderer copy the depth buffer every
	# frame, for motes four millimetres across.
	mat.distance_fade_mode = BaseMaterial3D.DISTANCE_FADE_PIXEL_ALPHA
	mat.distance_fade_min_distance = 9.0
	mat.distance_fade_max_distance = 0.5
	quad.material = mat
	p.draw_pass_1 = quad
	return p


# --- utilities --------------------------------------------------------------------

static func _line_xform(a: Vector3, b: Vector3, base_len: float, extra := Vector3.ONE) -> Transform3D:
	var d := b - a
	var len := d.length()
	var yaw := atan2(-d.z, d.x)
	var basis := Basis(Vector3.UP, yaw) * Basis.from_scale(Vector3(len / base_len, extra.y, extra.z))
	return Transform3D(basis, (a + b) / 2)


static func _build_utilities(g: Node3D) -> void:
	var trays := []
	for t in _data.trays:
		# Tile 6 m ladder modules along each run so the rung pitch stays 0.3 m.
		var a := Vector3(t[0], t[4], t[1])
		var b := Vector3(t[2], t[4], t[3])
		var count := maxi(1, ceili(a.distance_to(b) / 6.0))
		for k in count:
			trays.append(_line_xform(a.lerp(b, float(k) / count), a.lerp(b, float(k + 1) / count), 6.0))
	_multimesh(g, "Trays", "cable_tray_6", trays)
	var by_model := {}
	for p in _data.pipes:
		if p[1] == p[3] and p[2] == p[4]:
			continue
		by_model.get_or_add(p[0], []).append(_line_xform(Vector3(p[1], p[5], p[2]), Vector3(p[3], p[5], p[4]), 1.0))
	for d in _data.drops:
		var length: float = d[3] - d[2]
		by_model.get_or_add("pipe_conduit", []).append(
			Transform3D(Basis(Vector3.BACK, PI / 2) * Basis.from_scale(Vector3(length, 1, 1)), Vector3(d[0], (d[2] + d[3]) / 2, d[1])))
	for m in by_model:
		_multimesh(g, m.to_pascal_case(), m, by_model[m])
	var lines := []
	for l in _data.lines:
		lines.append(_line_xform(Vector3(l[0], 0.0, l[1]), Vector3(l[2], 0.0, l[3]), 4.0))
	_multimesh(g, "FloorLines", "floor_line", lines)
	_log.append("utilities: %d trays, %d lines" % [trays.size(), lines.size()])


# --- machines and stock -----------------------------------------------------------

static func _build_machines(g: Node3D) -> void:
	var n := 0
	for m in _data.machines:
		n += 1
		_place(g, "%s%02d" % [String(m[0]).to_pascal_case(), n], m[0], Vector3(m[1], m[7] if m.size() > 7 else 0.0, m[2]),
			90.0 - float(m[3]), [m[4], m[5], m[6]], 0.8, 1.25)
	_log.append("machines: %d" % n)


static func _build_stock(g: Node3D) -> void:
	var n := 0
	for p in _data.props:
		n += 1
		_place(g, "%s%03d" % [String(p.model).to_pascal_case(), n], p.model, Vector3(p.x, p.y, p.z), p.rot, p.fit,
			0.7, 1.4, p.collide)
	_log.append("stock: %d" % n)


## Plain floor slabs that fill gaps the Cyclops shell leaves.
static func _build_patches(g: Node3D) -> void:
	for p in _data.get("patches", []):
		var body := StaticBody3D.new()
		body.name = p[0]
		var size := Vector3(p[2] - p[1], p[6] - p[5], p[4] - p[3])
		body.position = Vector3((p[1] + p[2]) / 2.0, (p[5] + p[6]) / 2.0, (p[3] + p[4]) / 2.0)
		g.add_child(body)
		body.owner = _root
		var mesh := MeshInstance3D.new()
		mesh.name = "Mesh"
		var bm := BoxMesh.new()
		bm.size = size
		bm.material = _mat(p[7])
		# Lightmap UVs, so the bake lights it.
		bm.add_uv2 = true
		mesh.mesh = bm
		body.add_child(mesh)
		mesh.owner = _root
		var shape := CollisionShape3D.new()
		shape.name = "Shape"
		var bs := BoxShape3D.new()
		bs.size = size
		shape.shape = bs
		body.add_child(shape)
		shape.owner = _root
	_log.append("patches: %d" % _data.get("patches", []).size())


## Rooms, slabs, stairs, rails, and curtains from the layout's plain boxes. A
## box that collides is a StaticBody3D with its mesh; `visible: false` boxes
## are clip only (a stair's ramp, a rail's full-height clip) and sit beside the
## meshes they stand for; boxes that do not collide are meshes alone.
static func _build_boxes(g: Node3D) -> void:
	var n := 0
	for b in _data.get("boxes", []):
		n += 1
		var lo := Vector3(b.lo[0], b.lo[1], b.lo[2])
		var hi := Vector3(b.hi[0], b.hi[1], b.hi[2])
		var size := hi - lo
		var centre := (lo + hi) / 2.0
		var shown: bool = b.get("visible", true)
		var mesh: MeshInstance3D = null
		if shown:
			mesh = MeshInstance3D.new()
			var bm := BoxMesh.new()
			bm.size = size
			bm.material = _mat(b.mat)
			bm.add_uv2 = true
			mesh.mesh = bm
		if not b.collide:
			mesh.name = b.name
			mesh.position = centre
			g.add_child(mesh)
			mesh.owner = _root
			continue
		var body := StaticBody3D.new()
		body.name = b.name
		body.position = centre
		g.add_child(body)
		body.owner = _root
		if mesh != null:
			mesh.name = "Mesh"
			body.add_child(mesh)
			mesh.owner = _root
		var shape := CollisionShape3D.new()
		shape.name = "Shape"
		var bs := BoxShape3D.new()
		var pitch: float = b.get("pitch", 0.0)
		if is_zero_approx(pitch):
			bs.size = size
		else:
			# A stair's ramp: one slab from the foot to the head, 0.3 m thick,
			# its top along the step noses.
			var along_z: bool = is_zero_approx(b.get("yaw", 0.0))
			var run := size.z if along_z else size.x
			var rise := size.y - 0.1
			var length := sqrt(run * run + rise * rise)
			bs.size = Vector3(size.x, 0.3, length) if along_z else Vector3(length, 0.3, size.z)
			var axis := Vector3.RIGHT if along_z else Vector3.BACK
			var angle := atan2(rise, run) * signf(pitch)
			body.position = Vector3(centre.x, lo.y + 0.1 + rise / 2.0 - 0.15 / cos(atan2(rise, run)), centre.z)
			shape.rotation = (Basis(axis, angle)).get_euler()
		shape.shape = bs
		body.add_child(shape)
		shape.owner = _root
	# Climbable ladders (rust/game/src/ladder.rs): a LadderVolume at each
	# ladder's foot on its rungs' face, its -Z pointing the way a climber faces.
	for l in _data.get("ladders", []):
		var volume: Node3D = ClassDB.instantiate("LadderVolume")
		volume.name = l.name
		volume.position = _v(l.pos)
		volume.rotation_degrees.y = l.yaw
		volume.set("length", l.length)
		volume.set("width", l.width)
		g.add_child(volume)
		volume.owner = _root
	_log.append("boxes: %d, ladders: %d" % [n, _data.get("ladders", []).size()])


# --- detail pass ------------------------------------------------------------------

## The detail pass (tools/map_blockout/warehouse_detail_layout.py): set pieces
## placed one by one, with the one smooth clip shape each needs (a wreck's box,
## the rubble's low mound) or none at all, and strewn clutter drawn in batches
## by quarter of the map, casting no shadows and never colliding, so nothing
## catches a sprinting or sliding player.
static func _build_detail(g: Node3D) -> void:
	var n := 0
	for d in _data.get("detail", []):
		n += 1
		if d.has("beam"):
			_place_beam(g, d)
			continue
		var ps := _scene(d.model)
		if ps == null:
			continue
		var collides: bool = d.has("clip") or d.has("hull")
		var holder: Node3D = StaticBody3D.new() if collides else Node3D.new()
		holder.name = d.get("name", "%s%02d" % [String(d.model).to_pascal_case(), n])
		# Glass partitions, doors, and windows stop players, not rounds.
		if d.has("surface"):
			holder.set_meta("surface", d.surface)
		g.add_child(holder)
		holder.owner = _root
		var inst: Node3D = ps.instantiate()
		holder.add_child(inst)
		inst.owner = _root
		var r: Array = d.rot
		var s := Vector3.ONE
		if d.has("scale"):
			s = Vector3(d.scale[0], d.scale[1], d.scale[2])
		inst.transform = Transform3D(
			Basis.from_euler(Vector3(deg_to_rad(r[0]), 0.0, deg_to_rad(r[2])), EULER_ORDER_ZXY) * Basis.from_scale(s),
			Vector3.ZERO)
		# Centre what it now covers on the footprint, its lowest point on the
		# base (or, for wall pieces, its back on the wall).
		var box := _local_aabb(inst)
		box = inst.transform * box
		var c := box.get_center()
		inst.position = Vector3(-c.x, -box.position.y, -box.position.z if d.get("back", false) else -c.z)
		holder.position = _v(d.pos)
		holder.rotation_degrees.y = r[1]
		if not d.get("shadow", true):
			for gi: GeometryInstance3D in inst.find_children("*", "GeometryInstance3D", true, false):
				gi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var k := 0
		for c2 in d.get("clip", []):
			var shape := CollisionShape3D.new()
			shape.name = "Clip%02d" % k
			k += 1
			var bs := BoxShape3D.new()
			bs.size = Vector3(c2[3], c2[4], c2[5])
			shape.shape = bs
			shape.position = Vector3(c2[0], c2[1], c2[2])
			holder.add_child(shape)
			shape.owner = _root
		if d.has("hull"):
			var shape := CollisionShape3D.new()
			shape.name = "Mound"
			var hull := ConvexPolygonShape3D.new()
			var pts := PackedVector3Array()
			for q in d.hull:
				pts.append(_v(q))
			hull.points = pts
			shape.shape = hull
			holder.add_child(shape)
			shape.owner = _root
	var strewn := 0
	for model in _data.get("scatter", {}):
		var quarters := {}
		for t in _data.scatter[model]:
			var xf := Transform3D(
				Basis.from_euler(Vector3(deg_to_rad(t[4]), deg_to_rad(t[3]), deg_to_rad(t[5])), EULER_ORDER_YXZ)
					* Basis.from_scale(Vector3.ONE * t[6]),
				Vector3(t[0], t[1], t[2]))
			var q := ("N" if t[2] < 3.5 else "S") + ("W" if t[0] < 13.5 else "E")
			quarters.get_or_add(q, []).append(xf)
			strewn += 1
		for q in quarters:
			for mmi: MultiMeshInstance3D in _multimesh(g, "%s%s" % [String(model).to_pascal_case(), q], model, quarters[q]):
				mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_log.append("detail: %d pieces, %d strewn" % [n, strewn])


## A beam hung between two points (a joist torn loose at one end).
static func _place_beam(g: Node3D, d: Dictionary) -> void:
	var a := _v(d.beam[0])
	var b := _v(d.beam[1])
	var dir := b - a
	var x := dir.normalized()
	var z := x.cross(Vector3.UP).normalized()
	var y := z.cross(x)
	var basis := Basis(x * dir.length() / 10.0, y * float(d.get("depth", 1.0)), z)
	# The joist model hangs below its axis line from 0 to its depth: put its
	# top chord on the line between the two points.
	var mid := (a + b) / 2.0 - y * float(d.get("depth", 1.0))
	_multimesh(g, d.get("name", "Beam"), d.model, [Transform3D(basis, mid)])


const FX := "res://scripts/warehouse_fx.gd"


## The roof breach's daylight, sparks from a torn cable tray, flickering
## high-bays, forklift beacons, and steam from wall vents.
static func _build_effects(g: Node3D) -> void:
	var lighting := _root.get_node("PhotoPass/Lighting")
	var baked := lighting.get_node("BakedLights")
	var lights: Array = lighting.find_children("HighBay*", "SpotLight3D", true, false)
	var n := 0
	for e in _data.get("effects", []):
		n += 1
		match e.kind:
			"sun":
				var h: Array = e.hole
				var sun := DirectionalLight3D.new()
				sun.name = "BreachSun"
				# Low enough in the west that its shaft slants across the stockpile.
				sun.rotation_degrees = Vector3(-62.0, -125.0, 0.0)
				sun.light_color = Color(1.0, 0.93, 0.8)
				sun.light_energy = 2.2
				sun.light_volumetric_fog_energy = 3.0
				sun.shadow_enabled = true
				sun.directional_shadow_max_distance = 120.0
				# The shaft is baked into the lightmap; the live sun (its
				# shadows cost about 1.4 ms on Medium) lights players and the
				# haze on High only, where lightmapped surfaces skip it.
				sun.light_bake_mode = Light3D.BAKE_STATIC
				sun.add_to_group(&"graphics_high_only", true)
				g.add_child(sun)
				sun.owner = _root
				# Denser haze under the breach makes the shaft read.
				var fog := FogVolume.new()
				fog.name = "BreachHaze"
				fog.position = Vector3((h[0] + h[1]) / 2.0, 4.6, (h[2] + h[3]) / 2.0)
				fog.size = Vector3(h[1] - h[0] + 10.0, 9.2, h[3] - h[2] + 10.0)
				var fm := FogMaterial.new()
				fm.density = 0.012
				fm.albedo = Color(0.95, 0.92, 0.86)
				fm.edge_fade = 0.4
				fog.material = fm
				g.add_child(fog)
				fog.owner = _root
			"flicker":
				var best: SpotLight3D = null
				for l: SpotLight3D in lights:
					var d := Vector2(l.position.x - e.pos[0], l.position.z - e.pos[1]).length()
					if best == null or d < Vector2(best.position.x - e.pos[0], best.position.z - e.pos[1]).length():
						best = l
				if best != null:
					# A failing driver must stay live to flicker: out of the
					# bake-only lights, its direct light drawn live and only
					# its bounce baked.
					best.get_parent().remove_child(best)
					lighting.add_child(best)
					best.owner = _root
					best.light_bake_mode = Light3D.BAKE_DYNAMIC
					best.set_script(load(FX))
					best.set("kind", "flicker")
			"lamp":
				var lamp := OmniLight3D.new()
				lamp.name = "OfficeLamp%02d" % n
				lamp.position = Vector3(e.pos[0], e.pos[1], e.pos[2])
				var tint: Array = e.get("color", [0.95, 0.97, 1.0])
				lamp.light_color = Color(tint[0], tint[1], tint[2])
				lamp.light_energy = e.get("energy", 1.6)
				lamp.omni_range = e.get("range", 7.5)
				lamp.shadow_enabled = e.get("shadow", true)
				# Baked like the high-bays (scripts/baked_lights.gd).
				lamp.light_bake_mode = Light3D.BAKE_STATIC
				lamp.light_size = 0.2
				baked.add_child(lamp)
				lamp.owner = _root
			"sparks", "steam", "beacon", "smolder", "embers", "arc", "wisp", "drip", "dangle":
				var fx := Node3D.new()
				fx.name = "%s%02d" % [String(e.kind).capitalize(), n]
				fx.position = Vector3(e.pos[0], e.pos[1], e.pos[2])
				fx.set_script(load(FX))
				fx.set("kind", e.kind)
				match e.kind:
					"arc":
						fx.set("floor_y", 0.0)
					"dangle":
						fx.set("length", e.length)
					"drip":
						# Drips fall to the slab, or into the water of a bucket
						# standing under them.
						var catch := 0.0
						for d in _data.get("detail", []):
							if d.model == "mop_bucket" and Vector2(d.pos[0] - e.pos[0], d.pos[2] - e.pos[2]).length() < 0.4:
								catch = 0.3
						fx.set("length", e.pos[1] - catch)
						fx.set("fast", e.get("fast", false))
				g.add_child(fx)
				fx.owner = _root
	_log.append("effects: %d" % n)


# --- gameplay ----------------------------------------------------------------

const GAMEPLAY_NODES := ["Walkthrough", "StartMarkers", "Player", "HUD", "PauseMenu", "SpawnPoints",
	"ReverbZones", "Boundary", "BotLanes"]
const HUD_CONTROLS := ["move:Move", "sprint:Sprint", "crouch:Crouch / slide", "jump:Jump", "fire:Fire",
	"aim:Aim", "reload:Reload", "melee:Knife", "weapon_1:Switch weapon", "interact:Take weapon",
	"pause:Release mouse"]


## The player, HUD, pause menu, team spawns, reverb zones, and yard boundary
## the game needs to run Warehouse offline or on a dedicated server.
static func _build_gameplay() -> void:
	for n in GAMEPLAY_NODES:
		var old := _root.get_node_or_null(n)
		if old:
			_root.remove_child(old)
			old.free()
	_root.set_script(load("res://scripts/warehouse.gd"))
	# The editor's physics space does not answer queries, so floor and body
	# clearance for every point is checked by godot/tests/warehouse_test.gd.
	var spawns := Node3D.new()
	spawns.name = "SpawnPoints"
	_root.add_child(spawns)
	spawns.owner = _root
	var counts := [0, 0, 0]
	var first: Marker3D = null
	for s in _data.spawns:
		var pos := Vector3(s[0], s[1], s[2])
		var team: int = s[4]
		var marker := Marker3D.new()
		var slot: int = team if team >= 0 else 2
		marker.name = ("Team%d_%02d" % [team, counts[slot]]) if team >= 0 else ("Any_%02d" % counts[slot])
		counts[slot] += 1
		marker.position = pos
		marker.rotation.y = deg_to_rad(s[3])
		if team >= 0:
			marker.set_meta("team", team)
		spawns.add_child(marker)
		marker.owner = _root
		if first == null and team == 0:
			first = marker
	var player: Node3D = load("res://scenes/player.tscn").instantiate()
	player.name = "Player"
	_root.add_child(player)
	player.owner = _root
	player.position = first.position + Vector3.UP * 0.05
	player.rotation.y = first.rotation.y
	var hud: Node = load("res://scenes/ui/hud.tscn").instantiate()
	hud.name = "HUD"
	hud.set("controls", PackedStringArray(HUD_CONTROLS))
	_root.add_child(hud)
	hud.owner = _root
	var home := Button.new()
	home.name = "ReturnToMenu"
	home.offset_left = 40.0
	home.offset_top = 112.0
	home.offset_right = 236.0
	home.offset_bottom = 154.0
	home.text = "← Home"
	hud.add_child(home)
	home.owner = _root
	var pause: Node = load("res://scenes/ui/pause_menu.tscn").instantiate()
	pause.name = "PauseMenu"
	# The pause menu pushes Settings (input device, look speed, FOV) onto this
	# player: without it the right stick does nothing (#258).
	pause.set("player_path", NodePath("../Player"))
	_root.add_child(pause)
	pause.owner = _root
	var zones := Node3D.new()
	zones.name = "ReverbZones"
	_root.add_child(zones)
	zones.owner = _root
	for r in _data.reverb:
		var zone: Node3D = ClassDB.instantiate("ReverbZone")
		zone.name = r[0]
		zone.position = Vector3(r[1], r[2], r[3])
		zone.set("size", Vector3(r[4], r[5], r[6]))
		zone.set("room_size", r[7])
		zone.set("damping", r[8])
		zone.set("wet", r[9])
		zone.set("predelay_ms", r[10])
		zones.add_child(zone)
		zone.owner = _root
	var boundary := StaticBody3D.new()
	boundary.name = "Boundary"
	_root.add_child(boundary)
	boundary.owner = _root
	for b in _data.boundary:
		var shape := CollisionShape3D.new()
		var box := BoxShape3D.new()
		box.size = Vector3(b[1] - b[0], 15.0, b[3] - b[2])
		shape.shape = box
		shape.position = Vector3((b[0] + b[1]) / 2.0, 4.5, (b[2] + b[3]) / 2.0)
		boundary.add_child(shape)
		shape.owner = _root
	var lanes := Node3D.new()
	lanes.name = "BotLanes"
	_root.add_child(lanes)
	lanes.owner = _root
	for lane_name in _data.lanes:
		var lane := Node3D.new()
		lane.name = lane_name
		lanes.add_child(lane)
		lane.owner = _root
		var k := 0
		for p in _data.lanes[lane_name]:
			var point := Marker3D.new()
			point.name = "P%d" % k
			point.position = Vector3(p[0], p[1], p[2])
			lane.add_child(point)
			point.owner = _root
			k += 1
	_log.append("gameplay: %d team 0, %d team 1, %d neutral spawns" % counts)
