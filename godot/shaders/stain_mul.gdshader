shader_type spatial;
// Stains on Breakwater's ground (oil under the trucks, damp under the
// downpipes: tools/breakwater_layout.py `stain`): a thin fan laid over the
// ground that multiplies what is already drawn under it by its vertex
// colour, white at the rim and dark in the middle. It needs no light of its
// own, so it matches the lit, lightmapped ground exactly and stays out of the
// bake (a lit stain baked dark rims round its small lightmap chart).
render_mode blend_mul, unshaded, cull_back, depth_draw_never, shadows_disabled, fog_disabled;

void fragment() {
	ALBEDO = COLOR.rgb;
}
