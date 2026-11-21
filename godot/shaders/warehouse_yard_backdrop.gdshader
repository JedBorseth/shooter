// Outside the yard clip. One inexpensive surface keeps the visible ground
// connected to the distant horizon on every preset, including bake-only Low.
shader_type spatial;
render_mode unshaded, cull_disabled;

uniform sampler2D dry_grass : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D gravel : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D asphalt : source_color, filter_linear_mipmap, repeat_enable;
varying vec2 ground;

void vertex() {
	ground = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xz;
}

void fragment() {
	vec3 dry = texture(dry_grass, ground / 3.3).rgb * vec3(0.74, 0.72, 0.62);
	vec3 stone = texture(gravel, ground / 4.5).rgb * vec3(0.64, 0.58, 0.47);
	vec3 yard = texture(asphalt, ground / 5.2).rgb * vec3(0.76, 0.71, 0.62);
	float drift = 0.5 + 0.26 * sin(ground.x * 0.24 + ground.y * 0.08)
		+ 0.20 * sin(ground.y * 0.29 - ground.x * 0.12);
	float freight = smoothstep(12.0, 42.0, ground.y + (drift - 0.5) * 22.0);
	vec3 tone = mix(mix(dry, stone, drift * 0.69), yard, freight * 0.87);
	float distance_to_yard = length(max(vec2(0.0), abs(ground - vec2(4.0, 2.0)) - vec2(41.0, 40.0)));
	ALBEDO = mix(tone, mix(vec3(0.18, 0.18, 0.141), vec3(0.21, 0.205, 0.173), freight),
		smoothstep(29.0, 89.0, distance_to_yard) * 0.68);
}
