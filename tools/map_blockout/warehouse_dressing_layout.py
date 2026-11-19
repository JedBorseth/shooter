"""Warehouse dressing positions and approximate collision footprints in metres."""
def prop(group, name, model, x, y, z, yaw=0, collider=None, size=(1, 1, 1)):
    return dict(
        group=group, name=name, model=model, position=(x, y, z), yaw=yaw,
        collider=collider, scale=size,
    )


def cluster(group, model, points, collider, prefix):
    return [
        prop(group, f"{prefix}{index:02d}", model, x, 0, z, yaw, collider, (scale,) * 3)
        for index, (x, z, yaw, scale) in enumerate(points, 1)
    ]


PROPS = [
    *cluster("ProductionStock", "stacked_corrugated", [
        (-12.8, -20.0, 13, 1.24), (-10.0, -19.65, -3, 1.10),
        (-5.2, -19.4, 5, 1.22), (-2.5, -19.8, 0, 0.96),
        (-13.6, -10.9, 0, 1.18), (-13.4, -8.7, 9, 0.93),
        (-10.3, -7.7, 84, 1.28), (-8.0, -7.22, 91, 1.10),
        (3.1, -4.9, 18, 1.20), (5.1, -5.34, 8, 1.05),
        (-12.4, 3.5, 11, 1.28), (-9.9, 3.8, -5, 1.18),
        (-14.0, 8.0, 86, 1.10), (-14.3, 10.5, 92, 1.32),
        (-6.1, 5.6, 87, 1.25), (-5.8, 7.8, 82, 1.07),
        (3.4, 6.0, 6, 1.10), (5.4, 4.6, 3, 1.02),
        (29.4, -6.65, 88, 1.28), (29.4, -3.65, 92, 1.10),
        (32.6, -1.1, 9, 1.17), (32.5, 1.4, -8, 1.08),
        (30.8, 10.1, 95, 1.18), (30.8, 12.5, 91, 1.28),
        (17.5, 20.2, -1, 1.10), (20.2, 20.4, 4, 1.20),
        (29.1, 23.0, 90, 1.28), (29.5, 25.6, 92, 1.10),
        (-10.5, 20.1, -2, 1.12), (-7.6, 19.6, -6, 1.09),
        (-12.9, 27.6, 87, 1.13), (-12.6, 30.6, 93, 1.29),
        (-1.0, 29.8, -4, 1.23), (1.6, 30.25, 1, 0.97),
    ], (1.63, 2.38, 1.16), "PalletizedStock"),
    *cluster("ProductionStock", "corrugated_pallet", [
        (-5.7, -6.5, 81, 1.18), (-2.9, -7.65, -3, 1.08),
        (2.3, 4.1, 13, 1.12), (0.4, 6.1, -3, 1.15),
        (-4.4, 15.6, 8, 1.1), (-4.1, 17.8, -8, 1.05),
        (14.2, 18.55, 4, 1.10), (21.8, 23.3, 6, 1.18),
        (28.4, -19.8, 12, 1.1), (32.2, 15.55, 90, 1.13),
    ], (1.52, 0.61, 1.12), "CutSheets"),
    *cluster("ProductionStock", "paper_rolls", [
        (30.8, -19.45, 93, 1.09), (32.7, -17.4, 91, 0.98),
        (32.35, -12.15, 82, 1.1), (30.6, -10.45, 15, 0.98),
        (22.0, 4.6, 2, 1.01), (25.2, 5.17, -7, 1.03),
    ], (2.46, 2.42, 1.49), "KraftRolls"),
    *cluster("ProductionStock", "empty_pallet_stack", [
        (-15.6, -15.3, 91, 1.13), (-12.2, 17.65, 4, 1.08),
        (31.65, 7.0, 91, 1.0), (20.8, 29.8, -4, 1.05),
    ], (1.35, 1.18, 1.06), "EmptyPallets"),
    *cluster("ProductionStock", "scrap_hopper", [
        (-11.4, -4.0, 9, 1.15), (-14.0, 13.7, 87, 1.02),
        (3.88, 10.0, 0, 1.13), (27.25, 19.0, -14, 1.08),
        (31.2, -23.4, 90, 1.03),
    ], (1.68, 1.10, 1.31), "ScrapHopper"),
    *cluster("ProductionStock", "warehouse_workbench", [
        (-14.65, 1.8, 85, 1.0), (-3.6, 23.8, 0, 1.0),
        (8.4, 20.7, 9, 0.96), (29.65, 3.72, 90, 0.96),
    ], (3.22, 1.12, 1.04), "ToolBench"),
    *cluster("ProductionStock", "warehouse_guardrail", [
        (-4.5, -18.20, 2, 1.02), (-3.65, 8.7, 92, 1.00),
        (27.6, 6.2, 2, 0.98), (16.8, 24.3, 82, 1.00),
    ], (2.35, 1.02, 0.20), "FloorGuard"),

    *cluster("LeftWingStock", "stock_rack", [
        (-34.15, -19.9, 90, 1.03), (-34.15, -15.95, 90, 1.03),
        (-34.15, -4.85, 90, 1.03), (-34.15, -0.9, 90, 1.03),
        (-34.15, 3.12, 90, 1.03), (-34.15, 7.15, 90, 1.03),
        (-18.08, -18.5, -90, 1.03), (-18.08, -14.2, -90, 1.03),
        (-18.12, 11.85, -90, 1.0), (-18.12, 16.15, -90, 1.0),
    ], (3.49, 3.4, 1.25), "StockRack"),
    *cluster("LeftWingStock", "stacked_corrugated", [
        (-30.4, -20.8, 14, 1.30), (-30.45, -18.7, 7, 1.10),
        (-26.6, -15.25, 85, 1.23), (-24.25, -15.05, 86, 1.15),
        (-28.7, -8.6, -4, 1.28), (-25.93, -8.6, 7, 1.23),
        (-23.45, -5.75, 90, 1.22), (-23.52, -3.65, 92, 1.02),
        (-28.2, 0.0, 11, 1.33), (-28.3, 2.57, -5, 1.12),
        (-20.75, 4.75, -1, 1.13), (-20.7, 7.18, 2, 1.25),
        (-29.15, 11.83, 8, 1.29), (-31.70, 11.78, -1, 1.11),
        (-21.4, 19.9, 91, 1.07), (-21.4, 26.95, 85, 1.12),
    ], (1.63, 2.38, 1.16), "WingStock"),
    *cluster("LeftWingStock", "paper_rolls", [
        (-31.60, -10.75, 0, 1.05), (-29.35, -11.0, -10, 0.92),
        (-29.66, 7.65, 0, 1.05), (-25.77, 10.4, 95, 0.91),
    ], (2.46, 2.42, 1.49), "WingRolls"),
    *cluster("LeftWingStock", "empty_pallet_stack", [
        (-20.1, -9.7, 87, 1.09), (-32.9, 9.50, 0, 1.08),
        (-26.0, 5.88, 9, 1.0), (-22.15, 14.1, 13, 1.0), (-27.3, 28.8, 0, 1.04),
    ], (1.35, 1.18, 1.06), "WingPallets"),

    prop("OfficeFurniture", "ProductionDesk", "office_desk", 3.2, 0, 27.0, -90, (1.8, .79, .75)),
    prop("OfficeFurniture", "ProductionDeskReturn", "warehouse_workbench", 3.25, 0, 29.85, -90, (3.22, 1.1, 1.04), (.64, 1, .82)),
    prop("OfficeFurniture", "ProductionChair", "office_chair", 2.12, 0, 27.15, 90, (.54, .98, .60)),
    prop("OfficeFurniture", "HallOfficeDesk", "office_desk", 12.4, 0, 30.7, 180, (1.8, .79, .75)),
    prop("OfficeFurniture", "HallOfficeFrontDesk", "office_desk", 6.4, 0, 27.3, 90, (1.8, .79, .75)),
    prop("OfficeFurniture", "HallOfficeChair", "office_chair", 12.4, 0, 29.75, 0, (.54, .98, .60)),
    prop("OfficeFurniture", "HallOfficeFrontChair", "office_chair", 7.45, 0, 27.30, -90, (.54, .98, .60)),
    prop("OfficeFurniture", "HallFilingBank", "office_files", 6.0, 0, 31.3, 180, (.95, 1.67, .72), (1.4, 1, 1)),
    prop("OfficeFurniture", "HallFilingBankCorner", "office_files", 14.5, 0, 26, -90, (.95, 1.67, .72)),
    prop("OfficeFurniture", "WingOfficeDesk", "office_desk", -32.15, 0, 24.35, 180, (1.8, .79, .75)),
    prop("OfficeFurniture", "WingOfficeIntake", "office_desk", -32.5, 0, 16.53, 0, (1.8, .79, .75)),
    prop("OfficeFurniture", "WingOfficeChair", "office_chair", -32.2, 0, 23.35, 0, (.54, .98, .60)),
    prop("OfficeFurniture", "WingOfficeIntakeChair", "office_chair", -32.5, 0, 17.55, 180, (.54, .98, .60)),
    prop("OfficeFurniture", "WingOfficeFiles", "office_files", -33.95, 0, 20.65, 90, (.95, 1.67, .72), (1.65, 1, 1)),
    prop("OfficeFurniture", "WingOfficeSupply", "office_files", -27.0, 0, 16.2, 0, (.95, 1.67, .72), (1.35, 1, 1)),

    prop("LoadingYard", "DockForklift", "forklift", 15.3, -1.25, 40.0, 93, (1.22, 1.85, 2.00)),
    *[
        prop("LoadingYard", f"DockStock{i:02d}", "stacked_corrugated", x, -1.25, z, yaw,
             (1.63, 2.38, 1.16), (scale,) * 3)
        for i, (x, z, yaw, scale) in enumerate([
            (-26.4, 41.5, 15, 1.25), (-23.85, 41.1, 8, 1.22), (-21.2, 41.8, 14, 1.05),
            (-14.65, 36.0, 8, 1.21), (-12.0, 36.35, -6, 1.02),
            (-1.25, 39.8, 10, 1.26), (1.45, 39.82, 3, 1.10),
            (10.8, 43.42, -6, 1.13), (13.45, 43.0, 5, 1.20),
            (28.6, 42.5, 91, 1.24), (31.2, 42.65, 92, 1.15),
        ], 1)
    ],
    prop("LoadingYard", "DockEmptyPallets", "empty_pallet_stack", -9.95, -1.25, 41.5, 0, (1.34, 1.18, 1.06)),
    prop("LoadingYard", "DockSecondPalletBank", "empty_pallet_stack", 32.5, -1.25, 37.3, 90, (1.34, 1.18, 1.06)),
    prop("LoadingYard", "DockScrap", "scrap_hopper", 4.65, -1.25, 36.2, 8, (1.68, 1.1, 1.31)),
    prop("LoadingYard", "DockServiceBench", "warehouse_workbench", 12.5, -1.25, 34.55, 180, (3.23, 1.12, 1.05)),
    prop("ParkingDressing", "ParkedCarWest", "res://assets/props/covered_car/covered_car.gltf", -18.5, 0, -31.55, 90, (1.85, 1.35, 4.2)),
    prop("ParkingDressing", "ParkedCarEast", "res://assets/props/covered_car/covered_car.gltf", 8.2, 0, -31.4, 89, (1.85, 1.35, 4.2)),
    prop("ParkingDressing", "ParkingTrash", "res://assets/props/trash_can/trash_can.gltf", -34.5, 0, -28, 0, (0.55, 0.95, 0.61)),
    prop("ParkingDressing", "ParkingGuard", "warehouse_guardrail", -31.4, 0, -29.4, 90, (2.35, 1.02, 0.2)),

    prop("Overhead", "ProductionRoofAndJoists", "production_overhead", 9.25, 0, 3.5),
    prop("Overhead", "WingRoofAndJoists", "storage_overhead", -26.25, 0, 3.25),
    prop("Structure", "PaintedFloorAisles", "warehouse_floor_marks", 0, 0, 0),
    *[
        prop("Structure", f"Column{i:02d}", "warehouse_column", x, 0, z, yaw, (.49, 7.36, .52))
        for i, (x, z, yaw) in enumerate([
            (-15.86, -19, 90), (-15.86, -11, 90), (-15.86, 5.2, 90), (-15.86, 13.1, 90),
            (-15.86, 20.1, 90), (34.4, -19, -90), (34.4, -10, -90), (34.4, 0.8, -90),
            (34.4, 10.6, -90), (34.4, 20.9, -90), (34.4, 29.8, -90),
            (1.25, -5.90, 6), (1.25, 7.55, -4), (10.5, 21.8, 0),
            (-33.55, -11.90, 90), (-33.55, 11.0, 90),
        ], 1)
    ],
    *[
        prop("Structure", f"Clerestory{i:02d}", "clerestory_window", x, 5.05, z, yaw)
        for i, (x, z, yaw) in enumerate([
            (-20.6, -23.95, 0), (-7.0, -25.43, 0), (7.0, -25.43, 0),
            (24.7, -25.43, 0), (34.92, -17, -90), (34.92, -7.3, -90),
            (34.92, 3.6, -90), (34.92, 14, -90), (34.92, 26.4, -90),
            (-34.95, -15, 90), (-34.95, -4, 90), (-34.95, 8.4, 90),
            (-24.4, 30.46, 180), (20.5, 32.45, 180),
        ], 1)
    ],
]

