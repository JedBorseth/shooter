extends RefCounted
## Objective markers drawn over the world, as Black Ops 3 marks its flags,
## zones, and bomb sites, in our own art: a pointy-topped hexagon in the
## holder's colour with a light rim and its letter or glyph, a small pointer
## under it toward the spot, the action word above it on a dark plate with a
## notch, and the distance under it.
##
## `draw_world` places every marker of a mode together so they never pile up:
## off-screen markers (and ones that would sit on the HUD's corner clusters)
## hold to an inset rounded path with an arrow toward their spot and lose their
## words; markers closer than a hexagon's pitch slide apart; a farther
## marker whose words would cross a nearer one's drops them; and nearer
## markers draw on top. `badge` draws the hexagon alone (the minimap and the
## row of letters under it).
##
## Load it with `preload`; everything here is static.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")

## A world marker's hexagon, centre to corner, at 900 px tall.
const RADIUS := 15.0
## The least distance between two markers' centres.
const PITCH := 34.0
## The rounded path off-screen markers hold to: its centre's height as a share of
## the screen's, and its insets from the left and right edges and from the
## top, with its bottom kept above the HUD's bottom clusters (900 px units).
const HOLD_CENTRE_Y := 0.43
const HOLD_INSET_X := 72.0
const HOLD_BOTTOM := 670.0
## The hold path's squareness: 2 is an ellipse, higher hugs the edges.
const HOLD_POWER := 4.0
## The HUD's corner clusters, kept clear of held markers (1600 x 900 units):
## the minimap with its rows, the kill feed with the score block, and the
## scorestreak column with the ammo.
const KEEP_CLEAR: Array[Rect2] = [
	Rect2(0, 0, 400, 262),
	Rect2(0, 240, 250, 160),
	Rect2(0, 560, 390, 340),
	Rect2(1230, 520, 370, 380),
]
const DISTANCE_INK := Color("d8dce0")
## How much the minimap and the mode rows under it fall toward the screen's
## centre (`hud_shapes.gd` TILT).
const ROW_TILT := 0.045
## While dead: the death card from its KILLED BY tab down to the prompt under
## it, with room for a marker's pointer (1600 x 900 units, centred on the
## screen).
const DEATH_CARD := Rect2(-280, 78, 560, 292)
## The top centre kept clear while a banner, the killcam's title, or a medal
## shows there: its half width (1600 x 900 units, about the screen's middle).
const TOP_HALF_W := 290.0

## How far down the top centre is taken this frame (screen pixels; 0 when
## nothing shows there). Set by `draw_world` for the pushes it makes.
static var _top_taken := 0.0
## Boxes of the centre pieces up this frame that markers keep off too: the
## points popup and the READY plate (screen pixels).
static var _boxes: Array[Rect2] = []


