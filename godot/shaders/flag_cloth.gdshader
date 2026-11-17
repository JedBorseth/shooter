// A flag's cloth: a subdivided quad, pinned along its left edge (UV.x = 0) to
// the pole, rippling more toward the free end. Lit, with a little of its own
// glow so a team's colour reads at a distance and in the shade.
shader_type spatial;
render_mode cull_disabled;

uniform vec4 colour : source_color = vec4(0.48, 0.77, 1.0, 1.0);
uniform float glow = 0.35;

void vertex() {
	float free_end = UV.x;
	float wave = sin(TIME * 5.5 - UV.x * 7.0);
	VERTEX.z += wave * 0.11 * free_end;
	VERTEX.y += sin(TIME * 3.1 - UV.x * 4.0) * 0.025 * free_end;
	// Tilt the normal with the ripple so the folds catch the light.
	NORMAL = normalize(NORMAL + vec3(cos(TIME * 5.5 - UV.x * 7.0) * 0.6 * free_end, 0.0, 0.0));
}

void fragment() {
	float fold = 0.8 + 0.2 * sin(TIME * 5.5 - UV.x * 7.0 + 1.2);
	// A darker band along the top and bottom edges.
	float hem = smoothstep(0.0, 0.08, UV.y) * smoothstep(1.0, 0.92, UV.y);
	vec3 base = colour.rgb * mix(0.55, 1.0, hem);
	ALBEDO = base * fold;
	EMISSION = colour.rgb * glow * fold;
	ROUGHNESS = 0.85;
}