FINISHING_PROPS = [
    *cluster("ProductionStock", "strapping_cart", [
        (-6.35, 16.9, -18, 1.0), (5.2, -7.36, -2, 1.05),
        (24.6, -6.14, 9, 1.0), (23.9, 18.6, -6, 1.0), (-9.6, 23.7, -4, 1.0),
    ], (1.25, 1.0, .81), "StrappingCart"),
    *cluster("ProductionStock", "pallet_jack", [
        (-11.35, 6.05, 96, 1.0), (-3.4, -19.25, 37, 1.0),
        (26.25, 24.15, -28, 1.0), (31.6, -8.25, -76, 1.0),
        (12.9, 18.8, 0, 1.0),
    ], (.70, .60, 1.66), "PalletJack"),
    *cluster("LeftWingStock", "pallet_jack", [
        (-30.4, -6.9, 85, 1.0), (-25.5, 5.9, 11, 1.0), (-23.7, 27.74, 90, 1.0),
    ], (.70, .6, 1.65), "WingJack"),
    *cluster("LeftWingStock", "warehouse_workbench", [
        (-32.6, -22.9, 0, 1.0), (-32.5, 28.75, 90, .86),
    ], (3.23, 1.1, 1.06), "WingBench"),
    prop("OfficeFurniture", "HallOfficeSupplyBench", "warehouse_workbench", 14.1, 0, 29.15, -90, (3.22, 1.1, 1.04), (.77, .82, .9)),
    prop("OfficeFurniture", "WingOfficeBench", "warehouse_workbench", -29.7, 0, 25.15, 180, (3.22, 1.1, 1.04), (.78, .82, .9)),
    prop("OfficeFurniture", "WingOfficePrintStation", "office_desk", -29.6, 0, 16.1, 0, (1.8, .79, .75), (.8, 1, 1)),
    prop("OfficeFurniture", "HallWasteBin", "rolling_bin", 5.1, 0, 29.5, 80, (1.04, .86, .74), (.59, .64, .59)),
    prop("OfficeFurniture", "WingWasteBin", "rolling_bin", -33.3, 0, 23.2, 95, (1.04, .86, .74), (.59, .64, .59)),
    *[
        prop(group, f"ShippingCarton{i:02d}", "res://assets/props/cardboard_box/cardboard_box.gltf",
             x, y, z, yaw, None, (scale,) * 3)
        for i, (group, x, y, z, yaw, scale) in enumerate([
            ("ProductionStock", -9.9, 0, 14.6, 12, 1),
            ("ProductionStock", -7.15, 0, 18.15, 65, 1.2),
            ("ProductionStock", -12.9, 0, 7.18, -12, 1.2),
            ("ProductionStock", 4.7, 0, 3.52, 18, 1.3),
            ("ProductionStock", 2.9, 0, 3.08, -17, .9),
            ("ProductionStock", 30.5, 0, 13.95, 11, 1.3),
            ("LeftWingStock", -25.2, 0, -12.8, 23, 1.2),
            ("LeftWingStock", -33.1, 0, 9.05, 14, .99),
            ("LeftWingStock", -19.8, 0, 6.7, -22, 1.2),
            ("OfficeFurniture", -31.65, 0, 18.1, 87, 1.0),
            ("OfficeFurniture", -27.0, 0, 23.5, 11, 1.25),
            ("OfficeFurniture", 5.7, 0, 30.3, -13, 1.0),
            ("OfficeFurniture", 14.17, 0.80, 29.12, -5, .9),
            ("OfficeFurniture", 3.40, 0.80, 29.94, 8, .8),
            ("LoadingYard", -21.2, -1.25, 40.6, 77, 1.2),
            ("LoadingYard", 5.2, -1.25, 39.2, -3, 1.1),
        ], 1)
    ],
    prop("LoadingYard", "WestDeliveryTruck", "delivery_truck", -18.5, -1.25, 39.1, -4, (2.43, 3.34, 6.91)),
    prop("LoadingYard", "EastDeliveryTruck", "delivery_truck", 20.8, -1.25, 40.3, 5, (2.43, 3.34, 6.91)),
    prop("LoadingYard", "DockWestGuard", "warehouse_guardrail", -9.2, -1.25, 35.1, 90, (2.31, 1.04, .23)),
    prop("LoadingYard", "DockEastGuard", "warehouse_guardrail", 29.3, -1.25, 35.2, 90, (2.31, 1.04, .23)),
    *[
        prop("Structure", f"Portal{i:02d}", "warehouse_portal", x, 0, z, yaw, None, (w, 1, 1))
        for i, (x, z, yaw, w) in enumerate([
            (-24.75, -24.28, 0, 2.5), (-12.75, -25.75, 0, 2.5),
            (-0.25, -25.75, 0, 2.5), (16.5, -25.75, 0, 3.0),
            (-16.75, -2.0, -90, 5.0), (-16.75, 23.75, -90, 2.5),
            (-31.75, 30.75, 0, 2.5), (-6.5, 32.75, 0, 3.0), (26, 32.75, 0, 4.0),
        ], 1)
    ],
]
PROPS.extend(FINISHING_PROPS)

