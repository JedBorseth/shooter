extends AudioStreamPlayer
## The short ringing in the ears after a close blast: a high, slightly
## beating tone, generated here (no sound file), that swells in and fades.
## `ring(strength)` starts it; a closer blast (strength toward 1) rings
## louder and longer. A stronger ring replaces a weaker one in progress.

const RATE := 22050.0
const TONE := 3150.0
## A second tone a little off the first, so the ring beats like the real thing.
const BEAT := 3190.0
const SWELL := 0.06

var _strength := 0.0
var _length := 0.0
var _time := 0.0
var _phase := 0.0
var _phase_beat := 0.0
var _playback: AudioStreamGeneratorPlayback


func _ready() -> void:
	var generator := AudioStreamGenerator.new()
	generator.mix_rate = RATE
	generator.buffer_length = 0.15
	stream = generator
	bus = &"Effects"
	volume_db = -6.0
	set_process(false)


## Rings for 0.6 to 2 s, louder the closer the blast (`strength` 0 to 1).
func ring(strength: float) -> void:
	strength = clampf(strength, 0.0, 1.0)
	if playing and _strength * _fade() >= strength:
		return
	_strength = strength
	_length = 0.6 + 1.4 * strength
	_time = 0.0
	if not playing:
		play()
		_playback = get_stream_playback() as AudioStreamGeneratorPlayback
	set_process(true)


func _fade() -> float:
	if _time < SWELL:
		return _time / SWELL
	return clampf(1.0 - (_time - SWELL) / maxf(_length - SWELL, 0.01), 0.0, 1.0)


func _process(_delta: float) -> void:
	if _playback == null:
		return
	var frames := _playback.get_frames_available()
	var level := 0.12 * _strength
	for i in frames:
		var envelope := _fade()
		envelope *= envelope
		var sample := (sin(_phase) * 0.6 + sin(_phase_beat) * 0.4) * level * envelope
		_playback.push_frame(Vector2(sample, sample))
		_phase = fmod(_phase + TAU * TONE / RATE, TAU)
		_phase_beat = fmod(_phase_beat + TAU * BEAT / RATE, TAU)
		_time += 1.0 / RATE
	if _time >= _length:
		stop()
		set_process(false)
