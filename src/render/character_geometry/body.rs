//! The figure under the garments, lofted ring by ring from the skeleton.

use crate::render::character_geometry::hood::build_hood;
use crate::render::character_geometry::{
    HEAD_CENTRE, M_LEATHER, M_ROBE, M_SKIN, M_TRIM, Ring, builder, finish, limb_rings, loft, ring,
    spine_bones,
};
use crate::render::geometry::StaticMesh;
use crate::systems::figure::{
    B_FOOT_L, B_FOOT_R, B_FORE_L, B_FORE_R, B_HAND_L, B_HAND_R, B_HEAD, B_NECK, B_SHIN_L, B_SHIN_R,
    B_THIGH_L, B_THIGH_R, B_UPPER_L, B_UPPER_R,
};
use nightshade::prelude::wgpu;

/// The figure under the garments: head, cowl, torso, arms, trousers and boots.
pub fn build_body(device: &wgpu::Device) -> StaticMesh {
    let mut builder = builder();

    const TORSO: [[f32; 3]; 8] = [
        [0.88, 0.150, 0.120],
        [0.98, 0.142, 0.113],
        [1.06, 0.134, 0.106],
        [1.14, 0.140, 0.109],
        [1.22, 0.156, 0.118],
        [1.30, 0.172, 0.126],
        [1.38, 0.176, 0.126],
        [1.44, 0.160, 0.116],
    ];
    let torso: Vec<Ring> = TORSO
        .iter()
        .map(|entry| {
            ring(
                [0.0, entry[0], 0.0],
                [entry[1], entry[2]],
                0.72,
                spine_bones(entry[0]),
            )
        })
        .collect();
    loft(&mut builder, &torso, M_TRIM, [0.0, 0.0, 1.0], true, false);

    let belt = [
        ring([0.0, 0.955, 0.0], [0.153, 0.124], 0.62, spine_bones(0.955)),
        ring([0.0, 0.995, 0.0], [0.160, 0.130], 0.70, spine_bones(0.995)),
        ring([0.0, 1.035, 0.0], [0.152, 0.123], 0.62, spine_bones(1.035)),
    ];
    loft(
        &mut builder,
        &belt,
        M_LEATHER,
        [0.0, 0.0, 1.0],
        false,
        false,
    );

    let neck = [
        ring(
            [0.0, 1.42, -0.005],
            [0.062, 0.058],
            0.35,
            [B_NECK as f32, 1.0, B_HEAD as f32, 0.0],
        ),
        ring(
            [0.0, 1.50, 0.0],
            [0.058, 0.055],
            0.30,
            [B_NECK as f32, 0.5, B_HEAD as f32, 0.5],
        ),
        ring(
            [0.0, 1.56, 0.002],
            [0.062, 0.060],
            0.28,
            [B_HEAD as f32, 1.0, 0.0, 0.0],
        ),
    ];
    loft(&mut builder, &neck, M_SKIN, [0.0, 0.0, 1.0], false, false);

    let head: Vec<Ring> = (0..=8)
        .map(|index| {
            let angle = (index as f32 / 8.0) * std::f32::consts::PI;
            let radius = angle.sin();
            ring(
                [
                    0.0,
                    HEAD_CENTRE[1] - angle.cos() * 0.105,
                    HEAD_CENTRE[2] + radius * 0.006,
                ],
                [0.089 * radius + 0.004, 0.096 * radius + 0.004],
                0.22,
                [B_HEAD as f32, 1.0, 0.0, 0.0],
            )
        })
        .collect();
    loft(&mut builder, &head, M_SKIN, [0.0, 0.0, 1.0], true, true);

    let scarf = [
        ring(
            [0.0, 1.560, 0.010],
            [0.086, 0.092],
            0.30,
            [B_HEAD as f32, 1.0, 0.0, 0.0],
        ),
        ring(
            [0.0, 1.600, 0.012],
            [0.094, 0.100],
            0.34,
            [B_HEAD as f32, 1.0, 0.0, 0.0],
        ),
        ring(
            [0.0, 1.638, 0.008],
            [0.092, 0.098],
            0.30,
            [B_HEAD as f32, 1.0, 0.0, 0.0],
        ),
    ];
    loft(&mut builder, &scarf, M_TRIM, [0.0, 0.0, 1.0], false, false);

    build_hood(&mut builder);

    for arm in 0..2 {
        let side = if arm == 0 { -1.0 } else { 1.0 };
        let (upper, fore, hand) = if arm == 0 {
            (B_UPPER_L, B_FORE_L, B_HAND_L)
        } else {
            (B_UPPER_R, B_FORE_R, B_HAND_R)
        };

        let upper_rings = limb_rings(
            [side * 0.185, 1.400, 0.0],
            [side * 0.230, 1.123, 0.0],
            [0.064, 0.050],
            4,
            [upper, fore],
            0.55,
            [0.72, 1.0],
        );
        loft(
            &mut builder,
            &upper_rings,
            M_ROBE,
            [0.0, 0.0, 1.0],
            true,
            false,
        );

        let fore_rings = limb_rings(
            [side * 0.230, 1.123, 0.0],
            [side * 0.243, 0.866, 0.016],
            [0.050, 0.042],
            4,
            [fore, hand],
            0.62,
            [0.75, 1.0],
        );
        loft(
            &mut builder,
            &fore_rings,
            M_ROBE,
            [0.0, 0.0, 1.0],
            false,
            false,
        );

        let bones = [hand as f32, 1.0, 0.0, 0.0];
        let hand_rings = [
            ring([side * 0.243, 0.866, 0.016], [0.044, 0.038], 0.55, bones),
            ring([side * 0.245, 0.820, 0.024], [0.050, 0.040], 0.55, bones),
            ring([side * 0.247, 0.780, 0.032], [0.046, 0.036], 0.52, bones),
            ring([side * 0.248, 0.752, 0.038], [0.030, 0.026], 0.50, bones),
        ];
        loft(
            &mut builder,
            &hand_rings,
            M_LEATHER,
            [0.0, 0.0, 1.0],
            false,
            true,
        );
    }

    for leg in 0..2 {
        let side = if leg == 0 { -1.0 } else { 1.0 };
        let (thigh, shin, foot) = if leg == 0 {
            (B_THIGH_L, B_SHIN_L, B_FOOT_L)
        } else {
            (B_THIGH_R, B_SHIN_R, B_FOOT_R)
        };

        let thigh_rings = limb_rings(
            [side * 0.100, 0.905, 0.0],
            [side * 0.100, 0.460, 0.0],
            [0.114, 0.086],
            5,
            [thigh, shin],
            0.5,
            [0.74, 1.0],
        );
        loft(
            &mut builder,
            &thigh_rings,
            M_ROBE,
            [0.0, 0.0, 1.0],
            true,
            false,
        );

        let shin_rings = [
            ring(
                [side * 0.100, 0.460, 0.0],
                [0.086, 0.086],
                0.55,
                [shin as f32, 1.0, 0.0, 0.0],
            ),
            ring(
                [side * 0.100, 0.360, 0.004],
                [0.076, 0.076],
                0.55,
                [shin as f32, 1.0, 0.0, 0.0],
            ),
            ring(
                [side * 0.100, 0.270, 0.006],
                [0.070, 0.070],
                0.52,
                [shin as f32, 1.0, 0.0, 0.0],
            ),
            ring(
                [side * 0.100, 0.200, 0.006],
                [0.075, 0.076],
                0.48,
                [shin as f32, 0.6, foot as f32, 0.4],
            ),
            ring(
                [side * 0.100, 0.140, 0.004],
                [0.080, 0.082],
                0.44,
                [shin as f32, 0.25, foot as f32, 0.75],
            ),
            ring(
                [side * 0.100, 0.100, 0.0],
                [0.074, 0.078],
                0.42,
                [foot as f32, 1.0, 0.0, 0.0],
            ),
        ];
        loft(
            &mut builder,
            &shin_rings,
            M_ROBE,
            [0.0, 0.0, 1.0],
            false,
            false,
        );

        let bones = [foot as f32, 1.0, 0.0, 0.0];
        let boot_rings = [
            ring([side * 0.100, 0.055, -0.088], [0.046, 0.052], 0.35, bones),
            ring([side * 0.100, 0.058, -0.050], [0.056, 0.066], 0.38, bones),
            ring([side * 0.100, 0.054, 0.010], [0.058, 0.060], 0.42, bones),
            ring([side * 0.100, 0.048, 0.078], [0.056, 0.050], 0.45, bones),
            ring([side * 0.100, 0.043, 0.142], [0.050, 0.043], 0.48, bones),
            ring([side * 0.100, 0.040, 0.190], [0.033, 0.031], 0.48, bones),
        ];
        loft(
            &mut builder,
            &boot_rings,
            M_LEATHER,
            [0.0, 1.0, 0.0],
            true,
            true,
        );
    }

    finish(builder, device, "snow_character_body")
}