OFFICE_CEILING_PROPS = [
    prop("OfficeFurniture", "HallOfficeCeiling", "office_ceiling", 9.75, 0, 28.5, 0, None, (11.5, 1, 8.0)),
    prop("OfficeFurniture", "WingOfficeCeiling", "office_ceiling", -30, 0, 20.5, 0, None, (10.0, 1, 12.0)),
    prop("OfficeFurniture", "HallRoofStock", "corrugated_pallet", 13.3, 3.17, 31.45, -8),
    prop("OfficeFurniture", "WingRoofEmptyPallets", "empty_pallet_stack", -32.95, 3.17, 24.5, 9),
    prop("OfficeFurniture", "WingRoofStock", "corrugated_pallet", -31.05, 3.17, 24.45, -12),
]
PROPS.extend(OFFICE_CEILING_PROPS)

# Sources are ready-to-place machine envelopes. The brushes remain underneath
# as collision and the local origin is aligned to the bottom of each brush.
MACHINES = {
    "Jumbo Back": "jumbo_infeed",
    "Jumbo Crawl Section Roof": "jumbo_crawl_hood",
    "Jumbo Die Cut Machine": "jumbo_die_cutter",
    "Sheeter Machine": "sheet_cutter",
    "Rotary Machine": "rotary_press",
}