## Draws a mode's markers. Each entry holds "world" (Vector3, the point the
## marker floats over) and the marker's look: "fill" (Color), "letter"
## (String) or "glyph" ("flag", "charge", or "zone"), and optionally
## "action" (String), "distance" (metres, -1 for none), "under" (String: a
## timer drawn under the hexagon in place of the distance), "progress" (0 to
## 1, a ring filling clockwise from the top), "progress_colour", and "alpha".
## `eye` is where distances are measured from (sorting nearest on top).
static func draw_world(
	item: CanvasItem, camera: Camera3D, marks: Array, screen: Vector2, eye: Vector3, dead := false
) -> void:
	var ui := screen.y / 900.0
	_top_taken = _top_band(item, ui)
	var placed: Array[Dictionary] = []
	for mark: Dictionary in marks:
		var at := place(camera, mark.world, screen, dead)
		var entry := mark.duplicate()
		entry.point = at.point
		entry.edge = at.edge
		entry.out = at.out
		entry.range = eye.distance_to(mark.world)
		placed.append(entry)
	placed.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return a.range < b.range)
	# Quiet near the crosshair and the points popup over it, so neither is
	# covered (Black Ops 3 fades its markers as the crosshair nears them).
	var middle := screen / 2.0
	var popup := Rect2(middle.x - 90.0 * ui, screen.y * 0.34 - 36.0 * ui, 180.0 * ui, 70.0 * ui)
	for entry: Dictionary in placed:
		var point: Vector2 = entry.point
		# From the marker's whole outline (its words reach 25 px above and
		# below the hexagon), not only its middle.
		var gap := (point - middle).abs() - Vector2(18.0, 40.0) * ui
		var near := smoothstep(20.0 * ui, 70.0 * ui, maxf(gap.x, gap.y))
		var fade := lerpf(0.35, 1.0, near)
		if popup.has_point(point):
			fade = minf(fade, 0.5)
		# Under a centre piece that is up (the points, the READY plate), an
		# on-screen marker stays where its spot is but steps right back.
		for box in _boxes:
			if box.grow(RADIUS * ui).has_point(point):
				fade = minf(fade, 0.25)
		entry.alpha = float(entry.get("alpha", 1.0)) * fade
		entry.quiet = fade < 0.9
	_spread(placed, PITCH * ui)
	for entry: Dictionary in placed:
		if entry.edge:
			# Spreading can slide a held marker back onto a cluster.
			entry.point = _keep_clear(entry.point, screen, dead, true)
	# Words: none on held markers; a farther marker drops its words when they
	# would cross a nearer marker or its words.
	var taken: Array[Rect2] = []
	for entry: Dictionary in placed:
		var hex := Rect2(entry.point - Vector2.ONE * RADIUS * ui, Vector2.ONE * RADIUS * 2.0 * ui)
		var words := _word_rects(entry, ui)
		# Faded near the crosshair, a marker keeps quiet too.
		var clear: bool = not entry.edge and not entry.quiet
		for rect in words:
			for other in taken:
				if rect.intersects(other):
					clear = false
		if not clear:
			entry.action = ""
			entry.distance = -1
			entry.under = ""
		taken.append(hex)
		if clear:
			taken.append_array(words)
	for i in range(placed.size() - 1, -1, -1):
		_draw_one(item, placed[i], ui)


## Where a world point's marker goes: {"point", "edge" (held on the path),
## "out" (the unit direction from the marker toward the spot when held)}.
static func place(camera: Camera3D, world: Vector3, screen: Vector2, dead := false) -> Dictionary:
	var ui := screen.y / 900.0
	var centre := Vector2(screen.x / 2.0, screen.y * HOLD_CENTRE_Y)
	var axes := Vector2(screen.x / 2.0 - HOLD_INSET_X * ui, HOLD_BOTTOM * ui - centre.y)
	var point := HudPaint.screen_point(camera, world)
	var behind := camera.is_position_behind(world)
	var offset := point - centre
	if behind:
		# Behind us: hold on the lower half of the path by the spot's
		# bearing (straight behind is the bottom middle), never on the top,
		# as Black Ops 3 holds a waypoint behind you at the screen's foot.
		var local := camera.global_transform.affine_inverse() * world
		var bearing := atan2(local.x, -local.z)
		offset = Vector2(sin(bearing), maxf(-cos(bearing), 0.0)) * axes
		if offset.length() < 1.0:
			offset = Vector2(0.0, axes.y)
	var reach := _reach(offset, axes)
	if not behind and reach <= 1.0:
		var pushed := _keep_clear(point, screen, dead)
		if pushed == point:
			return {"point": point, "edge": false, "out": Vector2.ZERO}
		return {"point": pushed, "edge": true, "out": (point - pushed).normalized()}
	var held := centre + offset / maxf(reach, 0.001)
	return {
		"point": _keep_clear(held, screen, dead, true), "edge": true, "out": offset.normalized()
	}


## How far out `offset` is on the hold path (1 on it): a squared-off ellipse
## (a superellipse), so held markers ride close to the screen's edges with
## rounded corners instead of floating in the picture on an ellipse's flanks.
static func _reach(offset: Vector2, axes: Vector2) -> float:
	var x := absf(offset.x / axes.x)
	var y := absf(offset.y / axes.y)
	return pow(pow(x, HOLD_POWER) + pow(y, HOLD_POWER), 1.0 / HOLD_POWER)


