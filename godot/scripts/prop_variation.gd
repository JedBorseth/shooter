class_name PropVariation
## Makes repeated stock and yard pieces differ, as a real plant's loads do:
## board from different mills and batches, pallets new and old, straps of
## different colours, and some loads opened (the strap cut, the cap board
## gone, the label torn away, the stretch wrap stripped). Call of Duty dresses
## repeated stock the same way, with one model's variants and per-instance
## colour and wear, so a row never reads as copies (docs/map-design.md).
##
## Applied when the map loads, never saved into the scene: only materials and
## part visibility change, never a transform, so collision, the measured
## sightlines, and the baked lighting all stand. Each prop's choices come from
## a hash of its node name, so every player sees the same loads.

## Tints multiply the model's own colour. Board: as made, bleached, recycled
## grey-brown, dark, yellowed, dusty.
const BOARD_TINTS: Array[Color] = [
	Color(1, 1, 1),
	Color(1.24, 1.2, 1.1),
	Color(0.88, 0.86, 0.83),
	Color(0.74, 0.65, 0.55),
	Color(1.05, 0.98, 0.86),
	Color(1.04, 1.0, 0.96),
]
## A bleached white-top liner on some board.
const WHITE_TOP := Color(1.22, 1.2, 1.16)
const WHITE_TOP_ODDS := 0.15
## Pallets: pine as made, fresh, weathered grey, dark.
const PALLET_TINTS: Array[Color] = [
	Color(1, 1, 1), Color(1.12, 1.07, 0.96), Color(0.78, 0.79, 0.8), Color(0.6, 0.5, 0.42)
]
## Straps are untextured: a colour outright (as made, black, white, blue, yellow).
const STRAP_COLOURS: Array[Color] = [
	Color(0.2478, 0.2717, 0.2348),
	Color(0.04, 0.04, 0.045),
	Color(0.82, 0.82, 0.8),
	Color(0.1, 0.24, 0.58),
	Color(0.86, 0.64, 0.08),
]
## Paper rolls: as made, light, dark kraft, a grey recycled grade.
const ROLL_TINTS: Array[Color] = [
	Color(1, 1, 1), Color(1.12, 1.06, 0.96), Color(0.82, 0.74, 0.66), Color(0.92, 0.94, 0.96)
]
## Concrete barriers: as cast, darker, newer and paler, stained.
const CONCRETE_TINTS: Array[Color] = [
	Color(1, 1, 1), Color(0.88, 0.87, 0.85), Color(1.08, 1.06, 1.0), Color(0.95, 0.9, 0.82)
]
## Parts some loads lose, by the start of their mesh's name: the chance each
## is gone, and the salt for its roll. The straps run over the cap board, so
## a load without its cap loses its straps too (cut to lift the cap off).
const OPENED := {
	"Poly": [0.2, 7],
	"Cap": [0.22, 8],
	"Freight": [0.35, 9],
	"Stretch": [0.35, 10],
}


## Varies every model instanced under `groups` (a holder per prop, the model
## its child). Returns how many props changed.
static func apply(groups: Array[Node]) -> int:
	var cache := {}
	var n := 0
	for g in groups:
		for holder in g.get_children():
			for inst in holder.get_children():
				if inst is Node3D and inst.scene_file_path != "":
					if _vary(inst, String(holder.name).hash(), cache):
						n += 1
	return n


## A number in 0..1 from a prop's seed and a salt.
static func pick(seed: int, salt: int) -> float:
	return float(absi(("%d:%d" % [seed, salt]).hash()) % 10007) / 10007.0


static func _choose(list: Array[Color], seed: int, salt: int) -> Color:
	return list[int(pick(seed, salt) * list.size())]


static func _vary(inst: Node3D, seed: int, cache: Dictionary) -> bool:
	var concrete := inst.scene_file_path.ends_with("barrier.gltf")
	var board := _choose(BOARD_TINTS, seed, 1)
	var liner := WHITE_TOP if pick(seed, 2) < WHITE_TOP_ODDS else board
	var pallet := _choose(PALLET_TINTS, seed, 3)
	var strap := _choose(STRAP_COLOURS, seed, 4)
	var roll := _choose(ROLL_TINTS, seed, 5)
	var stone := _choose(CONCRETE_TINTS, seed, 6)
	var changed := false
	for mi: MeshInstance3D in inst.find_children("*", "MeshInstance3D", true, false):
		for s in mi.mesh.get_surface_count():
			var mat := mi.mesh.surface_get_material(s) as BaseMaterial3D
			if mat == null:
				continue
			var key := mat.resource_name
			var tint := Color(1, 1, 1)
			var colour := Color(0, 0, 0, 0)
			if concrete:
				tint = stone
			elif (
				key.begins_with("Corrugated sheet edges")
				or key == "Cap board"
				or key == "Corrugated bale"
			):
				tint = board
			elif key == "Kraft liner":
				tint = liner
			elif key == "Pallet pine" or key == "Weathered pallet":
				tint = pallet
			elif key == "Kraft roll" or key == "Kraft roll end":
				tint = roll
			elif key == "Poly strap":
				colour = strap
			if tint == Color(1, 1, 1) and colour.a == 0.0:
				continue
			var vkey := "%d|%s|%s" % [mat.get_instance_id(), tint, colour]
			var v: BaseMaterial3D = cache.get(vkey)
			if v == null:
				v = mat.duplicate()
				v.albedo_color = colour if colour.a > 0.0 else mat.albedo_color * tint
				cache[vkey] = v
			mi.set_surface_override_material(s, v)
			changed = true
		var part := String(mi.name)
		for prefix: String in OPENED:
			if part.begins_with(prefix) and _opened(seed, prefix):
				mi.visible = false
				changed = true
		if part.begins_with("Poly") and _opened(seed, "Cap"):
			mi.visible = false
	return changed


static func _opened(seed: int, prefix: String) -> bool:
	var rule: Array = OPENED[prefix]
	return pick(seed, rule[1]) < rule[0]
