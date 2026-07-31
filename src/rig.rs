pub type Matrix = [f32; 16];

pub const IDENTITY: Matrix = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// Writes a rigid frame from three orthonormal axes and an origin.
pub fn set_frame(
    out: &mut Matrix,
    position: [f32; 3],
    x_axis: [f32; 3],
    y_axis: [f32; 3],
    z_axis: [f32; 3],
) {
    out[0] = x_axis[0];
    out[1] = x_axis[1];
    out[2] = x_axis[2];
    out[3] = 0.0;
    out[4] = y_axis[0];
    out[5] = y_axis[1];
    out[6] = y_axis[2];
    out[7] = 0.0;
    out[8] = z_axis[0];
    out[9] = z_axis[1];
    out[10] = z_axis[2];
    out[11] = 0.0;
    out[12] = position[0];
    out[13] = position[1];
    out[14] = position[2];
    out[15] = 1.0;
}

/// Builds a frame from a bone direction and a reference front.
pub fn set_frame_from_direction(
    out: &mut Matrix,
    position: [f32; 3],
    direction: [f32; 3],
    reference: [f32; 3],
) {
    let mut length = norm(direction);
    if length < 1e-6 {
        length = 1.0;
    }
    let y = [
        direction[0] / length,
        direction[1] / length,
        direction[2] / length,
    ];

    let mut x = cross(y, reference);
    let mut length = norm(x);
    if length < 1e-5 {
        x = cross(y, [1.0, 0.0, 0.0]);
        length = norm(x).max(1e-6);
    }
    let x = [x[0] / length, x[1] / length, x[2] / length];

    set_frame(out, position, x, y, cross(x, y));
}

/// `out = a * b`, both rigid.
pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for column in 0..4 {
        let base = column * 4;
        let (bx, by, bz, bw) = (b[base], b[base + 1], b[base + 2], b[base + 3]);
        out[base] = a[0] * bx + a[4] * by + a[8] * bz + a[12] * bw;
        out[base + 1] = a[1] * bx + a[5] * by + a[9] * bz + a[13] * bw;
        out[base + 2] = a[2] * bx + a[6] * by + a[10] * bz + a[14] * bw;
        out[base + 3] = a[3] * bx + a[7] * by + a[11] * bz + a[15] * bw;
    }
    out
}

/// Inverse of a rigid transform: transpose the rotation and negate the rotated
/// translation.
pub fn invert_rigid(m: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    out[0] = m[0];
    out[1] = m[4];
    out[2] = m[8];
    out[3] = 0.0;
    out[4] = m[1];
    out[5] = m[5];
    out[6] = m[9];
    out[7] = 0.0;
    out[8] = m[2];
    out[9] = m[6];
    out[10] = m[10];
    out[11] = 0.0;
    out[12] = -(m[0] * m[12] + m[1] * m[13] + m[2] * m[14]);
    out[13] = -(m[4] * m[12] + m[5] * m[13] + m[6] * m[14]);
    out[14] = -(m[8] * m[12] + m[9] * m[13] + m[10] * m[14]);
    out[15] = 1.0;
    out
}

/// Transforms a point.
pub fn transform_point(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn norm(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
