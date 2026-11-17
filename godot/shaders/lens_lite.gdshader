// The lens on the Medium preset: the vignette and the grain of lens.gdshader
// without reading the screen, at about a quarter of its cost. It draws a black
// vignette and light or dark specks over the picture with ordinary blending.
// No sharpening.
shader_type canvas_item;
render_mode unshaded;

uniform float vignette : hint_range(0.0, 1.0) = 0.13;
uniform float grain : hint_range(0.0, 0.2) = 0.014;
uniform float aim : hint_range(0.0, 1.0) = 0.0;
uniform float suppression : hint_range(0.0, 1.0) = 0.0;
uniform float tick = 0.0;

float hash(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

void fragment() {
	vec2 centred = (SCREEN_UV - 0.5) * vec2(1.0, 0.82);
	float edge = smoothstep(0.45, 0.95, length(centred) * 1.55);
	float shade = edge * clamp(vignette + aim * 0.16 + suppression * 0.45, 0.0, 0.9);
	// Grain a pixel and a half wide reads as film; single pixels read as sensor noise.
	float noise = hash(floor(FRAGCOORD.xy / 1.5) + vec2(tick * 37.0, tick * 91.0)) - 0.5;
	// A speck is white or black; how strongly it shows is the grain.
	float speck = abs(noise) * 2.0 * grain;
	float alpha = shade + speck * (1.0 - shade);
	vec3 colour = vec3(step(0.0, noise)) * speck * (1.0 - shade) / max(alpha, 0.0001);
	COLOR = vec4(colour, alpha);
}
