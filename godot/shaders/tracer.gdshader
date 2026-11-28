// Bullet tracer: a ribbon along the round's path, turned to face the camera:
// a white-hot core inside a wider orange glow, with a head that burns brighter
// than the tail, as Call of Duty draws them (hot enough to read against a
// sunlit sky and bloom). The mesh is a unit quad: its Y runs from the tail
// (-0.5) to the head (+0.5) and the node's Y axis is scaled to the streak's
// length, its X axis to the core's width. However far away it is, the core
// stays at least `min_pixels` wide, dimmed to match, so a distant tracer
// reads as a thin steady line instead of flickering in and out between
// pixels; the glow is drawn `glow_width` times as wide round it.
// Additive and HDR: tracers are burning, so they bloom like a muzzle flash.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(1.0, 0.42, 0.1, 1.0);
uniform vec4 core_color : source_color = vec4(1.0, 0.86, 0.62, 1.0);
/// The core's brightness, and the glow's at its middle.
uniform float energy = 14.0;
uniform float glow_energy = 2.4;
/// The glow's width, in core widths.
uniform float glow_width = 4.0;
uniform float min_pixels = 3.0;
/// How much of the streak, from the tail, fades in.
uniform float tail_fade = 0.9;

varying float thin;

void vertex() {
	vec3 axis = MODEL_MATRIX[1].xyz;
	float len = max(length(axis), 0.0001);
	axis /= len;
	float width = length(MODEL_MATRIX[0].xyz);
	vec3 point = MODEL_MATRIX[3].xyz + axis * VERTEX.y * len;
	vec3 to_camera = normalize(CAMERA_POSITION_WORLD - point);
	vec3 side = cross(axis, to_camera);
	side = length(side) > 0.001 ? normalize(side) : normalize(cross(axis, vec3(0.0, 1.0, 0.1)));
	// One pixel's width in metres at this end's depth.
	float depth = max(-(VIEW_MATRIX * vec4(point, 1.0)).z, 0.01);
	float pixel = depth * 2.0 / (PROJECTION_MATRIX[1][1] * VIEWPORT_SIZE.y);
	float core = max(width, min_pixels * pixel);
	// Widened past its true width, it dims so it carries the same light.
	thin = width / core;
	VERTEX = (VIEW_MATRIX * vec4(point + side * VERTEX.x * core * glow_width, 1.0)).xyz;
}

void fragment() {
	// Across the ribbon in core half-widths: 0 in the middle, `glow_width`
	// at the edge.
	float across = abs(UV.x * 2.0 - 1.0) * glow_width;
	// Widened to a couple of pixels, a soft core would fall between pixel
	// centres: it stays solid out to its edge.
	float core = 1.0 - smoothstep(mix(0.2, 0.6, 1.0 - thin), 1.0, across);
	float glow = exp(-across * across * 0.22);
	// UV.y is 0 at the head and 1 at the tail.
	float along = 1.0 - smoothstep(1.0 - tail_fade, 1.0, UV.y);
	float head = smoothstep(0.0, 0.03, UV.y) * (1.0 + 0.8 * smoothstep(0.2, 0.0, UV.y));
	float light = along * head * mix(1.0, thin, 0.5);
	ALBEDO = (core_color.rgb * core * energy + color.rgb * glow * glow_energy) * light;
}