# Second-pass furnishing for the fictional office modules and shop floor.
OFFICE_SECOND_PASS = []
for west in (True, False):
    for north in (True, False):
        suffix = ("West" if west else "East") + ("North" if north else "South")
        x_wall = -31.0 if west else -29.0
        z_wall = 18.65 if north else 22.35
        z_center = 16.86 if north else 24.16
        x_outer = -33.55 if west else -26.55
        OFFICE_SECOND_PASS.extend([
            prop("WingCubicles", "Spine" + suffix, "cubicle_glazing",
                 x_wall, 0, z_center, 90, (1.0, 2.12, .065), (3.50, 1, 1)),
            prop("WingCubicles", "Return" + suffix, "cubicle_glazing",
                 x_outer, 0, z_wall, 0, (1.0, 2.12, .065), (1.26, 1, 1)),
            prop("WingCubicles", "Station" + suffix, "office_desk",
                 -33.3 if west else -26.9, 0, 15.88 if north else 25.12,
                 180 if north else 0, (1.8, .79, .75)),
            prop("WingCubicles", "Seat" + suffix, "office_chair",
                 -33.05 if west else -26.9, 0, 16.81 if north else 24.21,
                 0 if north else 180, (.54, .98, .60)),
            prop("WingCubicles", "Printer" + suffix, "office_printer",
                 -33.74 if west else -26.44, .79, 15.85 if north else 25.14,
                 180 if north else 0),
        ])

