// An amber warning beacon: an emissive lens that pulses on its own, offset by
// where it stands so a row of them does not flash in step. No light is cast;
// the glow pass does the rest.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_schlick_ggx;

uniform vec4 color : source_color = vec4(1.0, 0.55, 0.1, 1.0);
uniform float energy = 9.0;
uniform float rate = 1.6;

varying float phase;

void vertex() {
	vec3 origin = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	phase = fract(sin(dot(floor(origin.xz * 0.5), vec2(12.9898, 78.233))) * 43758.5453);
}

void fragment() {
	float t = fract(TIME * rate + phase);
	float pulse = smoothstep(0.0, 0.08, t) * (1.0 - smoothstep(0.25, 0.45, t));
	ALBEDO = color.rgb * 0.3;
	ROUGHNESS = 0.3;
	EMISSION = color.rgb * energy * (0.08 + 0.92 * pulse);
}
