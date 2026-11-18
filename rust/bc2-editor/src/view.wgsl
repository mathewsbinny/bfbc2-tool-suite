// The map view's shaders (render.rs): the scene (meshes placed by instance transforms, lines) into a colour and an
// object-id target, the selected object into a mask, and the composite that draws the selection outline.

struct Globals {
    view_proj: mat4x4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct MeshOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) id: u32,
};

struct SceneOut {
    @location(0) color: vec4<f32>,
    @location(1) id: u32,
};

// a mesh vertex in its blueprint's space, placed by the instance's rows (right, up, forward, translation)
@vertex
fn vs_mesh(
    @location(0) p: vec3<f32>,
    @location(1) r0: vec4<f32>,
    @location(2) r1: vec4<f32>,
    @location(3) r2: vec4<f32>,
    @location(4) r3: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(6) id: u32,
) -> MeshOut {
    let w = r0.xyz * p.x + r1.xyz * p.y + r2.xyz * p.z + r3.xyz;
    var o: MeshOut;
    o.pos = g.view_proj * vec4<f32>(w, 1.0);
    o.world = w;
    o.color = color;
    o.id = id;
    return o;
}

// flat shading from the face's own normal (screen-space derivatives: no normals in the data)
@fragment
fn fs_mesh(i: MeshOut) -> SceneOut {
    let c = cross(dpdx(i.world), dpdy(i.world));
    let l = length(c);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    if (l > 1e-12) {
        n = c / l;
    }
    let d = abs(dot(n, normalize(g.light.xyz)));
    let shade = 0.40 + 0.45 * d + 0.15 * abs(n.y);
    var o: SceneOut;
    o.color = vec4<f32>(i.color.rgb * shade, 1.0);
    o.id = i.id;
    return o;
}

struct LineOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) id: u32,
};

@vertex
fn vs_line(@location(0) p: vec3<f32>, @location(1) color: vec4<f32>, @location(2) id: u32) -> LineOut {
    var o: LineOut;
    o.pos = g.view_proj * vec4<f32>(p, 1.0);
    o.color = color;
    o.id = id;
    return o;
}

@fragment
fn fs_line(i: LineOut) -> SceneOut {
    var o: SceneOut;
    o.color = vec4<f32>(i.color.rgb, 1.0);
    o.id = i.id;
    return o;
}

@fragment
fn fs_mask_mesh(i: MeshOut) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

@fragment
fn fs_mask_line(i: LineOut) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

// ------------------------------------------------------------------------------------------------ composite

struct Composite {
    selection: vec4<f32>,
    // x: outline width in pixels
    params: vec4<f32>,
};
@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var mask_tex: texture_2d<f32>;
@group(0) @binding(2) var<uniform> comp: Composite;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

// the selection outline: pixels outside the selected object's mask within the width of it
@fragment
fn fs_composite(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy);
    let dims = vec2<i32>(textureDimensions(mask_tex));
    let base = textureLoad(scene_tex, p, 0);
    if (textureLoad(mask_tex, p, 0).r > 0.5) {
        return base;
    }
    let r = i32(comp.params.x);
    for (var dy = -r; dy <= r; dy = dy + 1) {
        for (var dx = -r; dx <= r; dx = dx + 1) {
            let q = clamp(p + vec2<i32>(dx, dy), vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
            if (textureLoad(mask_tex, q, 0).r > 0.5) {
                return vec4<f32>(comp.selection.rgb, 1.0);
            }
        }
    }
    return base;
}
