#define_import_path vane::flow

#import vane::math::{quat_conjugate, mat3x3_from_quat}

struct Flow {
    translation: vec3<f32>,
    field_index: u32,
    rotation: vec4<f32>,
    scale: vec3<f32>,
    flags: u32,
    linear_velocity: vec3<f32>,
    layers: u32,
    angular_velocity: vec3<f32>,
    influence: f32,
}

const FLOW_FLAG_INHERIT_LINEAR_VELOCITY: u32 = 1 << 0;
const FLOW_FLAG_INHERIT_ANGULAR_VELOCITY: u32 = 1 << 1;

// -----------------------------------------------------------------------------

@group(#{FLOW_BIND_GROUP}) @binding(#{FLOW_BINDINGS_START} + 0) var<storage, read> flows: array<Flow>;
@group(#{FLOW_BIND_GROUP}) @binding(#{FLOW_BINDINGS_START} + 1) var fields: array<texture_3d<f32>>;
@group(#{FLOW_BIND_GROUP}) @binding(#{FLOW_BINDINGS_START} + 2) var field_sampler: sampler;

// -----------------------------------------------------------------------------

fn sample_flow(index: u32, pos_ws: vec3<f32>, layers: u32) -> vec4<f32> {
    let flow = &flows[i]; 

    var transform: FlowTransform;
    transform.local_from_world = mat3x3_from_quat(quat_conjugate(*flow.rotation);
    transform.world_from_local = mat3x3_from_quat(*flow.translation);

    sample_flow_with(index, transform, pos_ws, layers);
}

struct FlowTransform { 
    local_from_world: mat3x3<f32>,
    world_from_local: mat3x3<f32>,
}

fn sample_flow_with(index: u32, transform: FlowTransform, pos_ws: vec3<f32>, layers: u32) -> vec4<f32> {
    let flow = &flows[i]; 

    let relative_pos_ws = pos_ws - *flow.translation;
    let pos_fs = transform.local_from_world * relative_pos_ws;

    //TODO: remove negative area check in flow.scale cpu code. it's literally fine. HOWEVER make sure it's not zero.
    let in_bounds = all(abs(pos_fs + pos_fs) <= abs(*flow.scale));
    let layers_match = layers & *flow.layers != 0;
    if !(in_bounds && layers_match) { return vec4(0.0); }

    let uvw = pos_fs / max(*flow.scale, vec3(1.0e-12));
    var flow_vector = textureSampleLevel(fields[*flow.field_index], field_sampler, uvw, 0.0);
    flow_vector *= *flow.influence;

    // convert to world space
    flow_vector = vec4(transform.world_from_local * flow_vector.xyz, flow_vector.w);

    if *flow.flags & FLOW_FLAG_INHERIT_LINEAR_VELOCITY != 0 {
        flow_vector.xyz += *flow.linear_velocity;
    }

    if *flow.flags & FLOW_FLAG_INHERIT_ANGULAR_VELOCITY != 0 {
        flow_vector.xyz += cross(relative_pos_ws, *flow.angular_velocity);
    }

    return flow_vector;
}
