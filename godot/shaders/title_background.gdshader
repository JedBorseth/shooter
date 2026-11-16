shader_type canvas_item;
render_mode unshaded;

void fragment() {
	vec2 position = UV * vec2(1.78, 1.0);
	vec3 base = mix(vec3(0.025, 0.045, 0.065), vec3(0.065, 0.115, 0.14), UV.x);
	float glow = max(0.0, 1.0 - distance(position, vec2(1.4, 0.48)) * 1.25);
	base += vec3(0.015, 0.065, 0.07) * glow;
	vec2 grid = abs(fract(position * 24.0 - 0.5) - 0.5) / fwidth(position * 24.0);
	float line = 1.0 - min(min(grid.x, grid.y), 1.0);
	base += vec3(0.06, 0.13, 0.15) * line * smoothstep(0.35, 1.0, UV.x) * 0.3;
	float slash = 1.0 - smoothstep(0.002, 0.004, abs(position.x - position.y * 0.35 - 1.1));
	base += vec3(0.15, 0.35, 0.34) * slash * 0.4;
	COLOR = vec4(base, 1.0);
}
