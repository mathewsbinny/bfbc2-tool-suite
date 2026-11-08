// shaderview.exe - draw meshes with compiled Dx10/11 shaders off screen (Direct3D 11) and dump the images.
// Driven by shaderpreview.py, which writes the job file and turns the dumps into PNGs.
//
// Job file: one command per line ('#' comments). A frame: "frame <out.raw> <width> <height> r g b" (clear colour),
// then draws; "end" renders it and writes <out.raw> (RGBA 32-bit float, row-major).
// A draw:
//   vs <dxbc> | ps <dxbc>
//   el <semantic index> <DXGI format> <slot> <byte offset> <per instance 0|1>      (semantic name TEXCOORD)
//   vb <file> <byte offset> <stride>
//   ib <file> <byte offset> <16|32> <index count> <first index>
//   cb <vs|ps> <file>                    ($Globals contents, slot b0)
//   tex <slot> <2d|cube> <file>          (file: u32 w, u32 h, u32 faces, then RGBA float32 texels per face)
//   draw
// "blend 1" draws the next draws alpha blended (source alpha, inverse source alpha; depth tested, not written) until
// "blend 0" (a frame starts opaque).
// The shadow map (t0) is a 1x1 depth of 1 (lit); s0 a comparison sampler, s1 trilinear anisotropic wrap.
#include <d3d11.h>
#include <cstdio>
#include <cstdint>
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
    if (FAILED(hr)) { fprintf(stderr, "%s failed: 0x%08lx\n", what, (unsigned long)hr); exit(3); }
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

