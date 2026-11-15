extends Node2D
## Entry point for future game state, enemies, and spawning.


func _draw() -> void:
	var arena_size := get_viewport_rect().size
	for x in range(0, int(arena_size.x), 48):
		draw_line(Vector2(x, 0), Vector2(x, arena_size.y), Color(0.09, 0.13, 0.19))
	for y in range(0, int(arena_size.y), 48):
		draw_line(Vector2(0, y), Vector2(arena_size.x, y), Color(0.09, 0.13, 0.19))


func _ready() -> void:
	get_viewport().size_changed.connect(queue_redraw)
