extends Label
## Netcode readout in the top-right corner: ping, loss, snapshots, prediction
## corrections, interpolation delay, and bandwidth. Hidden at first, since it
## covers the score; F3 toggles it.

var client: NetClient


func _ready() -> void:
	visible = false
	set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT, Control.PRESET_MODE_MINSIZE, 16)
	grow_horizontal = Control.GROW_DIRECTION_BEGIN
	horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	add_theme_font_size_override("font_size", 15)
	add_theme_color_override("font_outline_color", Color.BLACK)
	add_theme_constant_override("outline_size", 4)


func _unhandled_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key != null and key.pressed and not key.echo and key.physical_keycode == KEY_F3:
		visible = not visible


func _process(_delta: float) -> void:
	if client == null or not visible:
		return
	var s := client.stats()
	var lines := PackedStringArray()
	lines.append("%s   player %d" % [s.status, client.player_id()])
	lines.append("ping %.0f ms   loss %.1f%%" % [s.ping_ms, s.loss_percent])
	if s.has("corrections"):
		lines.append(
			(
				"pending inputs %d   corrections %d (last %.1f cm)"
				% [s.pending_inputs, s.corrections, s.last_error_cm]
			)
		)
	if s.has("interp_delay_ms"):
		lines.append("interp %.0f ms   jitter %.1f ms" % [s.interp_delay_ms, s.jitter_ms])
	lines.append(
		"in %.1f kbps   out %.1f kbps   players %d" % [s.kbps_in, s.kbps_out, s.remote_players + 1]
	)
	text = "\n".join(lines)
