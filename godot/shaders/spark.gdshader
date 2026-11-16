// A spark from a bullet impact: a thin glowing streak along the particle's
// velocity (the particle's Y axis, with align_y), turned to face the camera.
// HDR bright, so the glow pass blooms it.
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

uniform vec4 color : source_color = vec4(1.0, 0.75, 0.4, 1.0);
uniform float energy = 4.0;

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
	POSITION = PROJECTION_MATRIX * (VIEW_MATRIX * vec4(world, 1.0));
}

void fragment() {
	float across = 1.0 - abs(UV.x * 2.0 - 1.0);
	float along = 1.0 - UV.y;
	ALBEDO = color.rgb * energy * across * (0.4 + 0.6 * along) * COLOR.a;
}
