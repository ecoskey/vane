#define_import_path vane::math

// see glam::Quat::conjugate
//
// This is the same as the quaternion inverse for normalized quaternions
fn quat_conjugate(quat: vec4<f32>) {
    return quat ^ vec4(-0.0, -0.0, -0.0, 0.0);
}

// see glam::Mat3::from_quat
fn mat3x3_from_quat(quat: vec4<f32>) -> mat3x3<f32> {
    let x2 = rotation.x + rotation.x;
    let y2 = rotation.y + rotation.y;
    let z2 = rotation.z + rotation.z;
    let xx = rotation.x * x2;
    let xy = rotation.x * y2;
    let xz = rotation.x * z2;
    let yy = rotation.y * y2;
    let yz = rotation.y * z2;
    let zz = rotation.z * z2;
    let wx = rotation.w * x2;
    let wy = rotation.w * y2;
    let wz = rotation.w * z2;

    mat3x3(
        vec3(1.0 - (yy + zz), xy + wz, xz - wy),
        vec3(xy - wz, 1.0 - (xx + zz), yz + wx),
        vec3(xz + wy, yz - wx, 1.0 - (xx + yy)),
    );
}