OFFICE_SECOND_PASS += [
    prop("WingCubicles", "SharedPaperwork", "packing_slips", -29.9, 0, 20.4, -9),
    prop("WingCubicles", "ShippingPaperwork", "packing_slips", -29.0, 0, 21.33, 32, None, (.65, 1, .65)),
    prop("WingCubicles", "SupplyCart", "shop_supply_cart", -34.0, 0, 20.7, 81, (1.12, 1.17, .7)),
    prop("WingCubicles", "ReturnCartons", "res://assets/props/cardboard_box/cardboard_box.gltf", -31.9, 0, 20.8, 26),
    prop("WingCubicles", "BoxLids", "packing_slips", -33, 0, 19.73, 90, None, (.68, 1, .68)),
    prop("HallDispatch", "NorthCounter", "dispatch_counter", 13.45, 0, 23.78, 0, (1,.92,.82), (4.70,1,1)),
    prop("HallDispatch", "EastCounter", "dispatch_counter", 15.80, 0, 27.1, 90, (1,.92,.82), (5.82,1,1)),
    prop("HallDispatch", "DispatchPrinter", "office_printer", 13.38, .922, 23.73, 180),
    prop("HallDispatch", "DispatchPaperwork", "packing_slips", 12.1, .922, 23.8, 11, None, (.65, 1, .65)),
    prop("HallDispatch", "CounterCarton", "res://assets/props/cardboard_box/cardboard_box.gltf", 15.8, .922, 26.91, 12, None, (.76,.76,.76)),
    prop("HallDispatch", "DispatchCart", "shop_supply_cart", 17.15, 0, 29.1, 91, (1.12, 1.17, .7)),
    prop("HallDispatch", "FloorPaperwork", "packing_slips", 11.6, 0, 22.55, 32),
    prop("HallDispatch", "OfficeDoorPanel", "warehouse_service_door", 14.05, 0, 31.93, 180),
    prop("HallDispatch", "WorkshopDoorPanel", "warehouse_service_door", 34.92, 0, 28.3, -90),
    prop("WingCubicles", "SupplyDoorPanel", "warehouse_service_door", -34.43, 0, 23.1, 90),
]

