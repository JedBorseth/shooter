extends StaticBody3D
## A wall rack of long guns standing upright (assets/models/wall_rack, built by
## tools/map_dressing.py): each slot holds the same third-person weapon model
## players carry (its LODs included), butt on the rubber pad, muzzle up
## against the cradle, its side to the room. Set dressing: the guns cannot be
## taken, and the dedicated server skips them.

## Slot centres along the rack (metres), the height the butts stand on, and
## how far off the wall the guns stand.
const SLOTS: Array[float] = [-0.6, -0.2, 0.2, 0.6]
const SHELF := 0.305
const DEPTH := 0.15
const WEAPON_DIR := "res://scenes/weapons"

## The guns, by weapon scene key (scenes/weapons/<key>_view.tscn), one per
## slot from the left.
@export var weapons: PackedStringArray = ["m4", "ak47", "br3", "m40"]


func _ready() -> void:
	if DisplayServer.get_name() == "headless" and not Engine.has_meta(&"wall_rack_headless"):
		return
	fill()


## Stands a gun in each slot. Safe to call again: it clears the old ones.
func fill() -> void:
	for old in get_children():
		if old.has_meta(&"rack_gun"):
			old.free()
	for i in mini(weapons.size(), SLOTS.size()):
		var path := "%s/%s_view.tscn" % [WEAPON_DIR, weapons[i]]
		if not ResourceLoader.exists(path):
			push_warning("wall rack: no weapon scene %s" % path)
			continue
		var view: Node3D = (load(path) as PackedScene).instantiate()
		# A third-person model: normal materials, no sounds or shells.
		view.set(&"world_model", true)
		view.set_meta(&"rack_gun", true)
		add_child(view)
		_stand(view, SLOTS[i])


## Muzzle up (the view's barrel runs down -Z), its right side to the room
## (+Z), butt on the shelf, centred on the slot.
func _stand(view: Node3D, slot: float) -> void:
	var basis := Basis(Vector3(0, 0, 1), Vector3(-1, 0, 0), Vector3(0, -1, 0))
	view.transform = Transform3D(basis, Vector3.ZERO)
	var box := _bounds(view, view.transform)
	if box.size == Vector3.ZERO:
		return
	var centre := box.get_center()
	view.position = Vector3(slot - centre.x, SHELF - box.position.y, DEPTH - centre.z)
	_no_shadows(view)


## The bounds of every mesh under `node`, in this rack's space, given the
## transform `to_rack` from `node` to it.
func _bounds(node: Node, to_rack: Transform3D) -> AABB:
	var out := AABB()
	var first := true
	if node is VisualInstance3D and (node as VisualInstance3D).visible:
		var box := to_rack * (node as VisualInstance3D).get_aabb()
		out = box
		first = false
	for child in node.get_children():
		if child is Node3D:
			var box := _bounds(child, to_rack * (child as Node3D).transform)
			if box.size != Vector3.ZERO:
				out = box if first else out.merge(box)
				first = false
	return out


## A gun against a wall hides nearly all of its own shadow, but every shadow
## cascade would still draw its triangles.
func _no_shadows(node: Node) -> void:
	if node is GeometryInstance3D:
		(node as GeometryInstance3D).cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	for child in node.get_children():
		_no_shadows(child)
