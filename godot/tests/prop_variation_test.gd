extends SceneTree
## Headless test of Warehouse's prop variation (`PropVariation`,
## docs/map-design.md):
##
## - repeated stock comes out in several board, pallet, and strap variants;
## - some loads are opened, and a load without its cap has no straps left
##   floating over it;
## - nothing moves: every prop keeps its transform and its collision;
## - the same names give the same loads (every player sees the same map).
## godot --headless --path godot --script res://tests/prop_variation_test.gd

const LEVEL := "res://scenes/warehouse.tscn"
const LAYOUT := "res://tools/warehouse_photo_layout.json"
## As godot/tools/warehouse_photo_build.gd's PICKED_MIN_HEIGHT and
## KNOCKED_MIN_HEIGHT: a picked stack's low half stays above eye height, and
## the low stacks players mantle are never knocked.
const PICKED_MIN_HEIGHT := 2.4
const KNOCKED_MIN_HEIGHT := 1.5

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var first := _varied()
	var scene: Node3D = first[0]
	var changed: int = first[1]
	var stock := scene.get_node("PhotoPass/Stock")
	_check(changed > stock.get_child_count() / 2, "most props vary (%d)" % changed)

	var board := {}
	var straps := {}
	var opened := 0
	var floating := 0
	for holder in stock.get_children():
		var cap_gone := false
		var strap_shown := false
		for mi: MeshInstance3D in holder.find_children("*", "MeshInstance3D", true, false):
			var part := String(mi.name)
			for s in mi.mesh.get_surface_count():
				var over := mi.get_surface_override_material(s) as BaseMaterial3D
				var own := mi.mesh.surface_get_material(s) as BaseMaterial3D
				if own == null:
					continue
				var colour := over.albedo_color if over else own.albedo_color
				if own.resource_name.begins_with("Corrugated sheet edges"):
					board[colour.to_html()] = true
				elif own.resource_name == "Poly strap" and mi.visible:
					straps[colour.to_html()] = true
			if part.begins_with("Cap") and not mi.visible:
				cap_gone = true
			if part.begins_with("Poly") and mi.visible:
				strap_shown = true
			if not mi.visible:
				opened += 1
		if cap_gone and strap_shown:
			floating += 1
	_check(board.size() >= 5, "board comes in several shades (%d)" % board.size())
	_check(straps.size() >= 4, "straps come in several colours (%d)" % straps.size())
	_check(opened > 20, "some loads are opened (%d parts gone)" % opened)
	_check(floating == 0, "no strap floats over a load without its cap")

	# Nothing moves, and collision stays as built.
	var plain: Node3D = (load(LEVEL) as PackedScene).instantiate()
	var moved := 0
	var shapes := 0
	for holder: Node3D in stock.get_children():
		var other: Node3D = plain.get_node("PhotoPass/Stock").get_node(NodePath(holder.name))
		if not holder.global_transform.is_equal_approx(other.global_transform):
			moved += 1
		for c in holder.find_children("*", "Node3D", true, false):
			var twin := other.get_node(holder.get_path_to(c)) as Node3D
			if twin == null or not c.transform.is_equal_approx(twin.transform):
				moved += 1
		shapes += holder.find_children("*", "CollisionShape3D", true, false).size()
	var plain_shapes := 0
	for holder in plain.get_node("PhotoPass/Stock").get_children():
		plain_shapes += holder.find_children("*", "CollisionShape3D", true, false).size()
	_check(moved == 0, "no prop or part moves (%d did)" % moved)
	_check(shapes == plain_shapes, "collision is unchanged (%d shapes)" % shapes)
	plain.free()

	# The same names give the same loads.
	var second := _varied()
	var again: Node3D = second[0]
	var same := true
	var a := scene.get_node("PhotoPass/Stock").find_children("*", "MeshInstance3D", true, false)
	var b := again.get_node("PhotoPass/Stock").find_children("*", "MeshInstance3D", true, false)
	for i in a.size():
		var ma: MeshInstance3D = a[i]
		var mb: MeshInstance3D = b[i]
		if ma.visible != mb.visible:
			same = false
		for s in ma.mesh.get_surface_count():
			var oa := ma.get_surface_override_material(s) as BaseMaterial3D
			var ob := mb.get_surface_override_material(s) as BaseMaterial3D
			if (oa == null) != (ob == null) or (oa and oa.albedo_color != ob.albedo_color):
				same = false
	_check(same, "the same names give the same loads")
	_check_variants(stock)
	scene.free()
	again.free()

	if _failures == 0:
		print("prop_variation_test: all checks passed")
	quit(1 if _failures > 0 else 0)


## The variant models (picked, knocked, torn, crushed) are placed, keep their
## placement's name, and keep to their heights.
func _check_variants(stock: Node) -> void:
	var props: Array = JSON.parse_string(FileAccess.get_file_as_string(LAYOUT)).props
	var kinds := {}
	var too_low := []
	var renamed := 0
	for holder in stock.get_children():
		var index := int(String(holder.name).right(3)) - 1
		var placed: Dictionary = props[index]
		var model: Node = holder.get_child(0)
		var file := model.scene_file_path.get_file().get_basename()
		if not file.begins_with(placed.model):
			renamed += 1
		var kind := file.trim_prefix(placed.model).trim_prefix("_")
		if kind.is_empty():
			continue
		kinds[kind] = int(kinds.get(kind, 0)) + 1
		var height := float(placed.fit[2])
		if (
			(kind == "picked" and height < PICKED_MIN_HEIGHT)
			or (kind == "knocked" and height < KNOCKED_MIN_HEIGHT)
		):
			too_low.append(holder.name)
	_check(renamed == 0, "every placement holds its layout model or a variant of it")
	for kind in ["picked", "knocked", "torn", "crushed"]:
		_check(
			int(kinds.get(kind, 0)) >= 3, "%s variants are placed (%d)" % [kind, kinds.get(kind, 0)]
		)
	_check(too_low.is_empty(), "variants keep to their heights %s" % [too_low])


## The level, outside the tree (its own script stays out of it), varied.
func _varied() -> Array:
	var scene: Node3D = (load(LEVEL) as PackedScene).instantiate()
	var groups: Array[Node] = [
		scene.get_node("PhotoPass/Stock"), scene.get_node("PhotoPass/Machines")
	]
	return [scene, PropVariation.apply(groups)]


func _check(ok: bool, what: String) -> void:
	if ok:
		print("ok    ", what)
	else:
		_failures += 1
		push_error("FAIL  " + what)
