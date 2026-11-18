extends RefCounted
## Lens dirt: faint smudges and specks on the front of the lens that light up
## only where something bright blooms through them (the sun, a lamp, a muzzle
## flash). Built once, in code, from a fixed seed, so there is no asset to
## ship. The glow pass multiplies its bloom by this map (`Environment.glow_map`)
## and the lens pass lights it around the sun (shaders/lens.gdshader), so it
## costs one texture read in passes that already run.

const WIDTH := 384
const HEIGHT := 216
## Brightness of clean glass: bloom keeps this much of its strength where there
## is no dirt.
const CLEAN := 0.6
## Sizes are in pixels of the map, which is a fifth of 1080p.
const SPECKS := 70
const SMUDGES := 26

static var _texture: ImageTexture


## The dirt map, built the first time it is asked for.
static func texture() -> ImageTexture:
	if _texture == null:
		_texture = ImageTexture.create_from_image(build())
	return _texture


## Builds the map: clean glass with soft round specks and a few long smudges.
static func build() -> Image:
	var value := PackedFloat32Array()
	value.resize(WIDTH * HEIGHT)
	value.fill(CLEAN)
	var rng := RandomNumberGenerator.new()
	rng.seed = 7341
	for i in SMUDGES:
		var centre := Vector2(rng.randf() * WIDTH, rng.randf() * HEIGHT)
		var radius := rng.randf_range(10.0, 34.0)
		var stretch := rng.randf_range(1.5, 3.5)
		_splat(value, centre, radius, stretch, rng.randf() * PI, rng.randf_range(0.08, 0.2))
	for i in SPECKS:
		var centre := Vector2(rng.randf() * WIDTH, rng.randf() * HEIGHT)
		# Mostly tiny specks, a few larger drops.
		var radius := lerpf(1.2, 7.0, pow(rng.randf(), 3.0))
		_splat(value, centre, radius, 1.0, 0.0, rng.randf_range(0.08, 0.2))
	var data := PackedByteArray()
	data.resize(WIDTH * HEIGHT)
	for i in value.size():
		data[i] = roundi(clampf(value[i], 0.0, 1.0) * 255.0)
	var image := Image.create_from_data(WIDTH, HEIGHT, false, Image.FORMAT_L8, data)
	image.convert(Image.FORMAT_RGB8)
	image.generate_mipmaps()
	return image


## Adds one soft ellipse: `radius` across its short axis, `stretch` times that
## along `angle`, brightening the glass by up to `amount` at its centre.
static func _splat(
	value: PackedFloat32Array,
	centre: Vector2,
	radius: float,
	stretch: float,
	angle: float,
	amount: float
) -> void:
	var reach := radius * stretch
	var along := Vector2(cos(angle), sin(angle))
	var x0 := maxi(int(centre.x - reach), 0)
	var x1 := mini(int(centre.x + reach) + 1, WIDTH)
	var y0 := maxi(int(centre.y - reach), 0)
	var y1 := mini(int(centre.y + reach) + 1, HEIGHT)
	for y in range(y0, y1):
		for x in range(x0, x1):
			var d := Vector2(x, y) - centre
			var a := d.dot(along) / stretch
			var b := d.dot(Vector2(-along.y, along.x))
			var r := sqrt(a * a + b * b) / radius
			if r < 1.0:
				var falloff := 1.0 - r * r
				value[y * WIDTH + x] += amount * falloff * falloff