SHOP_SECOND_PASS = [
    *cluster("ShopSecondPass", "shop_supply_cart", [
        (-7.0, -17.55, 82, .93), (1.7, -6.3, 5, 1.0),
        (5.7, -1.79, 88, .97), (29.4, -1.6, -14, 1.02),
        (23.6, 3.25, 7, 1.05), (-6.6, 15.7, -10, .94),
        (28.9, 7.85, 78, .94), (13.8, 18.9, 19, .98),
        (-19.9, -11.7, -72, .91), (-32, 6.38, 9, .90),
        (-23.2, -7.7, -4, 1.0), (-11.0, 26.45, 22, .97),
    ], (1.12, 1.10, .72), "OperatorCart"),
    *[
        prop("ShopSecondPass", f"LoosePackingSlips{i:02d}", "packing_slips", x, y, z, yaw, None, (s,1,s))
        for i, (x,y,z,yaw,s) in enumerate([
            (-9.8,0,15.3,7,1),(-5.8,0,11.0,0,.76),(-5.7,0,-5.95,13,.93),
            (-7.6,0,-18.4,91,.8), (5.45,0,-.75,25,.8), (27.2,0,-17.8,7,1.15),
            (26.4,0,3.85,63,.90), (19,0,18.28,76,.82), (29.8,0,23.65,12,.78),
            (-26.45,0,3.65,91,.85),(-30.6,0,12.7,55,1.03),(-22,0,-14.85,23,.81),
            (13.7,-1.25,37.55,62,.89),(-22.15,-1.25,42.95,10,.73),
        ],1)
    ],
    *[
        prop("ShopSecondPass", f"OpenCarton{i:02d}", "res://assets/props/cardboard_box/cardboard_box.gltf",
             x, 0, z, yaw, None, (s,s,s))
        for i,(x,z,yaw,s) in enumerate([
            (-7,13.9,-4,1.1),(-8.14,17.66,15,1.2),(4,7.2,67,1.11),
            (3.4,7.25,4,.88), (31.2,9.3,15,1.22),(32.2,-20.8,-6,1.06),
            (-10.2,-17.55,13,1.2),(-14.65,5.1,22,.89),(-22.4,10.1,2,1.12),
            (-30.9,4.2,2,1.08),(-30.55,4.9,88,.97),(-18.45,17.91,22,1.06),
            (17.1,24.9,28,1.04),(18.1,25.3,-22,1.11),
        ],1)
    ],
    *[
        prop("ShopSecondPass", f"BayBuffer{i}", "warehouse_dock_bumper", x, -1.25, 33.12, 0)
        for i,x in enumerate((-8.28,-4.72,23.72,28.28),1)
    ],
]

