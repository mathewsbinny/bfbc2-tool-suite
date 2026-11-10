// shaderview.exe - draw meshes with compiled Dx10/11 shaders off screen (Direct3D 11) and dump the images.
// Driven by renderqa.py (the suite's render harness) and Project OPFOR's wake_preview.py / weapon_preview.py, which
// write the job file and turn the dumps into PNGs.
//
//   shaderview <job file> [-warp]          -warp: the software rasterizer (WARP) instead of the GPU
//
// Job file: one command per line ('#' comments). A frame: "frame <out.raw> <width> <height> r g b [a]" (clear
// colour, alpha 1 when not given), then draws; "end" renders it and writes <out.raw> (RGBA 32-bit float, row-major)
// and, when "depth 1" was given in the frame, <out.raw>.depth (32-bit float depth, 1 = nothing drawn).
// A draw:
//   vs <dxbc> | ps <dxbc>
//   el <semantic index> <DXGI format> <slot> <byte offset> <per instance 0|1>      (semantic name TEXCOORD)
//   vb <file> <byte offset> <stride>
//   ib <file> <byte offset> <16|32> <index count> <first index>
//   cb <vs|ps> <file>                    ($Globals contents, slot b0)
//   tex <slot> <2d|cube> <file>          (file: u32 w, u32 h, u32 faces, then RGBA float32 texels per face)
//   texres <slot> <file>                 (a BFBC2 PC texture resource as the game stores it: 92-byte header, then the
//                                         levels; uploaded in its own block format with its own mips, an sRGB view
//                                         when header flag bit 0 is set, as the game binds it; a cube map when the
//                                         header's type is 1)
//   samp <slot> <104 hex digits>         (a D3D11_SAMPLER_DESC, 52 bytes, for sampler register s<slot> - a shader
//                                         parameter block's sampler-state record after its u32 register)
//   draw
// "blend 1" draws the next draws alpha blended (source alpha, inverse source alpha; depth tested, not written) until
// "blend 0" (a frame starts opaque).
// "count 1" (until "count 0") also measures each draw, depth test off: the samples its pixel shader writes and the
// samples its triangles cover (no pixel shader); printed as "count <draw> <shaded> <covered>" - a pixel shader that
// discards everything (an alpha test reading the wrong channel) shows as shaded 0 of covered N.
// Defaults per draw (a "vs" line starts a draw): the shadow map (t0) is a 1x1 depth of 1 (lit) unless the draw binds
// its own with "tex 0" (the translator's differential test, xdiff.py); s0 a comparison
// sampler, s1 trilinear anisotropic wrap, other samplers unbound (D3D11's default: linear, clamp).
// Exit codes: 1 usage, 2 unreadable file, 3 a Direct3D call failed, 4 the device was lost (retry with -warp).
#include <d3d11.h>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>
#include <map>
#pragma comment(lib, "d3d11.lib")

static ID3D11Device *dev;
static ID3D11DeviceContext *ctx;

