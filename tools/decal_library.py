"""Source, footprint and tags for each decal in the texture library."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Decal:
    asset: str | None
    size: tuple[float, float]
    tags: tuple[str, ...]
    grey: bool = False
    orm: bool = False
    # Atlas crop (left, top, right, bottom), in thousandths of the source image.
    crop: tuple[int, int, int, int] | None = None


# Footprints are source measurements where available and placement estimates elsewhere.
LEGACY_DECALS = {
    "crosswalk": Decal("RoadLines020C", (4, 4), ("floor", "road", "marking"), grey=True),
    "lane_dashes": Decal("RoadLines028C", (4, 4), ("floor", "road", "marking"), grey=True),
    "edge_lines": Decal("RoadLines023C", (4, 4), ("floor", "road", "marking"), grey=True),
    "asphalt_damage": Decal("AsphaltDamage001", (1.2, 1.2), ("floor", "road", "damage", "crack")),
    "manhole": Decal("ManholeCover010", (1, 1), ("floor", "road", "manhole")),
    "drips": Decal("Leaking016B", (1, 1), ("wall", "grime", "water")),
    "parking_lines": Decal("RoadLines019B", (4, 4), ("floor", "road", "marking"), grey=True),
    "bullet_hole": Decal(None, (0.18, 0.18), ("wall", "floor", "damage", "impact")),
    "puddle": Decal(None, (2.4, 1.6), ("floor", "road", "water"), orm=True),
    "oil_stain": Decal(None, (2, 1.6), ("floor", "road", "grime", "oil"), orm=True),
}


def _source(asset, size, tags, *, grey=False, crop=None):
    return Decal(asset, size, tuple(tags.split()), grey=grey, orm=True, crop=crop)


# Each crop isolates one piece of the provider's atlas, using the same box on every map.
SURFACE_DECALS = {
    "utility_hatch": _source("ManholeCover011", (0.9, 0.9), "floor road manhole metal rust"),
    "rusted_drain": _source("ManholeCover009", (0.9, 0.9), "floor road drain metal rust"),
    "twin_utility_hatch": _source("ManholeCover007", (1.4, 0.7), "floor road manhole concrete"),
    "cracked_manhole": _source("ManholeCover008", (0.9, 0.9), "floor road manhole concrete damage crack"),
    "perforated_drain": _source("ManholeCover005", (0.8, 0.8), "floor road drain metal"),
    "recessed_manhole": _source("ManholeCover006", (0.8, 0.8), "floor road manhole metal"),
    "stone_paving_edge": _source("PavingEdge002", (2.6, 0.081), "floor road paving stone"),
    "brick_gutter": _source("PavingEdge001", (0.5, 2), "floor road paving brick drain"),
    "dirt_tyre_tracks": _source("TireTracks001", (1.8, 0.45), "floor dirt tyre tracks"),
    "brown_runoff": _source("Leaking006", (0.4, 1.6), "wall grime water stain"),
    "worn_road_stripe": _source("RoadLines003", (0.35, 1.4), "floor road marking paint", grey=True),
    "worn_cycle_symbol": _source("RoadLines005", (1.55, 1.55), "floor road marking paint bicycle", grey=True),
    "asphalt_spalled_seam": _source("AsphaltDamageSet001", (2, 0.5), "floor road damage asphalt",
                                   crop=(0, 0, 1000, 246)),
    "asphalt_hairline_cracks": _source("AsphaltDamageSet001", (2, 0.5), "floor road damage asphalt crack",
                                     crop=(0, 258, 1000, 485)),
    "asphalt_failed_patch": _source("AsphaltDamageSet001", (2, 0.48), "floor road damage asphalt crack",
                                   crop=(0, 490, 1000, 731)),
    "asphalt_broken_joint": _source("AsphaltDamageSet001", (2, 0.5), "floor road damage asphalt crack",
                                   crop=(0, 741, 1000, 1000)),
    "dirt_round_patch": _source("AsphaltDamageSet002", (0.65, 0.65), "floor road dirt grime scuff",
                                crop=(63, 35, 464, 430)),
    "dirt_scattered_patch": _source("AsphaltDamageSet002", (0.8, 0.8), "floor road dirt grime gravel",
                                    crop=(537, 0, 994, 440)),
    "dirt_swept_patch": _source("AsphaltDamageSet002", (0.71, 0.87), "floor road dirt grime scuff",
                                crop=(70, 509, 467, 998)),
    "dirt_litter_patch": _source("AsphaltDamageSet002", (0.64, 0.6), "floor road dirt grime litter",
                                 crop=(589, 584, 936, 909)),
    "graffiti_angular_tag": _source("GraffitiSet001", (0.72, 0.68), "wall graffiti paint",
                                    crop=(0, 0, 250, 242)),
    "graffiti_blue_bubble": _source("GraffitiSet001", (0.75, 0.73), "wall graffiti paint",
                                    crop=(749, 0, 1000, 245)),
    "graffiti_orange_letters": _source("GraffitiSet001", (0.85, 0.8), "wall graffiti paint",
                                       crop=(748, 252, 1000, 488)),
    "graffiti_yellow_outline": _source("GraffitiSet001", (0.75, 0.68), "wall graffiti paint",
                                       crop=(499, 507, 747, 731)),
    "graffiti_pink_script": _source("GraffitiSet001", (0.8, 0.51), "wall graffiti paint",
                                    crop=(500, 760, 748, 925)),
    "cardboard_taped_scrap": _source("CardboardSet001", (0.38, 0.46), "floor warehouse litter cardboard",
                                     crop=(34, 0, 244, 256)),
    "cardboard_printed_scrap": _source("CardboardSet001", (0.43, 0.42), "floor warehouse litter cardboard",
                                       crop=(0, 260, 244, 491)),
    "cardboard_torn_corner": _source("CardboardSet001", (0.45, 0.47), "floor warehouse litter cardboard",
                                     crop=(0, 499, 247, 747)),
    "cardboard_folded_flap": _source("CardboardSet001", (0.42, 0.53), "floor warehouse litter cardboard",
                                     crop=(787, 751, 979, 1000)),
}

DECALS = LEGACY_DECALS | SURFACE_DECALS
