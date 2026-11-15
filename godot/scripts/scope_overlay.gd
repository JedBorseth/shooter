extends Control
## Sniper scope: black surround with a round lens, fine crosshair, and a mil-dot
## stadia. Drawn once the player is nearly fully scoped (the weapon model hides
## at the same point).

const SHOW_AT := 0.85

var player: Node = null


func _process(_delta: float) -> void:
	visible = player != null and player.scope_amount() >= SHOW_AT
	if visible:
		queue_redraw()


func _draw() -> void:
	var centre := size / 2.0
	var radius := size.y * 0.46
	var black := Color(0, 0, 0, 1)
	# Surround: everything outside the lens.
	draw_rect(Rect2(0, 0, centre.x - radius, size.y), black)
	draw_rect(Rect2(centre.x + radius, 0, size.x - centre.x - radius, size.y), black)
	var ring := PackedVector2Array()
	var steps := 96
	for i in steps + 1:
		var a := TAU * i / steps
		ring.append(centre + Vector2(cos(a), sin(a)) * radius)
	# Fill the four corners between the lens and the side bars with thick arcs.
	draw_arc(centre, radius + size.y * 0.25, 0.0, TAU, steps, black, size.y * 0.5, true)
	draw_polyline(ring, Color(0, 0, 0, 0.9), 6.0, true)

	# Lens shading toward the rim.
	for i in 6:
		var r := radius * (0.86 + i * 0.025)
		draw_arc(centre, r, 0.0, TAU, steps, Color(0, 0, 0, 0.06 + i * 0.03), radius * 0.03, true)

	# Crosshair: thick outer posts, thin centre lines, and dots.
	var line := Color(0.02, 0.02, 0.02, 0.95)
	var gap := radius * 0.5
	draw_line(centre + Vector2(-radius, 0), centre + Vector2(-gap, 0), line, 5.0)
	draw_line(centre + Vector2(gap, 0), centre + Vector2(radius, 0), line, 5.0)
	draw_line(centre + Vector2(0, gap), centre + Vector2(0, radius), line, 5.0)
	draw_line(centre + Vector2(-gap, 0), centre + Vector2(gap, 0), line, 1.4)
	draw_line(centre + Vector2(0, -radius), centre + Vector2(0, gap), line, 1.4)
	for i in range(1, 5):
		var step := gap * i / 5.0
		for dot: Vector2 in [Vector2(step, 0), Vector2(-step, 0), Vector2(0, step), Vector2(0, -step)]:
			draw_circle(centre + dot, 2.2, line)

	if player.is_holding_breath() or player.breath() < 0.99:
		var label := "HOLDING BREATH" if player.is_holding_breath() else "[Shift] Hold breath"
		var font := get_theme_default_font()
		var width := 220.0
		var bar := Rect2(centre + Vector2(-width / 2.0, radius * 0.72), Vector2(width, 4))
		draw_rect(bar, Color(1, 1, 1, 0.25))
		draw_rect(Rect2(bar.position, Vector2(width * player.breath(), 4)), Color(1, 1, 1, 0.85))
		draw_string(font, bar.position + Vector2(0, -8), label, HORIZONTAL_ALIGNMENT_CENTER, width, 14, Color(1, 1, 1, 0.8))