static ID3D11Buffer *buffer(const void *data, size_t n, UINT bind) {
    D3D11_BUFFER_DESC bd = {};
    bd.ByteWidth = (UINT)((n + 15) & ~15); bd.Usage = D3D11_USAGE_DEFAULT; bd.BindFlags = bind;
    std::vector<char> pad(bd.ByteWidth, 0);
    memcpy(pad.data(), data, n);
    D3D11_SUBRESOURCE_DATA s = {pad.data(), 0, 0};
    ID3D11Buffer *b;
    check(dev->CreateBuffer(&bd, &s, &b), "CreateBuffer");
    return b;
}

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: shaderview <job file>\n"); return 1; }
    D3D_FEATURE_LEVEL fl = D3D_FEATURE_LEVEL_11_0;
    if (FAILED(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, &fl, 1, D3D11_SDK_VERSION, &dev,
                                 nullptr, &ctx)))
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
    D3D11_BLEND_DESC bdesc = {};
    bdesc.RenderTarget[0].BlendEnable = TRUE;
    bdesc.RenderTarget[0].SrcBlend = D3D11_BLEND_SRC_ALPHA; bdesc.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
    bdesc.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
    bdesc.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ONE; bdesc.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA;
    bdesc.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
    bdesc.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    ID3D11BlendState *alphaBlend; check(dev->CreateBlendState(&bdesc, &alphaBlend), "blend state");
    bool blending = false;

    std::ifstream job(argv[1]);
    std::string line, out;
    UINT W = 0, H = 0;
    ID3D11Texture2D *rt = nullptr, *depth = nullptr;
    ID3D11RenderTargetView *rtv = nullptr; ID3D11DepthStencilView *dsv = nullptr;
    std::vector<char> vsc, psc, vbd, ibd;
    std::vector<D3D11_INPUT_ELEMENT_DESC> els;
    UINT vbOff = 0, stride = 0, ibOff = 0, ibBits = 16, count = 0, first = 0;
    std::vector<char> cbv, cbp;
    std::map<int, ID3D11ShaderResourceView *> texs;
    std::map<std::string, ID3D11ShaderResourceView *> texCache;
    int draws = 0;
    while (std::getline(job, line)) {
        std::istringstream s(line);
        std::string cmd; s >> cmd;
        if (cmd.empty() || cmd[0] == '#') continue;
        if (cmd == "frame") {
            float c[4] = {0, 0, 0, 1};
            s >> out >> W >> H >> c[0] >> c[1] >> c[2];
            D3D11_TEXTURE2D_DESC td = {W, H, 1, 1, DXGI_FORMAT_R32G32B32A32_FLOAT, {1, 0}, D3D11_USAGE_DEFAULT,
                                       D3D11_BIND_RENDER_TARGET};
            check(dev->CreateTexture2D(&td, nullptr, &rt), "render target");
            check(dev->CreateRenderTargetView(rt, nullptr, &rtv), "rtv");
            td.Format = DXGI_FORMAT_D32_FLOAT; td.BindFlags = D3D11_BIND_DEPTH_STENCIL;
            check(dev->CreateTexture2D(&td, nullptr, &depth), "depth");
            check(dev->CreateDepthStencilView(depth, nullptr, &dsv), "dsv");
            ctx->ClearRenderTargetView(rtv, c);
            ctx->ClearDepthStencilView(dsv, D3D11_CLEAR_DEPTH, 1.0f, 0);
            blending = false;
        } else if (cmd == "blend") { int on = 0; s >> on; blending = on != 0; }
        else if (cmd == "vs") { std::string p; s >> p; vsc = load(p); els.clear(); texs.clear(); }
        else if (cmd == "ps") { std::string p; s >> p; psc = load(p); }
        else if (cmd == "el") {
            UINT si, fmt, slot, off, inst; s >> si >> fmt >> slot >> off >> inst;
            els.push_back({"TEXCOORD", si, (DXGI_FORMAT)fmt, slot, off,
                           inst ? D3D11_INPUT_PER_INSTANCE_DATA : D3D11_INPUT_PER_VERTEX_DATA, inst});
        } else if (cmd == "vb") { std::string p; s >> p >> vbOff >> stride; vbd = load(p); }
        else if (cmd == "ib") { std::string p; s >> p >> ibOff >> ibBits >> count >> first; ibd = load(p); }
        else if (cmd == "cb") { std::string w, p; s >> w >> p; (w == "vs" ? cbv : cbp) = load(p); }
        else if (cmd == "tex") {
            int slot; std::string kind, p; s >> slot >> kind >> p;
            std::string key = kind + p;
            if (!texCache.count(key)) texCache[key] = texture(p, kind == "cube");
            texs[slot] = texCache[key];
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
            ctx->OMSetDepthStencilState(blending ? dsNoWrite : ds, 0);
            ctx->OMSetBlendState(blending ? alphaBlend : nullptr, nullptr, 0xffffffff);
            ctx->IASetInputLayout(il);
            ctx->IASetVertexBuffers(0, 1, &vb, &stride, &vbOff);
            ctx->IASetIndexBuffer(ib, ibBits == 32 ? DXGI_FORMAT_R32_UINT : DXGI_FORMAT_R16_UINT, ibOff);
            ctx->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx->VSSetShader(vs, nullptr, 0); ctx->PSSetShader(ps, nullptr, 0);
            ctx->VSSetConstantBuffers(0, 1, &cv); ctx->PSSetConstantBuffers(0, 1, &cp);
            ctx->PSSetSamplers(0, 2, samp); ctx->VSSetSamplers(0, 2, samp);
            ID3D11ShaderResourceView *srv[16] = {shadow};
            for (auto &kv : texs) if (kv.first > 0 && kv.first < 16) srv[kv.first] = kv.second;
            ctx->PSSetShaderResources(0, 16, srv);
            ctx->DrawIndexed(count, first, 0);
            draws++;
            vs->Release(); ps->Release(); il->Release(); vb->Release(); ib->Release();
            if (cv) cv->Release();
            if (cp) cp->Release();
        } else if (cmd == "end") {
            D3D11_TEXTURE2D_DESC td; rt->GetDesc(&td);
            td.Usage = D3D11_USAGE_STAGING; td.BindFlags = 0; td.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
            ID3D11Texture2D *st; check(dev->CreateTexture2D(&td, nullptr, &st), "staging");
            ctx->CopyResource(st, rt);
            D3D11_MAPPED_SUBRESOURCE m; check(ctx->Map(st, 0, D3D11_MAP_READ, 0, &m), "Map");
            FILE *f = fopen(out.c_str(), "wb");
            for (UINT y = 0; y < H; y++) fwrite((char *)m.pData + y * m.RowPitch, 16, W, f);
            fclose(f);
            ctx->Unmap(st, 0); st->Release();
            rtv->Release(); rt->Release(); dsv->Release(); depth->Release();
            printf("%s: %d draws\n", out.c_str(), draws);
            draws = 0;
        } else {
            fprintf(stderr, "unknown command %s\n", cmd.c_str());
            return 1;
        }
    }
    return 0;
}
