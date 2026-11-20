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

// flat shading from the face's own normal (screen-space derivatives: no normals in the data). The colour's alpha
// says how: 1 the scene's light (terrain, objects: 0.40 - 1.0), 0 an overlay's even light (team-owned and gameplay
// objects: 0.88 - 1.0, so their view-* colour reads on the terrain from every side; theme.rs's contrast tests use
// these numbers)
fn shade_of(world: vec3<f32>, scene_lit: f32) -> f32 {
    let c = cross(dpdx(world), dpdy(world));
    let l = length(c);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    if (l > 1e-12) {
        n = c / l;
    }
    let d = abs(dot(n, normalize(g.light.xyz)));
    let lit = 0.40 + 0.45 * d + 0.15 * abs(n.y);
    let even = 0.88 + 0.12 * max(d, abs(n.y));
    var shade = mix(even, lit, scene_lit);
    // the unlit view mode: flat colours
    if (g.light.w > 0.5) {
        shade = 1.0;
    }
    return shade;
}

@fragment
fn fs_mesh(i: MeshOut) -> SceneOut {
    let shade = shade_of(i.world, i.color.a);
    var o: SceneOut;
    o.color = vec4<f32>(i.color.rgb * shade, 1.0);
    o.id = i.id;
    return o;
}

// ------------------------------------------------------------------------------------------- render meshes
// round SV1: the objects' render meshes; each vertex names its part's material - the mean colour of its diffuse
// texture (the tint), its tile in the atlas and its alpha test

struct Material {
    // rgb, and 1 when the part has a tint
    tint: vec4<f32>,
    // the tile's corner and size in the atlas (texels); size 0: no texture
    tile: vec4<f32>,
    // x: the alpha-test reference (0 - 1), negative for none
    test: vec4<f32>,
};
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var<storage, read> materials: array<Material>;

struct VisualOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) id: u32,
    @location(3) uv: vec2<f32>,
    @location(4) @interpolate(flat) material: u32,
};

@vertex
fn vs_visual(
    @location(0) p: vec3<f32>,
    @location(7) uv: vec2<f32>,
    @location(8) material: u32,
    @location(1) r0: vec4<f32>,
    @location(2) r1: vec4<f32>,
    @location(3) r2: vec4<f32>,
    @location(4) r3: vec4<f32>,
    @location(5) color: vec4<f32>,
    @location(6) id: u32,
) -> VisualOut {
    let w = r0.xyz * p.x + r1.xyz * p.y + r2.xyz * p.z + r3.xyz;
    var o: VisualOut;
    o.pos = g.view_proj * vec4<f32>(w, 1.0);
    o.world = w;
    o.color = color;
    o.id = id;
    o.uv = uv;
    o.material = material;
    return o;
}

// the texel of the part's texture under the coordinates (wrapped, as the game's samplers wrap)
fn texel(m: Material, uv: vec2<f32>) -> vec4<f32> {
    let size = m.tile.zw;
    let f = fract(uv);
    let t = min(floor(f * size), size - vec2<f32>(1.0, 1.0));
    return textureLoad(atlas, vec2<i32>(m.tile.xy + t), 0);
}

// whether an alpha-tested part has a hole here
fn cut(m: Material, uv: vec2<f32>) -> bool {
    if (m.test.x < 0.0 || m.tile.z <= 0.0) {
        return false;
    }
    return texel(m, uv).a < m.test.x;
}

@fragment
fn fs_visual(i: VisualOut) -> SceneOut {
    // the shade first: derivatives in uniform control flow
    let shade = shade_of(i.world, i.color.a);
    let m = materials[i.material];
    if (cut(m, i.uv)) {
        discard;
    }
    // scene-lit objects take their texture's colour, muted (DICE's detail layers are not true colours) and brought to
    // one brightness, a little of the view's own colour in it (overlays and the modes' tints keep the instance's
    // colour: its alpha 0)
    var base = i.color.rgb;
    if (i.color.a > 0.5 && m.tint.a > 0.5) {
        let t = m.tint.rgb;
        let lum = dot(t, vec3<f32>(0.2126, 0.7152, 0.0722));
        let c = mix(vec3<f32>(lum), t, 0.65) * clamp(0.6 / max(lum, 0.05), 1.0, 2.4);
        base = mix(min(c, vec3<f32>(1.0)), i.color.rgb, 0.2);
    }
    var o: SceneOut;
    o.color = vec4<f32>(base * shade, 1.0);
    o.id = i.id;
    return o;
}

@fragment
fn fs_mask_visual(i: VisualOut) -> @location(0) vec4<f32> {
    if (cut(materials[i.material], i.uv)) {
        discard;
    }
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

// the wireframe view mode: a mesh's edges in its instance's colour
@fragment
fn fs_wire(i: MeshOut) -> SceneOut {
    var o: SceneOut;
    o.color = vec4<f32>(i.color.rgb, 1.0);
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