## A hexagon badge, pointy at the top and bottom: the fill (a little darker
## than the colour, sand kept bright), a light rim, its letter or glyph (dark
## on sand, white on the team colours), and the progress ring with a faint
## track. `radius` is centre to corner in pixels; `scale` sizes the rest.
static func badge(
	item: CanvasItem, at: Vector2, marker: Dictionary, radius: float, scale: float
) -> void:
	var alpha := float(marker.get("alpha", 1.0))
	var fill: Color = marker.get("fill", UiTheme.NEUTRAL)
	var neutral := fill.is_equal_approx(UiTheme.NEUTRAL)
	var shade := fill.darkened(0.04 if neutral else 0.12)
	hex(item, at + Vector2(0, 1.5 * scale), radius + 2.0 * scale, Color(0, 0, 0, 0.45 * alpha))
	hex(item, at, radius, Color(shade, 0.94 * alpha * fill.a))
	_outline(item, hex_points(at, radius - scale), Color(1, 1, 1, 0.8 * alpha), 2.0 * scale)
	var progress := float(marker.get("progress", 0.0))
	if progress >= 0.04:
		var colour: Color = marker.get("progress_colour", UiTheme.TEXT)
		var ring := radius + 4.0 * scale
		_outline(item, hex_points(at, ring), Color(1, 1, 1, 0.18 * alpha), 3.0 * scale)
		_ring(item, at, ring, progress, Color(colour, alpha), 3.0 * scale)
	var ink := Color(UiTheme.READY_INK if neutral else UiTheme.TEXT, alpha)
	var letter := String(marker.get("letter", ""))
	if letter != "":
		var size := int(radius * 1.05)
		HudPaint.text_centred(
			item,
			HudPaint.DISPLAY,
			letter,
			at + Vector2(0, size * 0.36),
			size,
			ink,
			0.0,
			not neutral
		)
	else:
		glyph(item, String(marker.get("glyph", "")), at, radius / RADIUS * scale, ink)


## A filled pointy-topped hexagon.
static func hex(item: CanvasItem, at: Vector2, radius: float, fill: Color) -> void:
	item.draw_colored_polygon(hex_points(at, radius), fill)


## The corners of a pointy-topped hexagon, clockwise from the top.
static func hex_points(at: Vector2, radius: float) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in 6:
		var angle := -PI / 2.0 + i * TAU / 6.0
		points.append(at + Vector2(cos(angle), sin(angle)) * radius)
	return points


## The white glyph inside a marker: a flag, the charge, or a zone's chevrons.
static func glyph(item: CanvasItem, kind: String, at: Vector2, scale: float, colour: Color) -> void:
	match kind:
		"flag":
			var top := at + Vector2(-4.5, -7.0) * scale
			item.draw_line(top, at + Vector2(-4.5, 7.5) * scale, colour, 1.8 * scale, true)
			item.draw_colored_polygon(
				PackedVector2Array(
					[top, at + Vector2(6.5, -3.5) * scale, at + Vector2(-4.5, 0.5) * scale]
				),
				colour
			)
		"charge":
			# A charge: a case with a dark timer window, a red lamp, and a
			# short antenna (not a box with a red dot, which reads as first aid).
			var case := Rect2(at - Vector2(7.5, 3.5) * scale, Vector2(15.0, 9.0) * scale)
			item.draw_rect(case, colour)
			var window := Rect2(at + Vector2(-5.5, -1.5) * scale, Vector2(7.0, 4.0) * scale)
			item.draw_rect(window, Color(0, 0, 0, 0.75 * colour.a))
			item.draw_circle(
				at + Vector2(4.2, 0.5) * scale, 1.4 * scale, Color(UiTheme.HARM, colour.a)
			)
			item.draw_line(
				at + Vector2(5.0, -3.5) * scale,
				at + Vector2(7.5, -8.0) * scale,
				colour,
				1.4 * scale,
				true
			)
		"zone":
			for i in 2:
				var y := (i * 5.5 - 4.0) * scale
				var chevron := PackedVector2Array(
					[
						at + Vector2(-6.0 * scale, y),
						at + Vector2(0, y + 4.5 * scale),
						at + Vector2(6.0 * scale, y),
					]
				)
				item.draw_polyline(chevron, colour, 2.2 * scale, true)


