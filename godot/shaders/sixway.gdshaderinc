// Six-way lightmaps for the Blender-rendered gas flipbooks
// (`tools/build_flipbooks.py`): each cell holds the same puff of gas six
// times, lit by one sun from one side only, with the light scattered through
// the gas and its self-shadowing baked in, packed as
//   a: R lit from the right, G from the left, B from above, A coverage
//   b: R lit from below, G from the front, B from behind, A glow (sqrt)
// The lit channels are stored sRGB-encoded (sample them as colour textures)
// and unpremultiplied. `sixway_light` blends them for a light arriving from
// `l` (unit, toward the light) in the flipbook's own frame: x to the cell's
// right, y up the cell, z out of the cell toward the camera. Each axis takes
// the channel facing the light, weighted by the square of its share, so the
// six add up to the right answer for a light from any side.

float sixway_light(vec4 a, vec4 b, vec3 l) {
	vec3 w = l * l;
	float across = l.x > 0.0 ? a.r : a.g;
	float upright = l.y > 0.0 ? a.b : b.r;
	float depth = l.z > 0.0 ? b.g : b.b;
	return across * w.x + upright * w.y + depth * w.z;
}

// The gas's response to light from all round (a sky): the mean of the six.
float sixway_ambient(vec4 a, vec4 b) {
	return (a.r + a.g + a.b + b.r + b.g + b.b) / 6.0;
}

// The glow channel back to linear (0 to 1).
float sixway_glow(vec4 b) {
	return b.a * b.a;
}
