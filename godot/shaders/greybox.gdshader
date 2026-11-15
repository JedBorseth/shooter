// World-space grid for blockout geometry: 1 m minor lines, 4 m major lines.
// Gives a sense of scale and speed without textures.
shader_type spatial;

uniform vec4 base_color : source_color = vec4(0.5, 0.5, 0.5, 1.0);
uniform vec4 line_color : source_color = vec4(0.0, 0.0, 0.0, 0.35);
uniform float cell_size = 1.0;
uniform float major_every = 4.0;
uniform float line_width = 0.012;
uniform float roughness : hint_range(0.0, 1.0) = 0.85;

varying vec3 world_position;
varying vec3 world_normal;

void vertex() {
	world_position = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	world_normal = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

float grid(vec2 p, float size, float width) {
	vec2 distance_to_line = abs(fract(p / size - 0.5) - 0.5) * size;
	vec2 aa = fwidth(p);
	vec2 line = 1.0 - smoothstep(vec2(width) - aa, vec2(width) + aa, distance_to_line);
	return max(line.x, line.y);
}

void fragment() {
	vec3 n = abs(world_normal);
	vec2 p = n.y > 0.5 ? world_position.xz : (n.x > n.z ? world_position.zy : world_position.xy);
	float minor = grid(p, cell_size, line_width);
	float major = grid(p, cell_size * major_every, line_width * 2.0);
	float lines = max(minor * 0.55, major);
	ALBEDO = mix(base_color.rgb, line_color.rgb, lines * line_color.a);
	ROUGHNESS = roughness;
}