static func _draw_one(item: CanvasItem, entry: Dictionary, ui: float) -> void:
	var at: Vector2 = entry.point
	var alpha := float(entry.get("alpha", 1.0))
	var white := Color(UiTheme.TEXT, alpha)
	var r := RADIUS * ui
	badge(item, at, entry, r, ui)
	if entry.edge:
		# An arrow on the hexagon's side facing the spot, pointing straight
		# out (a diagonal one reads as a mouse pointer).
		var out: Vector2 = entry.out
		out = (
			Vector2(signf(out.x), 0.0) if absf(out.x) > absf(out.y) else Vector2(0.0, signf(out.y))
		)
		_arrow(item, at + out * (r + 9.0 * ui), 8.0 * ui, out, white)
	else:
		_arrow(item, at + Vector2(0, r + 6.0 * ui), 6.0 * ui, Vector2.DOWN, white)
	var action := String(entry.get("action", ""))
	if action != "":
		var plate := _action_rect(action, at, ui)
		item.draw_rect(plate, Color(0, 0, 0, 0.5 * alpha))
		# A small notch under the plate, toward the hexagon.
		var notch := Vector2(at.x, plate.end.y)
		(
			item
			. draw_colored_polygon(
				PackedVector2Array(
					[
						notch + Vector2(-3.5, 0) * ui,
						notch + Vector2(3.5, 0) * ui,
						notch + Vector2(0, 3.5) * ui,
					]
				),
				Color(0, 0, 0, 0.5 * alpha)
			)
		)
		HudPaint.text_centred(
			item,
			HudPaint.LABEL,
			action,
			Vector2(at.x, plate.end.y - 4.0 * ui),
			int(13 * ui),
			white,
			1.0,
			false
		)
	var under := String(entry.get("under", ""))
	var distance := int(entry.get("distance", -1))
	var baseline := at + Vector2(0, r + 25.0 * ui)
	if under != "":
		HudPaint.text_centred(
			item,
			HudPaint.DISPLAY,
			under,
			baseline + Vector2(0, 2.0 * ui),
			int(15 * ui),
			Color(UiTheme.HARM.lerp(Color.WHITE, 0.25), alpha)
		)
	elif distance >= 0:
		_shadowed(item, "%dm" % distance, baseline, int(13 * ui), Color(DISTANCE_INK, alpha), ui)


## Text with a 2 px dark shadow, so small words hold over a bright sky.
static func _shadowed(
	item: CanvasItem, value: String, at: Vector2, size: int, colour: Color, ui: float
) -> void:
	var shade := Color(0, 0, 0, 0.7 * colour.a)
	for offset: Vector2 in [Vector2(0, 2), Vector2(1, 1), Vector2(-1, 1)]:
		HudPaint.text_centred(
			item, HudPaint.LABEL, value, at + offset * ui, size, shade, 0.0, false
		)
	HudPaint.text_centred(item, HudPaint.LABEL, value, at, size, colour, 0.0, false)


static func _action_rect(action: String, at: Vector2, ui: float) -> Rect2:
	var width := HudPaint.text_width(HudPaint.LABEL, action, int(13 * ui), 1.0)
	return Rect2(
		at.x - width / 2.0 - 6.0 * ui, at.y - RADIUS * ui - 25.0 * ui, width + 12.0 * ui, 17.0 * ui
	)


## The boxes a marker's words take: the action plate and the distance.
static func _word_rects(entry: Dictionary, ui: float) -> Array[Rect2]:
	var rects: Array[Rect2] = []
	var at: Vector2 = entry.point
	var action := String(entry.get("action", ""))
	if action != "":
		rects.append(_action_rect(action, at, ui))
	if int(entry.get("distance", -1)) >= 0 or String(entry.get("under", "")) != "":
		rects.append(Rect2(at.x - 20.0 * ui, at.y + RADIUS * ui + 12.0 * ui, 40.0 * ui, 16.0 * ui))
	return rects