static std::vector<char> load(const std::string &p) {
    std::ifstream f(p, std::ios::binary);
    if (!f) { fprintf(stderr, "cannot read %s\n", p.c_str()); exit(2); }
    return std::vector<char>((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
}

static void check(HRESULT hr, const char *what) {
    if (FAILED(hr)) {
        fprintf(stderr, "%s failed: 0x%08lx\n", what, (unsigned long)hr);
        HRESULT reason = dev ? dev->GetDeviceRemovedReason() : S_OK;
        exit(hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET || FAILED(reason) ? 4 : 3);
    }
}

static ID3D11ShaderResourceView *texture(const std::string &path, bool cube) {
    auto d = load(path);
    uint32_t w = *(uint32_t *)&d[0], h = *(uint32_t *)&d[4], faces = *(uint32_t *)&d[8];
    D3D11_TEXTURE2D_DESC td = {};
    td.Width = w; td.Height = h; td.MipLevels = 0; td.ArraySize = faces;
    td.Format = DXGI_FORMAT_R32G32B32A32_FLOAT; td.SampleDesc.Count = 1; td.Usage = D3D11_USAGE_DEFAULT;
    td.BindFlags = D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET;
    td.MiscFlags = D3D11_RESOURCE_MISC_GENERATE_MIPS | (cube ? D3D11_RESOURCE_MISC_TEXTURECUBE : 0);
    ID3D11Texture2D *t;
    check(dev->CreateTexture2D(&td, nullptr, &t), "CreateTexture2D");
    t->GetDesc(&td);
    for (uint32_t f = 0; f < faces; f++)
        ctx->UpdateSubresource(t, D3D11CalcSubresource(0, f, td.MipLevels), nullptr, &d[12 + (size_t)f * w * h * 16],
                               w * 16, 0);
    D3D11_SHADER_RESOURCE_VIEW_DESC sd = {};
    sd.Format = td.Format;
    if (cube) { sd.ViewDimension = D3D11_SRV_DIMENSION_TEXTURECUBE; sd.TextureCube.MipLevels = (UINT)-1; }
    else { sd.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D; sd.Texture2D.MipLevels = (UINT)-1; }
    ID3D11ShaderResourceView *v;
    check(dev->CreateShaderResourceView(t, &sd, &v), "CreateShaderResourceView");
    ctx->GenerateMips(v);
    return v;
}

// A BFBC2 PC texture resource (the console converter's texture.py: 8 u32 - version, type, format, flags, width,
// height, depth, mip count - and 15 u32 level sizes; then the levels, face by face for a cube map).
static ID3D11ShaderResourceView *texture_res(const std::string &path) {
    auto d = load(path);
    if (d.size() < 92) { fprintf(stderr, "%s: not a texture resource\n", path.c_str()); exit(2); }
    const uint32_t *hd = (const uint32_t *)d.data();
    uint32_t type = hd[1], fmt = hd[2], flags = hd[3], w = hd[4], h = hd[5], mips = hd[7];
    const uint32_t *sizes = hd + 8;
    bool srgb = (flags & 1) != 0;
    DXGI_FORMAT f; UINT block = 0, bpp = 0;          // block: bytes per 4x4 block; bpp: bytes per texel
    switch (fmt) {
    case 0: case 18: f = srgb ? DXGI_FORMAT_BC1_UNORM_SRGB : DXGI_FORMAT_BC1_UNORM; block = 8; break;
    case 1: f = srgb ? DXGI_FORMAT_BC2_UNORM_SRGB : DXGI_FORMAT_BC2_UNORM; block = 16; break;
    case 2: f = srgb ? DXGI_FORMAT_BC3_UNORM_SRGB : DXGI_FORMAT_BC3_UNORM; block = 16; break;
    case 19: case 20: f = DXGI_FORMAT_BC5_UNORM; block = 16; break;
    case 13: f = DXGI_FORMAT_R16G16B16A16_FLOAT; bpp = 8; break;
    case 9: f = srgb ? DXGI_FORMAT_B8G8R8A8_UNORM_SRGB : DXGI_FORMAT_B8G8R8A8_UNORM; bpp = 4; break;
    case 10: f = DXGI_FORMAT_R8_UNORM; bpp = 1; break;
    default: fprintf(stderr, "%s: texture format %u not supported\n", path.c_str(), fmt); exit(2);
    }
    if (mips < 1 || mips > 15 || w == 0 || h == 0) {
        fprintf(stderr, "%s: %ux%u, %u mip levels\n", path.c_str(), w, h, mips); exit(2);
    }
    if (block && (w % 4 || h % 4)) {                     // D3D11 wants a block format's top level in whole blocks
        fprintf(stderr, "%s: %ux%u is not a multiple of 4\n", path.c_str(), w, h); exit(2);
    }
    bool cube = type == 1;
    UINT faces = cube ? 6 : 1;
    D3D11_TEXTURE2D_DESC td = {};
    td.Width = w; td.Height = h; td.MipLevels = mips; td.ArraySize = faces; td.Format = f;
    td.SampleDesc.Count = 1; td.Usage = D3D11_USAGE_IMMUTABLE; td.BindFlags = D3D11_BIND_SHADER_RESOURCE;
    td.MiscFlags = cube ? D3D11_RESOURCE_MISC_TEXTURECUBE : 0;
    std::vector<D3D11_SUBRESOURCE_DATA> init(faces * mips);
    size_t pos = 92;
    for (UINT s = 0; s < faces; s++)
        for (UINT m = 0; m < mips; m++) {
            UINT lw = w >> m ? w >> m : 1, lh = h >> m ? h >> m : 1;
            UINT pitch = block ? ((lw + 3) / 4) * block : lw * bpp;
            UINT rows = block ? (lh + 3) / 4 : lh;
            if (!block && sizes[m] >= (size_t)pitch * rows && rows) pitch = sizes[m] / rows;   // L8: rows padded
            if (pos + sizes[m] > d.size() || (size_t)pitch * rows > sizes[m]) {
                fprintf(stderr, "%s: level %u of face %u runs past the data\n", path.c_str(), m, s); exit(2);
            }
            init[s * mips + m] = {&d[pos], pitch, 0};
            pos += sizes[m];
        }
    ID3D11Texture2D *t;
    check(dev->CreateTexture2D(&td, init.data(), &t), "CreateTexture2D (resource)");
    D3D11_SHADER_RESOURCE_VIEW_DESC sd = {};
    sd.Format = f;
    if (cube) { sd.ViewDimension = D3D11_SRV_DIMENSION_TEXTURECUBE; sd.TextureCube.MipLevels = mips; }
    else { sd.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D; sd.Texture2D.MipLevels = mips; }
    ID3D11ShaderResourceView *v;
    check(dev->CreateShaderResourceView(t, &sd, &v), "CreateShaderResourceView (resource)");
    t->Release();
    return v;
}

static ID3D11Buffer *buffer(const void *data, size_t n, UINT bind) {
    D3D11_BUFFER_DESC bd = {};
    bd.ByteWidth = (UINT)((n + 15) & ~15); bd.Usage = D3D11_USAGE_DEFAULT; bd.BindFlags = bind;
    if (bd.ByteWidth == 0) bd.ByteWidth = 16;
    std::vector<char> pad(bd.ByteWidth, 0);
    if (n) memcpy(pad.data(), data, n);
    D3D11_SUBRESOURCE_DATA s = {pad.data(), 0, 0};
    ID3D11Buffer *b;
    check(dev->CreateBuffer(&bd, &s, &b), "CreateBuffer");
    return b;
}

static void dump(ID3D11Texture2D *src, const std::string &out, UINT W, UINT H, UINT bpp) {
    D3D11_TEXTURE2D_DESC td; src->GetDesc(&td);
    td.Usage = D3D11_USAGE_STAGING; td.BindFlags = 0; td.CPUAccessFlags = D3D11_CPU_ACCESS_READ; td.MiscFlags = 0;
    ID3D11Texture2D *st; check(dev->CreateTexture2D(&td, nullptr, &st), "staging");
    ctx->CopyResource(st, src);
    D3D11_MAPPED_SUBRESOURCE m; check(ctx->Map(st, 0, D3D11_MAP_READ, 0, &m), "Map");
    FILE *f = fopen(out.c_str(), "wb");
    if (!f) { fprintf(stderr, "cannot write %s\n", out.c_str()); exit(2); }
    for (UINT y = 0; y < H; y++) fwrite((char *)m.pData + y * m.RowPitch, bpp, W, f);
    fclose(f);
    ctx->Unmap(st, 0); st->Release();
}

static uint64_t occlusion(ID3D11Query *q) {
    UINT64 n = 0;
    HRESULT hr;
    while ((hr = ctx->GetData(q, &n, sizeof(n), 0)) == S_FALSE) Sleep(0);
    check(hr, "GetData (occlusion)");
    return n;
}

static bool hexdesc(const std::string &hex, D3D11_SAMPLER_DESC &out) {
    if (hex.size() != 2 * sizeof(D3D11_SAMPLER_DESC)) return false;
    unsigned char *b = (unsigned char *)&out;
    for (size_t i = 0; i < sizeof(D3D11_SAMPLER_DESC); i++) {
        unsigned v; if (sscanf(hex.c_str() + 2 * i, "%2x", &v) != 1) return false;
        b[i] = (unsigned char)v;
    }
    return true;
}

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: shaderview <job file> [-warp]\n"); return 1; }
    bool warp = argc > 2 && std::string(argv[2]) == "-warp";
    D3D_FEATURE_LEVEL fl = D3D_FEATURE_LEVEL_11_0;
    if (warp || FAILED(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, &fl, 1, D3D11_SDK_VERSION,
                                         &dev, nullptr, &ctx)))
        check(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr, 0, &fl, 1, D3D11_SDK_VERSION, &dev, nullptr,
                                &ctx), "D3D11CreateDevice");
    // fixed state: samplers, shadow map, rasterizer, depth
    D3D11_SAMPLER_DESC sc = {};
    sc.Filter = D3D11_FILTER_COMPARISON_MIN_MAG_LINEAR_MIP_POINT; sc.ComparisonFunc = D3D11_COMPARISON_LESS_EQUAL;
    sc.AddressU = sc.AddressV = sc.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP; sc.MaxLOD = D3D11_FLOAT32_MAX;
    D3D11_SAMPLER_DESC sl = {};
    sl.Filter = D3D11_FILTER_ANISOTROPIC; sl.MaxAnisotropy = 8;
    sl.AddressU = sl.AddressV = sl.AddressW = D3D11_TEXTURE_ADDRESS_WRAP; sl.MaxLOD = D3D11_FLOAT32_MAX;
    ID3D11SamplerState *samp[2];
    check(dev->CreateSamplerState(&sc, &samp[0]), "sampler0");
    check(dev->CreateSamplerState(&sl, &samp[1]), "sampler1");
    float one[4] = {1, 1, 1, 1};
    D3D11_TEXTURE2D_DESC sd = {1, 1, 1, 1, DXGI_FORMAT_R32_FLOAT, {1, 0}, D3D11_USAGE_DEFAULT, D3D11_BIND_SHADER_RESOURCE};
    D3D11_SUBRESOURCE_DATA sdat = {one, 4, 0};
    ID3D11Texture2D *shadowTex; ID3D11ShaderResourceView *shadow;
    check(dev->CreateTexture2D(&sd, &sdat, &shadowTex), "shadow map");
    check(dev->CreateShaderResourceView(shadowTex, nullptr, &shadow), "shadow view");
    D3D11_RASTERIZER_DESC rd = {}; rd.FillMode = D3D11_FILL_SOLID; rd.CullMode = D3D11_CULL_NONE; rd.DepthClipEnable = TRUE;
    ID3D11RasterizerState *rs; check(dev->CreateRasterizerState(&rd, &rs), "rasterizer");
    D3D11_DEPTH_STENCIL_DESC dd = {}; dd.DepthEnable = TRUE; dd.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ALL;
    dd.DepthFunc = D3D11_COMPARISON_LESS;
    ID3D11DepthStencilState *ds; check(dev->CreateDepthStencilState(&dd, &ds), "depth state");
    dd.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ZERO;
    ID3D11DepthStencilState *dsNoWrite; check(dev->CreateDepthStencilState(&dd, &dsNoWrite), "depth state (blend)");
    // counting: depth test on (a draw without a pixel shader is only rasterized for its depth) but always passing,
    // nothing written
    D3D11_DEPTH_STENCIL_DESC dc = {}; dc.DepthEnable = TRUE; dc.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ZERO;
    dc.DepthFunc = D3D11_COMPARISON_ALWAYS;
    ID3D11DepthStencilState *dsCount; check(dev->CreateDepthStencilState(&dc, &dsCount), "depth state (count)");
    D3D11_BLEND_DESC bdesc = {};
    bdesc.RenderTarget[0].BlendEnable = TRUE;
    bdesc.RenderTarget[0].SrcBlend = D3D11_BLEND_SRC_ALPHA; bdesc.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
    bdesc.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
    bdesc.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ONE; bdesc.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA;
    bdesc.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
    bdesc.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    ID3D11BlendState *alphaBlend; check(dev->CreateBlendState(&bdesc, &alphaBlend), "blend state");
    D3D11_BLEND_DESC nodesc = {};
    nodesc.RenderTarget[0].RenderTargetWriteMask = 0;
    ID3D11BlendState *noWrite; check(dev->CreateBlendState(&nodesc, &noWrite), "blend state (count)");
    D3D11_QUERY_DESC qd = {D3D11_QUERY_OCCLUSION, 0};
    bool blending = false, counting = false, wantDepth = false;

    std::ifstream job(argv[1]);
    if (!job) { fprintf(stderr, "cannot read %s\n", argv[1]); return 2; }
    std::string line, out;
    UINT W = 0, H = 0;
    ID3D11Texture2D *rt = nullptr, *depth = nullptr;
    ID3D11RenderTargetView *rtv = nullptr; ID3D11DepthStencilView *dsv = nullptr;
    std::vector<char> vsc, psc, vbd, ibd;
    std::vector<D3D11_INPUT_ELEMENT_DESC> els;
    UINT vbOff = 0, stride = 0, ibOff = 0, ibBits = 16, count = 0, first = 0;
    std::vector<char> cbv, cbp;
    std::map<int, ID3D11ShaderResourceView *> texs;
    std::map<int, ID3D11SamplerState *> samps;
    std::map<std::string, ID3D11ShaderResourceView *> texCache;
    std::map<std::string, ID3D11SamplerState *> sampCache;
    int draws = 0;
    while (std::getline(job, line)) {
        std::istringstream s(line);
        std::string cmd; s >> cmd;
        if (cmd.empty() || cmd[0] == '#') continue;
        if (cmd == "frame") {
            float c[4] = {0, 0, 0, 1};
            s >> out >> W >> H >> c[0] >> c[1] >> c[2];
            if (!(s >> c[3])) c[3] = 1;
            D3D11_TEXTURE2D_DESC td = {W, H, 1, 1, DXGI_FORMAT_R32G32B32A32_FLOAT, {1, 0}, D3D11_USAGE_DEFAULT,
                                       D3D11_BIND_RENDER_TARGET};
            check(dev->CreateTexture2D(&td, nullptr, &rt), "render target");
            check(dev->CreateRenderTargetView(rt, nullptr, &rtv), "rtv");
            td.Format = DXGI_FORMAT_D32_FLOAT; td.BindFlags = D3D11_BIND_DEPTH_STENCIL;
            check(dev->CreateTexture2D(&td, nullptr, &depth), "depth");
            check(dev->CreateDepthStencilView(depth, nullptr, &dsv), "dsv");
            ctx->ClearRenderTargetView(rtv, c);
            ctx->ClearDepthStencilView(dsv, D3D11_CLEAR_DEPTH, 1.0f, 0);
            blending = false; wantDepth = false;
        } else if (cmd == "blend") { int on = 0; s >> on; blending = on != 0; }
        else if (cmd == "count") { int on = 0; s >> on; counting = on != 0; }
        else if (cmd == "depth") { int on = 0; s >> on; wantDepth = on != 0; }
        else if (cmd == "vs") { std::string p; s >> p; vsc = load(p); els.clear(); texs.clear(); samps.clear(); }
        else if (cmd == "ps") { std::string p; s >> p; psc = load(p); }
        else if (cmd == "el") {
            UINT si, fmt, slot, off, inst; s >> si >> fmt >> slot >> off >> inst;
            els.push_back({"TEXCOORD", si, (DXGI_FORMAT)fmt, slot, off,
                           inst ? D3D11_INPUT_PER_INSTANCE_DATA : D3D11_INPUT_PER_VERTEX_DATA, inst});
        } else if (cmd == "vb") { std::string p; s >> p >> vbOff >> stride; vbd = load(p); }
        else if (cmd == "ib") { std::string p; s >> p >> ibOff >> ibBits >> count >> first; ibd = load(p); }
        else if (cmd == "cb") { std::string w, p; s >> w >> p; (w == "vs" ? cbv : cbp) = load(p); }
        else if (cmd == "tex" || cmd == "texres") {
            int slot; std::string kind, p;
            s >> slot;
            if (cmd == "tex") s >> kind;
            s >> p;
            std::string key = cmd + kind + p;
            if (!texCache.count(key)) texCache[key] = cmd == "tex" ? texture(p, kind == "cube") : texture_res(p);
            texs[slot] = texCache[key];
        } else if (cmd == "samp") {
            int slot; std::string hex; s >> slot >> hex;
            if (!sampCache.count(hex)) {
                D3D11_SAMPLER_DESC desc;
                if (!hexdesc(hex, desc)) { fprintf(stderr, "samp: bad sampler state %s\n", hex.c_str()); return 1; }
                check(dev->CreateSamplerState(&desc, &sampCache[hex]), "CreateSamplerState");
            }
            samps[slot] = sampCache[hex];
        } else if (cmd == "draw") {
            ID3D11VertexShader *vs; ID3D11PixelShader *ps; ID3D11InputLayout *il;
            check(dev->CreateVertexShader(vsc.data(), vsc.size(), nullptr, &vs), "CreateVertexShader");
            check(dev->CreatePixelShader(psc.data(), psc.size(), nullptr, &ps), "CreatePixelShader");
            check(dev->CreateInputLayout(els.data(), (UINT)els.size(), vsc.data(), vsc.size(), &il), "CreateInputLayout");
            ID3D11Buffer *vb = buffer(vbd.data(), vbd.size(), D3D11_BIND_VERTEX_BUFFER);
            ID3D11Buffer *ib = buffer(ibd.data(), ibd.size(), D3D11_BIND_INDEX_BUFFER);
            ID3D11Buffer *cv = cbv.empty() ? nullptr : buffer(cbv.data(), cbv.size(), D3D11_BIND_CONSTANT_BUFFER);
            ID3D11Buffer *cp = cbp.empty() ? nullptr : buffer(cbp.data(), cbp.size(), D3D11_BIND_CONSTANT_BUFFER);
            ctx->OMSetRenderTargets(1, &rtv, dsv);
            D3D11_VIEWPORT vp = {0, 0, (float)W, (float)H, 0, 1};
            ctx->RSSetViewports(1, &vp); ctx->RSSetState(rs);
            ctx->IASetInputLayout(il);
            ctx->IASetVertexBuffers(0, 1, &vb, &stride, &vbOff);
            ctx->IASetIndexBuffer(ib, ibBits == 32 ? DXGI_FORMAT_R32_UINT : DXGI_FORMAT_R16_UINT, ibOff);
            ctx->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx->VSSetShader(vs, nullptr, 0); ctx->PSSetShader(ps, nullptr, 0);
            ctx->VSSetConstantBuffers(0, 1, &cv); ctx->PSSetConstantBuffers(0, 1, &cp);
            ID3D11SamplerState *ss[16] = {samp[0], samp[1]};
            for (auto &kv : samps) if (kv.first >= 0 && kv.first < 16) ss[kv.first] = kv.second;
            ctx->PSSetSamplers(0, 16, ss); ctx->VSSetSamplers(0, 16, ss);
            ID3D11ShaderResourceView *srv[32] = {shadow};
            for (auto &kv : texs) if (kv.first >= 0 && kv.first < 32) srv[kv.first] = kv.second;   // tex 0 replaces the shadow map
            ctx->PSSetShaderResources(0, 32, srv);
            if (counting) {
                ID3D11Query *q[2];
                check(dev->CreateQuery(&qd, &q[0]), "CreateQuery");
                check(dev->CreateQuery(&qd, &q[1]), "CreateQuery");
                ctx->OMSetDepthStencilState(dsCount, 0);
                ctx->OMSetBlendState(noWrite, nullptr, 0xffffffff);
                ctx->Begin(q[0]); ctx->DrawIndexed(count, first, 0); ctx->End(q[0]);
                ctx->PSSetShader(nullptr, nullptr, 0);
                ctx->Begin(q[1]); ctx->DrawIndexed(count, first, 0); ctx->End(q[1]);
                ctx->PSSetShader(ps, nullptr, 0);
                printf("count %d %llu %llu\n", draws, (unsigned long long)occlusion(q[0]),
                       (unsigned long long)occlusion(q[1]));
                q[0]->Release(); q[1]->Release();
            }
            ctx->OMSetDepthStencilState(blending ? dsNoWrite : ds, 0);
            ctx->OMSetBlendState(blending ? alphaBlend : nullptr, nullptr, 0xffffffff);
            ctx->DrawIndexed(count, first, 0);
            draws++;
            vs->Release(); ps->Release(); il->Release(); vb->Release(); ib->Release();
            if (cv) cv->Release();
            if (cp) cp->Release();
        } else if (cmd == "end") {
            dump(rt, out, W, H, 16);
            if (wantDepth) dump(depth, out + ".depth", W, H, 4);
            rtv->Release(); rt->Release(); dsv->Release(); depth->Release();
            printf("%s: %d draws\n", out.c_str(), draws);
            fflush(stdout);
            draws = 0;
        } else {
            fprintf(stderr, "unknown command %s\n", cmd.c_str());
            return 1;
        }
    }
    return 0;
}
