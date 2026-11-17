shader_type canvas_item;
// Behind full-screen menus: a soft blur of whatever is on screen, darkened toward ink.

uniform sampler2D screen_texture : hint_screen_texture, filter_linear_mipmap;
uniform float lod = 2.5;
uniform vec4 tint : source_color = vec4(0.043, 0.051, 0.063, 0.7);

void fragment() {
	vec2 px = SCREEN_PIXEL_SIZE * 6.0;
	vec3 sum = textureLod(screen_texture, SCREEN_UV, lod).rgb * 4.0;
	sum += textureLod(screen_texture, SCREEN_UV + vec2(px.x, 0.0), lod).rgb;
	sum += textureLod(screen_texture, SCREEN_UV - vec2(px.x, 0.0), lod).rgb;
	sum += textureLod(screen_texture, SCREEN_UV + vec2(0.0, px.y), lod).rgb;
	sum += textureLod(screen_texture, SCREEN_UV - vec2(0.0, px.y), lod).rgb;
	COLOR = vec4(mix(sum / 8.0, tint.rgb, tint.a), 1.0);
}