## Slides markers apart until no two centres are closer than `pitch`; the
## nearer marker (earlier) stays put and the farther one moves.
static func _spread(placed: Array[Dictionary], pitch: float) -> void:
	for _pass in 3:
		for i in placed.size():
			for j in i:
				var a: Vector2 = placed[j].point
				var b: Vector2 = placed[i].point
				var gap := b - a
				if gap.length() >= pitch:
					continue
				var away := gap.normalized() if gap.length() > 0.5 else Vector2.RIGHT
				if not placed[i].edge:
					# Markers in the open slide sideways, as words read better side
					# by side than stacked.
					away = Vector2(signf(gap.x) if absf(gap.x) > 0.5 else 1.0, 0.0)
				placed[i].point = a + away * pitch if placed[i].edge else _slide(a, b, away, pitch)


## Moves `b` sideways along `away` until it is `pitch` from `a`.
static func _slide(a: Vector2, b: Vector2, away: Vector2, pitch: float) -> Vector2:
	var dy := absf(b.y - a.y)
	var dx := sqrt(maxf(pitch * pitch - dy * dy, 0.0))
	return Vector2(a.x + away.x * dx, b.y)


## Moves a point off the HUD's corner clusters, to the nearest edge of the
## cluster it sits on that faces the screen's middle.
static func _keep_clear(point: Vector2, screen: Vector2, dead := false, held := false) -> Vector2:
	var rects := _clear_rects(screen, dead, held)
	var inside := rects.filter(func(r: Rect2) -> bool: return r.has_point(point))
	if inside.is_empty():
		return point
	# Every way out of the boxes it sits in, nearest first; the first that
	# lands outside every box wins (two boxes overlap at the minimap's foot).
	var ui := screen.y / 900.0
	var options: Array[Vector2] = []
	for rect: Rect2 in inside:
		if rect.position.x > 1.0:
			options.append(Vector2(rect.position.x, point.y))
		if rect.end.x < screen.x - 1.0:
			options.append(Vector2(rect.end.x, point.y))
		if rect.position.y > 1.0:
			options.append(Vector2(point.x, rect.position.y))
		if rect.end.y < minf(screen.y - 1.0, HOLD_BOTTOM * ui):
			# Never down into the bottom clusters and the controls legend.
			options.append(Vector2(point.x, rect.end.y))
	options.sort_custom(
		func(a: Vector2, b: Vector2) -> bool: return a.distance_to(point) < b.distance_to(point)
	)
	for option in options:
		var free := true
		for rect: Rect2 in rects:
			if rect.grow(-0.5).has_point(option):
				free = false
				break
		if free:
			return option
	return options[0] if not options.is_empty() else point


## The HUD's clusters on this screen, grown by a marker's half size.
static func _clear_rects(screen: Vector2, dead: bool, held: bool) -> Array[Rect2]:
	var ui := screen.y / 900.0
	var boxes := KEEP_CLEAR.duplicate()
	if dead:
		boxes.append(Rect2(DEATH_CARD.position + Vector2(800, 450), DEATH_CARD.size))
	if _top_taken > 0.0:
		boxes.append(Rect2(800.0 - TOP_HALF_W, 0.0, TOP_HALF_W * 2.0, _top_taken / ui))
	var rects: Array[Rect2] = []
	for box: Rect2 in boxes:
		var rect := Rect2(box.position * ui, box.size * ui)
		if box.get_center().x > 900.0:
			# Right-hand clusters hang off the right edge on any screen shape.
			rect.position.x = screen.x - (1600.0 - box.position.x) * ui
		elif box.get_center().x > 700.0:
			rect.position.x = screen.x / 2.0 + (box.position.x - 800.0) * ui
		rects.append(rect.grow(RADIUS * ui + 4.0 * ui))
	# Held markers keep off the centre pieces up too; on-screen ones fade
	# under them instead (`draw_world`).
	if held:
		for box in _boxes:
			rects.append(box.grow(RADIUS * ui + 4.0 * ui))
	return rects


