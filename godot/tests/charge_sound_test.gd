extends SceneTree
## Headless test of where Search and Destroy's charge explosion is heard
## from (`snd_view.gd`): a charge going off inside a room (an indoor reverb
## zone, like site A in Breakwater's pump house) plays the indoor explosion,
## the blast through a real room; outdoors (site B on the quay), or in an
## outdoor reflection zone, the outdoor one. Zones pick as the rest of the
## sound does: the smallest zone around the point, and only an enclosed one.
## godot --headless --path godot --script res://tests/charge_sound_test.gd

const INDOOR := "res://assets/audio/world/charge_explosion_indoor.ogg"
const OUTDOOR := "res://assets/audio/world/charge_explosion.ogg"

var _failures: PackedStringArray = []
## `snd_view.gd`, loaded once the autoloads it uses are up.
var _snd_view: GDScript


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_snd_view = load("res://scripts/net/snd_view.gd") as GDScript
	_check(ResourceLoader.exists(INDOOR), "the indoor explosion exists")
	var world := Node3D.new()
	root.add_child(world)
	# A pump room (Breakwater's PumpE settings), a yard with light wall slap
	# around it, and a covered bay inside the yard flagged outdoor.
	_zone(world, Vector3(5.25, 2.4, -29.0), Vector3(6.8, 4.6, 13.3), 0.28, false)
	_zone(world, Vector3(0.0, 2.0, 0.0), Vector3(40.0, 8.0, 40.0), 0.06, false)
	_zone(world, Vector3(10.0, 2.0, 10.0), Vector3(4.0, 4.0, 4.0), 0.25, true)
	await process_frame

	_check(_plays(Vector3(5.25, 0.5, -25.0)) == INDOOR, "a charge in the pump room booms in it")
	_check(
		_plays(Vector3(19.0, 0.5, 29.0)) == OUTDOOR, "a charge on the open quay rolls off outdoors"
	)
	_check(
		_plays(Vector3(2.0, 0.5, 2.0)) == OUTDOOR,
		"a light outdoor reflection zone is still outdoors"
	)
	_check(_plays(Vector3(10.0, 0.5, 10.0)) == OUTDOOR, "a zone flagged outdoor is outdoors")

	# In a big hall the hall's room plays, but a smaller outdoor zone around
	# the point (a loading bay's open end) wins over it.
	var hall := Vector3(60.0, 2.0, 60.0)
	_zone(world, hall, Vector3(20.0, 6.0, 20.0), 0.26, false)
	_zone(world, hall + Vector3(6.0, 0.0, 0.0), Vector3(4.0, 4.0, 4.0), 0.25, true)
	await process_frame
	_check(_plays(hall + Vector3(-5.0, -1.5, 0.0)) == INDOOR, "a charge in a hall booms in it")
	_check(
		_plays(hall + Vector3(6.0, -1.5, 0.0)) == OUTDOOR,
		"the smallest zone around the charge decides"
	)

	world.queue_free()
	await process_frame
	print("charge sound test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)


func _zone(parent: Node3D, at: Vector3, size: Vector3, wet: float, outdoor: bool) -> void:
	var zone := ReverbZone.new()
	zone.position = at
	zone.size = size
	zone.wet = wet
	zone.outdoor = outdoor
	parent.add_child(zone)


func _plays(point: Vector3) -> String:
	return (_snd_view.call(&"explosion_at", self, point) as AudioStream).resource_path


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)
