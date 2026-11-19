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
const KEEP_SHELL := ["BayWestRamp", "BayEastRamp", "WingEntryRamp"]
const OLD_COVER := ["Jumbo Back", "Jumbo Crawl Section Roof", "Jumbo Die Cut Machine", "Sheeter Machine",
	"Rotary Machine", "WingOfficeWall4", "WingOfficeWall5", "DispatchOfficeL"]
const OLD_DRESSING := ["BoxMachine", "CardboardStock", "BlueRollingBin", "Forklift",
	"WarehouseFitout/ProductionStock", "WarehouseFitout/LeftWingStock", "WarehouseFitout/ProductionMachines",
	"WarehouseFitout/ShopSecondPass", "WarehouseFitout/Structure", "WarehouseFitout/Overhead",
	"WarehouseFitout/Lighting", "WarehouseFitout/HallDispatch/WorkshopDoorPanel",
	"WarehouseFitout/OfficeFurniture/ShippingCarton13", "WarehouseFitout/OfficeFurniture/ProductionDesk",
	"WarehouseFitout/OfficeFurniture/ProductionDeskReturn", "WarehouseFitout/OfficeFurniture/ProductionChair"]
## Older dressing kept but moved to where the photos put it (photo 1's shipping desk).
const LEGACY_MOVES := {}

static var _root: Node3D
static var _data: Dictionary
static var _scenes := {}
static var _clip := {}
static var _log: PackedStringArray


static func build(root: Node3D, parts: Array = ["shell", "structure", "lighting", "utilities", "machines", "stock", "patches", "clip", "gameplay"]) -> String:
	_root = root
	_log = PackedStringArray()
	_data = JSON.parse_string(FileAccess.get_file_as_string(LAYOUT))
	_clip = JSON.parse_string(FileAccess.get_file_as_string(CLIP)) if FileAccess.file_exists(CLIP) else {}
	_scenes = {}
	if "shell" in parts:
		_clear_old()
		_build_shell()
	var pass_node := _group(root, "PhotoPass")
	for part in ["structure", "lighting", "utilities", "machines", "stock", "patches"]:
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
	for body: StaticBody3D in _root.get_node("WarehouseDressing").find_children("*", "StaticBody3D", true, true):
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


static func _multimesh(parent: Node, name: String, model: String, xforms: Array) -> void:
	var ps := _scene(model)
	if ps == null or xforms.is_empty():
		return
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
		parent.add_child(mmi)
		mmi.owner = _root
	inst.free()


static func _v(a: Array) -> Vector3:
	return Vector3(a[0], a[1], a[2])


# --- shell -----------------------------------------------------------------

static func _clear_old() -> void:
	var shell := _root.get_node("Shell")
	for c in shell.get_children():
		if not (c.name in KEEP_SHELL):
			shell.remove_child(c)
			c.free()
	var cover := _root.get_node("BlockoutCover")
	for n in OLD_COVER:
		var c := cover.get_node_or_null(n)
		if c:
			cover.remove_child(c)
			c.free()
	var dressing := _root.get_node("WarehouseDressing")
	for n in OLD_DRESSING:
		var c := dressing.get_node_or_null(n)
		if c:
			c.get_parent().remove_child(c)
			c.free()
	for path in LEGACY_MOVES:
		var c := dressing.get_node_or_null(path) as Node3D
		if c:
			c.global_position = LEGACY_MOVES[path]


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
	if x > -3.3 and x < 9.0 and z > 24.1 and z < 33.0:  # dispatch office
		return true
	if x < -24.6 and z > 14.1 and z < 26.9:  # shipping room
		return true
	return false


static func _build_structure(g: Node3D) -> void:
	var deck: float = _data.deck_y
	var jb: float = _data.joist_bottom
	var girder_bottom := jb - 0.35
	var girder_depth := deck - girder_bottom
	var hall: Array = _data.hall
	var wing: Array = _data.wing
	var spans := [[hall, _data.girder_x, _data.column_z], [wing, _data.wing_girder_x, _data.wing_column_z]]
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
				girders.append(xf)
		var xs := [area[0]] + gx + [area[1]]
		for i in xs.size() - 1:
			var length: float = xs[i + 1] - xs[i]
			var z: float = area[2] + 0.9
			while z < area[3] - 0.5:
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
		g.add_child(spot)
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
		g.add_child(up)
		up.owner = _root
	_multimesh(g, "Fixtures", "high_bay_led", fixtures)
	g.add_child(_dust(Vector3(13.5, 4.2, 3.5), Vector3(61, 8, 59), 700))
	g.get_child(-1).owner = _root
	g.add_child(_dust(Vector3(-26.25, 4.2, 3.25), Vector3(18.5, 8, 55.5), 180))
	g.get_child(-1).owner = _root
	var hall: Array = _data.hall
	var probes := [[Vector3(4, 4.6, 3.5), Vector3(42, 9.4, 59)], [Vector3(31, 4.6, 3.5), Vector3(26, 9.4, 59)],
		[Vector3(-26.25, 4.6, 3.25), Vector3(18.5, 9.4, 55.5)]]
	for i in probes.size():
		var rp := ReflectionProbe.new()
		rp.name = "Probe%d" % i
		rp.position = probes[i][0]
		rp.size = probes[i][1]
		rp.box_projection = true
		rp.interior = true
		rp.max_distance = 80.0
		rp.ambient_mode = ReflectionProbe.AMBIENT_DISABLED
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
	mat.proximity_fade_enabled = true
	mat.proximity_fade_distance = 0.3
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
