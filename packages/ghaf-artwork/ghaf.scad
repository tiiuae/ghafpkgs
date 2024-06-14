// Copyright 2024 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0
color("#3D8252")

scale([0.2, 0.2, 0.2]) {

    difference() {
        scale([20,20,20])
        linear_extrude(1) {
            import("icons/ghaf-white.svg");
        };

        translate([90, 90, 30]) {
            resize([80, 80, 30]) sphere(20);
        }
    }


    translate([80,-5,0]) {
        difference() {
            cylinder(10, r=10);
            translate([0,0,-1])
            cylinder(22, r=5, center=false);
        }
    }
}