## How far down the top centre is taken (screen pixels): the match banners'
## plates, the killcam's title, and the medals. Also gathers the centre
## pieces' boxes (`popup_box`).
static func _top_band(item: CanvasItem, ui: float) -> float:
	var tree := item.get_tree()
	if tree == null:
		return 0.0
	var bottom := 0.0
	_boxes.clear()
	for node in tree.get_nodes_in_group(&"top_centre"):
		if node.has_method("popup_box") and (node as CanvasItem).is_visible_in_tree():
			var box: Rect2 = node.call("popup_box")
			if box.has_area():
				_boxes.append(box)
		if node.has_method("top_taken") and (node as CanvasItem).is_visible_in_tree():
			bottom = maxf(bottom, float(node.call("top_taken")))
	var net := tree.root.get_node_or_null("Net")
	if net != null and net.has_method("client"):
		var client: Object = net.call("client")
		if client != null and client.has_method("in_killcam") and bool(client.call("in_killcam")):
			bottom = maxf(bottom, 130.0 * ui)
	return bottom


## A mode row's plate under the minimap: solid behind its words up to
## `text_end` (plus a margin), then a short fade, so the words hold over a
## bright wall.
static func row_plate(item: CanvasItem, rect: Rect2, text_end: float, ui: float) -> void:
	var colour := Color(0, 0, 0, 0.6)
	var solid := maxf(text_end + 8.0 * ui - rect.position.x, 0.0)
	item.draw_rect(Rect2(rect.position, Vector2(solid, rect.size.y)), colour)
	HudPaint.fade(
		item, Rect2(rect.position.x + solid, rect.position.y, 28.0 * ui, rect.size.y), colour
	)


## A small solid triangle pointing `direction`, centred on `at`.
static func _arrow(
	item: CanvasItem, at: Vector2, size: float, direction: Vector2, colour: Color
) -> void:
	var side := direction.orthogonal() * size * 0.55
	var tip := at + direction * size * 0.5
	var back := at - direction * size * 0.5
	var shade := Color(0, 0, 0, 0.5 * colour.a)
	var drop := Vector2(0, 1)
	item.draw_colored_polygon(
		PackedVector2Array([tip + drop, back + side + drop, back - side + drop]), shade
	)
	item.draw_colored_polygon(PackedVector2Array([tip, back + side, back - side]), colour)


static func _outline(
	item: CanvasItem, points: PackedVector2Array, colour: Color, width: float
) -> void:
	points.append(points[0])
	item.draw_polyline(points, colour, width, true)


## A ring round a pointy-topped hexagon filled `amount` of the way,
## clockwise from the top corner.
static func _ring(
	item: CanvasItem, at: Vector2, radius: float, amount: float, colour: Color, width: float
) -> void:
	var path := hex_points(at, radius)
	path.append(path[0])
	var total := 0.0
	for i in path.size() - 1:
		total += path[i].distance_to(path[i + 1])
	var left := total * clampf(amount, 0.0, 1.0)
	var drawn := PackedVector2Array([path[0]])
	for i in path.size() - 1:
		var step := path[i].distance_to(path[i + 1])
		if left >= step:
			drawn.append(path[i + 1])
			left -= step
		else:
			drawn.append(path[i].lerp(path[i + 1], left / step))
			break
	item.draw_polyline(drawn, colour, width, true)


## The slope the mode rows under the minimap share with it: they fall a
## little toward the screen's centre from the minimap's left edge. Use it
## with `draw_set_transform_matrix`, and reset after.
static func row_slope(ui: float) -> Transform2D:
	var pivot := 44.0 * ui
	return Transform2D(Vector2(1, ROW_TILT), Vector2(0, 1), Vector2(0, -ROW_TILT * pivot))


## Whether the mode rows under the minimap show: with the minimap, so a
## killcam (which hides it) hides them too.
static func rows_shown(item: CanvasItem) -> bool:
	var scene := item.get_tree().current_scene if item.is_inside_tree() else null
	var minimap := scene.get_node_or_null(^"TdmHud/Minimap") as CanvasItem if scene else null
	return minimap == null or minimap.is_visible_in_tree()
