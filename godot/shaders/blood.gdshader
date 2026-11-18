// Blood droplets: dark, wet drops stretched along the particle's velocity (its
// Y axis, with align_y) and turned to face the camera. Alpha blended and lit by
// the scene like any surface, so they darken what is behind them and never
// glow under bloom (the old drops were additive and HDR bright).
shader_type spatial;
render_mode blend_mix, cull_disabled, depth_draw_never, shadows_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(0.2, 0.012, 0.01, 1.0);
uniform float roughness = 0.3;

void vertex() {
	vec3 axis = MODEL_MATRIX[1].xyz;
	float len = max(length(axis), 0.0001);
	axis /= len;
	vec3 centre = MODEL_MATRIX[3].xyz;
	vec3 to_camera = normalize(CAMERA_POSITION_WORLD - centre);
	vec3 side = cross(axis, to_camera);
	side = length(side) > 0.001 ? normalize(side) : vec3(1.0, 0.0, 0.0);
	float width = length(MODEL_MATRIX[0].xyz);
	vec3 world = centre + axis * VERTEX.y * len + side * VERTEX.x * width;
	VERTEX = (VIEW_MATRIX * vec4(world, 1.0)).xyz;
	// Lit as a sheet facing the camera, tipped toward the drop's travel so
	// its top catches the light.
	NORMAL = normalize(mat3(VIEW_MATRIX) * normalize(to_camera + axis * 0.3));
}

void fragment() {
	// A drop: round and full at the head (top of the quad), thinning to a tail.
	vec2 p = vec2(UV.x * 2.0 - 1.0, UV.y * 2.0 - 1.0);
	float width = mix(1.0, 0.35, UV.y);
	float shape = 1.0 - smoothstep(0.55, 1.0, abs(p.x) / width);
	shape *= smoothstep(1.0, 0.7, abs(p.y));
	ALBEDO = color.rgb;
	ROUGHNESS = roughness;
	SPECULAR = 0.5;
	ALPHA = color.a * COLOR.a * shape;
}
