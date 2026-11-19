"""Build the second-pass Warehouse office and shop props."""
from math import sin
from pathlib import Path
import sys

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_warehouse_dressing import block, palette, rod
from build_warehouse_props import export, material


def main():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    p = palette()
    glass = material("Tinted office glass", (.28, .40, .43), roughness=.12)
    glass.node_tree.nodes.get("Principled BSDF").inputs["Alpha"].default_value = .23
    if hasattr(glass, "surface_render_method"):
        glass.surface_render_method = "BLENDED"
    for x in (-.48, 0, .48):
        block("Aluminum mullion", x, 0, 0, .04, .07, 2.12, p["frame"])
    for z in (0, .075, 2.07):
        block("Horizontal frame", 0, 0, z, 1, .065, .047, p["frame"])
    for x in (-.24, .24):
        block("Glazing", x, 0, .11, .45, .019, 1.95, glass, 0)
        block("Frosted safety band", x, -.012, 1.06, .45, .008, .06, p["paper"], 0)
    export("cubicle_glazing")

    for x in (-.43, .43):
        for y in (-.35, .35):
            block("Counter leg", x, y, 0, .05, .06, .87, p["frame"])
    block("Laminate worktop", 0, 0, .85, 1, .82, .072, p["wood"])
    block("Lower storage", 0, .05, .20, .85, .53, .056, p["navy"])
    export("dispatch_counter")

    for i in range(14):
        x, y = sin(2.4*i) * .65, sin(3.2*i) * .52
        paper = block("Scattered packing slips", x, y, .008+i*.001,
                      .21, .30, .003, p["paper"] if i % 3 else p["kraft"], 0)
        paper.rotation_euler.z = i * .57
        if i % 3 == 0:
            block("Printed line", x+.03, y, .014+i*.001, .10, .012, .001, p["ink"], 0)
    export("packing_slips")

    block("Printer base", 0, 0, 0, .46, .38, .14, p["frame"])
    block("Printer housing", 0, .02, .11, .44, .34, .22, p["cream"])
    block("Paper slot", 0, -.163, .162, .32, .024, .053, p["rubber"])
    block("Output paper", .012, -.24, .17, .22, .20, .01, p["paper"])
    block("Printer screen", .12, -.07, .334, .07, .06, .008, p["screen"])
    export("office_printer")

    for x in (-.52, .52):
        for y in (-.31, .31):
            rod("Rubber caster", (x-.023,y,.045),(x+.023,y,.045),.045,p["rubber"])
            block("Cart tube", x, y, .084, .037, .037, .85, p["steel"])
    for z in (.17,.51,.87):
        block("Blue shelf", 0, 0, z, 1.12, .70, .056, p["navy"])
        for y in (-.324,.324):
            block("Shelf lip", 0, y, z+.025, 1.12, .032, .095, p["navy"])
    for i in range(5):
        block("Open stock carton", (i%3-1)*.31, .05, .57 if i<3 else .92,
              .23, .34, .18+(i%2)*.08,p["kraft"])
    export("shop_supply_cart")

    block("Flush door frame", 0, 0, 0, 1.08, .15, 2.16, p["frame"])
    block("Painted door leaf", 0, -.088, .055, .94, .045, 2.05, p["yellow"])
    block("Vision panel", 0, -.115, 1.35, .22, .018, .43, p["frame"])
    block("Push plate", .29, -.12, .89, .11, .021, .31, p["steel"])
    export("warehouse_service_door")

    block("Loading jamb bumper", 0, 0, .05, .32, .24, 3.15, p["rubber"])
    for z in (.36,.89,1.42,1.95,2.48,3.01):
        block("Reflective dock stripe", 0, -.127, z, .33, .012, .16, p["red"])
    export("warehouse_dock_bumper")


if __name__ == "__main__":
    main()
