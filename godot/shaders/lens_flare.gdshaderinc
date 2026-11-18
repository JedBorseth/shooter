// The sun in the lens, shared by both lens shaders: glare around the sun, a
// few faint ghosts on the line from the sun through the middle of the screen,
// and the lens dirt lit up near the sun. scripts/lens.gd tests whether the sun
// is hidden (with rays against the world) and fades `flare` in and out, so a
// sun behind a building or off screen costs one branch per pixel.

// Where the sun is on screen, in UV (it may be a little off screen).
uniform vec2 sun_uv = vec2(0.5);
// 0 for no flare, 1 for a clear sun in view at full strength.
uniform float flare = 0.0;
uniform vec3 flare_tint : source_color = vec3(1.0, 0.9, 0.75);
// Lens dirt (scripts/lens_dirt.gd): clean glass is 0.6, dirt brighter.
uniform sampler2D dirt : hint_default_black, filter_linear_mipmap, repeat_disable;

float flare_ghost(vec2 uv, vec2 centre, float radius, float aspect) {
	float d = length((uv - centre) * vec2(aspect, 1.0)) / radius;
	// A soft disc with a slightly brighter rim, like an aperture's reflection.
	return (1.0 - smoothstep(0.75, 1.0, d)) * (0.6 + 0.4 * smoothstep(0.4, 0.95, d));
}

vec3 lens_flare(vec2 uv, float aspect) {
	vec2 to_sun = (uv - sun_uv) * vec2(aspect, 1.0);
	float d = length(to_sun);
	// Glare: a tight bright core and a faint veil around the sun, small
	// enough that the clouds beside it stay readable.
	float glare = exp(-d * 18.0) * 0.15 + exp(-d * 4.0) * 0.04;
	// Lens dirt catches the light within about a third of the screen.
	float smudge = max(texture(dirt, uv).r - 0.6, 0.0) * 2.5;
	glare += smudge * exp(-d * 3.5) * 0.3;
	// Ghosts mirror the sun through the middle of the screen, each tinted by
	// the lens coating that made it.
	vec2 axis = vec2(0.5) - sun_uv;
	vec3 ghosts = vec3(0.0);
	ghosts += flare_ghost(uv, sun_uv + axis * 0.55, 0.018, aspect) * vec3(0.55, 0.45, 0.25);
	ghosts += flare_ghost(uv, sun_uv + axis * 1.25, 0.045, aspect) * vec3(0.2, 0.42, 0.35);
	ghosts += flare_ghost(uv, sun_uv + axis * 1.6, 0.03, aspect) * vec3(0.4, 0.3, 0.55);
	ghosts += flare_ghost(uv, sun_uv + axis * 2.1, 0.09, aspect) * vec3(0.25, 0.35, 0.5) * 0.6;
	return flare * (flare_tint * glare + ghosts * 0.15);
}
