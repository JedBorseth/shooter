// Shared by every material drawn on the first-person weapon, so the weapon,
// its muzzle flash, and anything attached to it line up exactly.

// Vertical FOV the weapon is drawn with, independent of the world camera.
uniform float viewmodel_fov : hint_range(20.0, 120.0) = 55.0;
// Fraction of the depth range the weapon is squeezed into, next to the near plane.
uniform float viewmodel_depth_scale : hint_range(0.001, 1.0) = 0.02;

vec4 viewmodel_position(mat4 projection_matrix, mat4 modelview_matrix, vec3 vertex) {
	// Rescale the focal terms, keeping their signs: the rendering backend may flip Y.
	float focal = 1.0 / tan(radians(viewmodel_fov) * 0.5);
	float scale = focal / abs(projection_matrix[1][1]);
	mat4 projection = projection_matrix;
	projection[0][0] *= scale;
	projection[1][1] *= scale;
	vec4 position = projection * (modelview_matrix * vec4(vertex, 1.0));
	// Reverse-Z (Godot 4.3+): clip depth equal to w is the near plane.
	position.z = mix(position.w, position.z, viewmodel_depth_scale);
	return position;
}
