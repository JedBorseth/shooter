shader_type canvas_item;
render_mode unshaded;

uniform sampler2D earth_texture : source_color, repeat_enable;
uniform float longitude = 0.0;

void fragment() {
	vec2 p = (UV - 0.5) * 2.16;
	float radius = length(p);
	vec4 result = vec4(0.0);
	if (radius < 1.0) {
		vec3 normal = vec3(p.x, -p.y, sqrt(max(0.0, 1.0 - dot(p, p))));
		vec2 sphere_uv = vec2(atan(normal.x, normal.z) / TAU + longitude / TAU + 0.5,
			0.5 - asin(normal.y) / PI);
		vec3 surface = texture(earth_texture, sphere_uv).rgb;
		float lighting = 0.4 + 0.85 * max(dot(normal, normalize(vec3(-0.5, 0.35, 1.0))), 0.0);
		float rim = pow(1.0 - normal.z, 3.5);
		vec3 lava = vec3(1.0, 0.24, 0.035) * max(surface.r - surface.b, 0.0) * 0.5;
		result = vec4(surface * lighting * 1.8 + lava + vec3(0.2, 0.28, 0.32) * rim,
			1.0 - smoothstep(0.993, 1.0, radius));
	} else {
		float halo = exp(-(radius - 1.0) * 55.0) * 0.22;
		result = vec4(0.27, 0.39, 0.46, halo);
	}
	COLOR = result;
}
