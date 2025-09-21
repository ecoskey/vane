#import vane::flow::{
    Flow, FlowTransforms, sample_flow_with,
    flows, fields, field_sampler,
}

@group(#{VANE_BIND_GROUP}) @binding(#{VANE_BINDINGS_START} + 0) var<storage, read> sample_positions: array<vec3<f32>>;
@group(#{VANE_BIND_GROUP}) @binding(#{VANE_BINDINGS_START} + 1) var<storage, write> sampled_vectors: array<vec4<f32>>;