PROPS[:] = [
    item for item in PROPS
    if item["name"] != "PalletizedStock34"
    and not (
        item["group"] == "OfficeFurniture"
        and item["name"].startswith("Wing")
        and "Ceiling" not in item["name"]
        and "Roof" not in item["name"]
    )
]
for item in PROPS:
    if item["name"] == "PalletizedStock33":
        item["position"] = (-1.0, 0, 31.29)
PROPS.extend(OFFICE_SECOND_PASS + SHOP_SECOND_PASS)

RELOCATED_PROPS = {
    "PalletizedStock13": (-15.5, 0, 7.4),
    "PalletizedStock14": (-14.8, 0, 10.9),
    "ScrapHopper02": (-15.2, 0, 14.8),
    "HallOfficeDesk": (5.97, 0, 30.73),
    "HallOfficeFrontDesk": (-1.78, 0, 27.82),
    "HallOfficeChair": (5.97, 0, 29.78),
    "HallOfficeFrontChair": (-.71, 0, 27.82),
    "HallFilingBank": (-1.7, 0, 31.42),
    "HallFilingBankCorner": (3.04, 0, 25.56),
    "HallOfficeSupplyBench": (7.4, 0, 30.35),
    "HallWasteBin": (2.8, 0, 31.05),
    "ProductionDesk": (-3.84, 0, 27.0),
    "ProductionDeskReturn": (-3.81, 0, 29.85),
    "ProductionChair": (-4.92, 0, 27.15),
    "NorthCounter": (6.4, 0, 27.93),
    "EastCounter": (9.04, 0, 29.65),
    "DispatchCart": (10.12, 0, 30.8),
}
for item in PROPS:
    item["position"] = RELOCATED_PROPS.get(item["name"], item["position"])
