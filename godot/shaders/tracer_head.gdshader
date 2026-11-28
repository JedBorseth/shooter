// The burning head of a tracer round, on High: a soft round glow at the front
// of the streak (`tracer.gdshader`), turned to face the camera, so the round
// itself reads as a hot point leading its trail, as Call of Duty's tracers do,
// rather than the streak simply ending. The mesh is a unit quad centred on the
// head. However far away the round is, the glow stays at least `min_pixels`
// across, dimmed to carry about the same light, so a distant round is a
// steady spark instead of a flickering pixel. Additive and HDR, so the bloom
// spreads it.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(1.0, 0.42, 0.1, 1.0);
uniform vec4 core_color : source_color = vec4(1.0, 0.78, 0.5, 1.0);
/// The glow's radius in metres, and the least it is drawn at, in pixels.
uniform float radius = 0.07;
uniform float min_pixels = 5.0;
uniform float energy = 5.0;

varying float dim;

void vertex() {
	vec4 centre = VIEW_MATRIX * vec4(MODEL_MATRIX[3].xyz, 1.0);
	float depth = max(-centre.z, 0.01);
	float pixel = depth * 2.0 / (PROJECTION_MATRIX[1][1] * VIEWPORT_SIZE.y);
	float r = max(radius, min_pixels * pixel);
	// Grown past its true size it dims, though not all the way: a round far
	// off still reads.
	dim = mix(1.0, radius / r, 0.6);
	// Pulled a little toward the camera, so it is not cut by the wall the
	// round strikes.
	VERTEX = centre.xyz + vec3(VERTEX.xy * 2.0 * r, 0.0) + normalize(-centre.xyz) * min(r, depth * 0.5);
}

void fragment() {
	float d = length(UV * 2.0 - 1.0);
	float glow = exp(-d * d * 5.0) * (1.0 - smoothstep(0.8, 1.0, d));
	float core = exp(-d * d * 36.0);
	ALBEDO = (color.rgb * glow + core_color.rgb * core * 2.5) * energy * dim;
}
