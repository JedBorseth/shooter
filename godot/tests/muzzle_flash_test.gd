extends SceneTree
## Headless muzzle flash test. The firearms' flash draws from the rendered
## atlas (tools/build_muzzle_flash.py): every one of its 64 frames holds a
## flash (none rendered black), the side frames start at the muzzle (the
## cell's left edge) and fade toward their tip, each weapon draws its own gun
## class's rows, and each weapon's side plume is as long as its flash is big.
## godot --headless --path godot --script res://tests/muzzle_flash_test.gd

const ATLAS := "res://assets/textures/effects/muzzle_flash_8x8.png"
const GRID := 8
## Each weapon's flash class: 0 rifle, 1 SMG and pistol, 2 shotgun, 3 marksman.
const WEAPONS := {
	"m4": 0,
	"ak47": 0,
	"kr12": 0,
	"pdw9": 1,
	"cq10": 1,
	"m870": 2,
	"sg12": 2,
	"lsw6": 0,
	"br3": 0,
	"dm7": 3,
	"m40": 3,
	"pistol": 1,
	"bp9": 1,
}

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var image := (load(ATLAS) as Texture2D).get_image()
	if image.is_compressed():
		image.decompress()
	var cell := image.get_width() / GRID
	for row in GRID:
		for column in GRID:
			var peak := 0.0
			var lit := 0
			for y in range(0, cell, 4):
				for x in range(0, cell, 4):
					var v := image.get_pixel(column * cell + x, row * cell + y).r
					peak = maxf(peak, v)
					lit += 1 if v > 0.05 else 0
			_check(peak > 0.9 and lit > 20, "frame %d,%d holds a flash" % [row, column])
			if row % 2 == 1:
				var root := _column_heat(image, column * cell + cell / 16, row * cell, cell)
				var tip := _column_heat(image, column * cell + cell * 15 / 16, row * cell, cell)
				_check(root > tip * 3.0, "side frame %d,%d starts at the muzzle" % [row, column])
	for key: String in WEAPONS:
		var view := (
			(load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene).instantiate() as Node3D
		)
		root.add_child(view)
		await process_frame
		var flash := view.get_node("Muzzle/Flash") as MeshInstance3D
		var material := (flash.mesh as QuadMesh).material as ShaderMaterial
		var texture := material.get_shader_parameter("flipbook") as Texture2D
		_check(
			texture != null and texture.resource_path == ATLAS, "%s draws the rendered atlas" % key
		)
		_check(
			int(material.get_shader_parameter("flash_class")) == WEAPONS[key],
			"%s draws its gun class's flashes" % key
		)
		var size := (flash.mesh as QuadMesh).size.x
		var plumes := view.get_node("Muzzle").find_children("*", "MeshInstance3D", false, false)
		var longest := 0.0
		for plume: Node in plumes:
			if plume != flash and (plume as MeshInstance3D).mesh is QuadMesh:
				longest = maxf(longest, ((plume as MeshInstance3D).mesh as QuadMesh).size.x)
		_check(
			absf(longest - size * 1.6) < 0.001,
			"%s: the side plume is as long as the flash is big" % key
		)
		view.queue_free()
		await process_frame
	print("Muzzle flash test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _column_heat(image: Image, x: int, top: int, cell: int) -> float:
	var total := 0.0
	for y in range(top, top + cell, 2):
		total += image.get_pixel(x, y).r
	return total


func _check(ok: bool, message: String) -> void:
	if ok:
		print("ok   ", message)
	else:
		_failures += 1
		push_error("FAIL: " + message)
