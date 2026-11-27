// Softens a still for the loading screen (`scripts/ui/loading_screen.gd`):
// a wide 25-tap blur and a slight darken, as Black Ops 3 blurs the map art
// behind its loading screen.
shader_type canvas_item;

uniform float radius = 9.0;
uniform float dim = 0.72;

void fragment() {
	vec2 step_uv = TEXTURE_PIXEL_SIZE * radius;
	vec4 sum = vec4(0.0);
	float weight = 0.0;
	for (int x = -2; x <= 2; x++) {
		for (int y = -2; y <= 2; y++) {
			float w = 1.0 / (1.0 + float(x * x + y * y));
			sum += texture(TEXTURE, UV + vec2(float(x), float(y)) * step_uv) * w;
			weight += w;
		}
	}
	vec4 colour = sum / weight;
	COLOR = vec4(colour.rgb * dim, colour.a) * COLOR;
}
