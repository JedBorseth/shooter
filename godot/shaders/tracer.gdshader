// Bullet tracer: a ribbon along the round's path, turned to face the camera,
// with a hot core, a soft edge, and a head that burns brighter than the tail.
// The mesh is a unit quad: its Y runs from the tail (-0.5) to the head (+0.5)
// and the node's Y axis is scaled to the streak's length, its X axis to the
// width. However far away it is, the ribbon stays at least `min_pixels`
// wide, dimmed to match, so a distant tracer reads as a thin steady line
// instead of flickering in and out between pixels.
// Additive and HDR: tracers are burning, so they bloom like a muzzle flash.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(1.0, 0.5, 0.2, 1.0);
uniform vec4 core_color : source_color = vec4(1.0, 0.68, 0.36, 1.0);
uniform float energy = 3.5;
uniform float min_pixels = 2.5;
/// How much of the streak, from the tail, fades in.
uniform float tail_fade = 0.85;

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
	float drawn = max(width, min_pixels * pixel);
	// Widened past its true width, it dims so it carries the same light.
	thin = width / drawn;
	VERTEX = (VIEW_MATRIX * vec4(point + side * VERTEX.x * drawn, 1.0)).xyz;
}

void fragment() {
	float across = abs(UV.x * 2.0 - 1.0);
	float core = smoothstep(0.5, 0.0, across);
	float glow = smoothstep(1.0, 0.0, across);
	// UV.y is 0 at the head and 1 at the tail.
	float along = 1.0 - smoothstep(1.0 - tail_fade, 1.0, UV.y);
	float head = smoothstep(0.0, 0.04, UV.y) * (1.0 + 0.6 * smoothstep(0.25, 0.0, UV.y));
	float light = along * head * mix(1.0, thin, 0.55);
	// Widened to a couple of pixels, its soft profile would fall between
	// pixel centres and leave only the dim edge: fill it solid instead.
	vec3 profile = color.rgb * glow * 0.6 + core_color.rgb * core;
	vec3 solid = mix(color.rgb, core_color.rgb, 0.35);
	ALBEDO = mix(profile, solid, 1.0 - thin) * energy * light;
}
