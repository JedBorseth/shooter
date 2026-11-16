extends Light3D
## A failing street lamp: a slow, faint wobble, and now and then a short burst of
## flickers. Visual only. Each lamp gets its own `seed`, so they never flicker in
## step.

## Picks this lamp's pattern.
@export var seed := 0
## Seconds between bursts, on average.
@export var burst_interval := 7.0
## How dark a flicker gets, as a fraction of the lamp's energy.
@export_range(0.0, 1.0) var depth := 0.75

var _base_energy := 1.0
var _time := 0.0
var _noise := FastNoiseLite.new()


func _ready() -> void:
	_base_energy = light_energy
	_noise.seed = seed
	_noise.frequency = 0.6
	_time = float(seed) * 1.37


func _process(delta: float) -> void:
	_time += delta
	var wobble := 1.0 + _noise.get_noise_1d(_time) * 0.08
	# A burst lasts about half a second, once per interval, at a phase set by the seed.
	var phase := fposmod(_time + float(seed) * 2.3, burst_interval)
	var flicker := 1.0
	if phase < 0.5:
		var fast := _noise.get_noise_1d(_time * 40.0 + 100.0)
		flicker = 1.0 - depth * clampf(fast * 2.0 + 0.4, 0.0, 1.0)
	light_energy = _base_energy * wobble * flicker
