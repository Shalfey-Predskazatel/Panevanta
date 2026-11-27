#include <windows.h>
#include <d3d11_4.h>
#include <d3dcompiler.h>
#include <dxgi1_6.h>
#include <wrl/client.h>
#include <cuda.h>
#include <cudaD3D11.h>
#include <nvVideoEffects.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <condition_variable>
#include <cstdio>
#include <deque>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

using Microsoft::WRL::ComPtr;
using namespace std::chrono_literals;
extern "C" NvCVImage* PaneAllocImages(unsigned count);
extern "C" NvCVImage* PaneGetImage(NvCVImage* images, unsigned index);

static void hr(HRESULT result, const char* op) {
    if (FAILED(result)) throw std::runtime_error(std::string(op) + ": HRESULT " + std::to_string(result));
}
static void cv(NvCV_Status result, const char* op) {
    if (result != NVCV_SUCCESS) throw std::runtime_error(std::string(op) + ": NVIDIA VFX " + std::to_string(result));
}
static void cu(CUresult result, const char* op) {
    if (result != CUDA_SUCCESS) throw std::runtime_error(std::string(op) + ": CUDA " + std::to_string(result));
}
template<class T> static T function(HMODULE module, const char* name) {
    auto fn = GetProcAddress(module, name);
    if (!fn) throw std::runtime_error(std::string("Missing function: ") + name);
    return reinterpret_cast<T>(fn);
}
static int64_t qpc() { LARGE_INTEGER t; QueryPerformanceCounter(&t); return t.QuadPart; }

struct Settings {
    int width, height, fps, factor, quality, sr, hdr, displayHdr, peak;
    float white, scale;
};

#define STR0(value) #value
#define STR(value) STR0(value)
#define CUDA_FUNCTIONS(F) F(cuInit); F(cuD3D11GetDevice); F(cuDevicePrimaryCtxRetain); F(cuDevicePrimaryCtxRelease); \
    F(cuCtxSetCurrent); F(cuStreamCreate); F(cuStreamDestroy); F(cuStreamSynchronize); \
    F(cuGraphicsD3D11RegisterResource); F(cuGraphicsUnregisterResource); F(cuGraphicsMapResources); \
    F(cuGraphicsUnmapResources); F(cuGraphicsResourceGetMappedPointer); F(cuMemAlloc); F(cuMemFree); F(cuMemcpyDtoDAsync)
#define VFX_FUNCTIONS(F) F(NvVFX_CreateEffect); F(NvVFX_DestroyEffect); F(NvVFX_SetU32); F(NvVFX_SetImage); \
    F(NvVFX_SetCudaStream); F(NvVFX_Load); F(NvVFX_Run)
#define FIELD(fn) decltype(&fn) fn##P{}
#define CALL_CU(fn, ...) cu(fn##P(__VA_ARGS__), #fn)
#define CALL_VFX(fn, ...) cv(fn##P(__VA_ARGS__), #fn)

struct Compute {
    CUDA_FUNCTIONS(FIELD);
    VFX_FUNCTIONS(FIELD);
    FIELD(NvCVImage_Init);
    HMODULE driver{}, vfx{}, image{};
    DLL_DIRECTORY_COOKIE coreDir{}, featureDir{};
    CUdevice gpu{};
    CUcontext context{};
    CUstream stream{};
    CUdeviceptr work{};
    NvVFX_Handle effect{};
    NvCVImage* images{};
    std::vector<CUgraphicsResource> registrations;

    void Setup(IDXGIAdapter* adapter, const Settings& s, int width, int height, const wchar_t* sdkName) {
        const std::filesystem::path sdk(sdkName);
        auto bin = sdk / L"bin";
        auto feature = sdk / L"features/nvvfxvideoframegeneration/bin";
        if (!std::filesystem::is_regular_file(bin / L"NVVideoEffects.dll"))
            throw std::runtime_error("Missing NVIDIA-VideoFX/bin/NVVideoEffects.dll");
        SetEnvironmentVariableW(L"NV_VIDEO_EFFECTS_PATH", sdk.c_str());
        coreDir = AddDllDirectory(bin.c_str());
        featureDir = AddDllDirectory(feature.c_str());
        const auto loadFlags = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;
        driver = LoadLibraryExW(L"nvcuda.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
        image = LoadLibraryExW((bin / L"NVCVImage.dll").c_str(), nullptr, loadFlags);
        vfx = LoadLibraryExW((bin / L"NVVideoEffects.dll").c_str(), nullptr, loadFlags);
        if (!driver || !image || !vfx)
            throw std::runtime_error("Could not load NVIDIA CUDA / Video Effects SDK");
#define GET_CU(fn) fn##P = function<decltype(fn##P)>(driver, STR(fn))
#define GET_VFX(fn) fn##P = function<decltype(fn##P)>(vfx, #fn)
        CUDA_FUNCTIONS(GET_CU);
        VFX_FUNCTIONS(GET_VFX);
#undef GET_CU
#undef GET_VFX
        NvCVImage_InitP = function<decltype(NvCVImage_InitP)>(image, "NvCVImage_Init");
        CALL_CU(cuInit, 0);
        CALL_CU(cuD3D11GetDevice, &gpu, adapter);
        CALL_CU(cuDevicePrimaryCtxRetain, &context, gpu);
        CALL_CU(cuCtxSetCurrent, context);
        CALL_CU(cuStreamCreate, &stream, CU_STREAM_NON_BLOCKING);
        CALL_CU(cuMemAlloc, &work, size_t(width) * height * 4);
        CALL_VFX(NvVFX_CreateEffect, "VideoFrameGeneration", &effect);
        CALL_VFX(NvVFX_SetCudaStream, effect, NVVFX_CUDA_STREAM, stream);
        CALL_VFX(NvVFX_SetU32, effect, "InputWidth", width);
        CALL_VFX(NvVFX_SetU32, effect, "InputHeight", height);
        CALL_VFX(NvVFX_SetU32, effect, "Mode", s.quality);
        CALL_VFX(NvVFX_SetU32, effect, "AutomaticShotChangeDetectionEnabled", 1);
        CALL_VFX(NvVFX_Load, effect);
        CALL_VFX(NvVFX_SetU32, effect, "FrameMultiplier", s.factor);
        CALL_VFX(NvVFX_SetU32, effect, "ShotChange", 0);
        images = PaneAllocImages(3);
        if (!images) throw std::bad_alloc();
    }
    CUgraphicsResource Register(ID3D11Buffer* buffer) {
        CUgraphicsResource registration{};
        CALL_CU(cuGraphicsD3D11RegisterResource, &registration, buffer, CU_GRAPHICS_REGISTER_FLAGS_NONE);
        registrations.push_back(registration);
        return registration;
    }
    void Generate(CUgraphicsResource previous, CUgraphicsResource current,
                  const std::vector<CUgraphicsResource>& outputs, int width, int height, bool pq) {
        auto entries = outputs;
        entries.push_back(previous);
        entries.push_back(current);
        CALL_CU(cuGraphicsMapResources, static_cast<unsigned>(entries.size()), entries.data(), stream);
        try {
            const auto pixel = pq ? NVCV_RGB10A2 : NVCV_BGRA;
            const auto type = pq ? NVCV_P32 : NVCV_U8;
            // Use the C image view ABI to keep allocation and lifetime with CUDA / D3D11.
            auto pointer = [&](CUgraphicsResource entry) {
                CUdeviceptr address;
                size_t size;
                CALL_CU(cuGraphicsResourceGetMappedPointer, &address, &size, entry);
                if (size < size_t(width) * height * 4) throw std::runtime_error("CUDA buffer too small");
                return address;
            };
            const CUdeviceptr pointers[]{pointer(previous), pointer(current), work};
            for (int i = 0; i < 3; ++i)
                cv(NvCVImage_InitP(PaneGetImage(images, i), width, height, width * 4, reinterpret_cast<void*>(pointers[i]),
                                   pixel, type, NVCV_CHUNKY, NVCV_CUDA), "NvCVImage_Init");
            CALL_VFX(NvVFX_SetImage, effect, NVVFX_INPUT_IMAGE_0, PaneGetImage(images, 0));
            CALL_VFX(NvVFX_SetImage, effect, NVVFX_INPUT_IMAGE_1, PaneGetImage(images, 1));
            CALL_VFX(NvVFX_SetImage, effect, NVVFX_OUTPUT_IMAGE, PaneGetImage(images, 2));
            for (unsigned i = 0; i < outputs.size(); ++i) {
                CALL_VFX(NvVFX_SetU32, effect, "FrameIndex", i + 1);
                CALL_VFX(NvVFX_Run, effect, 0);
                CALL_CU(cuMemcpyDtoDAsync, pointer(outputs[i]), work, size_t(width) * height * 4, stream);
            }
            CALL_CU(cuGraphicsUnmapResources, static_cast<unsigned>(entries.size()), entries.data(), stream);
        } catch (...) {
            cuGraphicsUnmapResourcesP(static_cast<unsigned>(entries.size()), entries.data(), stream);
            throw;
        }
    }
    ~Compute() {
        if (stream) cuStreamSynchronizeP(stream);
        if (effect) NvVFX_DestroyEffectP(effect);
        std::free(images);
        for (auto r : registrations) cuGraphicsUnregisterResourceP(r);
        if (work) cuMemFreeP(work);
        if (stream) cuStreamDestroyP(stream);
        if (context) { cuCtxSetCurrentP(nullptr); cuDevicePrimaryCtxReleaseP(gpu); }
        if (vfx) FreeLibrary(vfx);
        if (image) FreeLibrary(image);
        if (driver) FreeLibrary(driver);
        if (coreDir) RemoveDllDirectory(coreDir);
        if (featureDir) RemoveDllDirectory(featureDir);
    }
};

// NVIDIA D3D11 video processor extensions. These are also used by the legacy Panevanta engine.
constexpr GUID srExtension{0xd43ce1b3, 0x1f4b, 0x48ac, {0xba,0xee,0xc3,0xc2,0x53,0x75,0xe6,0xf7}};
constexpr GUID hdrExtension{0xfdd62bb4, 0x620b, 0x4fd7, {0x9a,0xb3,0x1e,0x59,0xd0,0xd5,0x44,0xb3}};
constexpr auto sdrSpace = DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709;
constexpr auto hdrSpace = DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;

struct Vpp {
    ComPtr<ID3D11VideoContext1> context;
    ComPtr<ID3D11VideoProcessor> processor;
    ComPtr<ID3D11VideoProcessorInputView> input;
    ComPtr<ID3D11VideoProcessorOutputView> output;
    unsigned frame{};
    void Setup(ID3D11Device* device, ID3D11DeviceContext* gfx, ID3D11Texture2D* in, ID3D11Texture2D* out, const Settings& s) {
        ComPtr<ID3D11VideoDevice> video;
        hr(device->QueryInterface(IID_PPV_ARGS(&video)), "D3D11 Video Device");
        hr(gfx->QueryInterface(IID_PPV_ARGS(&context)), "D3D11 Video Context");
        D3D11_TEXTURE2D_DESC a{}, b{};
        in->GetDesc(&a);
        out->GetDesc(&b);
        D3D11_VIDEO_PROCESSOR_CONTENT_DESC desc{};
        desc.InputFrameFormat = D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE;
        desc.InputFrameRate = desc.OutputFrameRate = {static_cast<unsigned>(s.fps), 1};
        desc.InputWidth = a.Width; desc.InputHeight = a.Height;
        desc.OutputWidth = b.Width; desc.OutputHeight = b.Height;
        desc.Usage = D3D11_VIDEO_USAGE_PLAYBACK_NORMAL;
        ComPtr<ID3D11VideoProcessorEnumerator> types;
        hr(video->CreateVideoProcessorEnumerator(&desc, &types), "Create NVIDIA video processor enumerator");
        hr(video->CreateVideoProcessor(types.Get(), 0, &processor), "Create NVIDIA video processor");
        D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC id{};
        id.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
        D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC od{};
        od.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
        hr(video->CreateVideoProcessorInputView(in, types.Get(), &id, &input), "Create video processor input view");
        hr(video->CreateVideoProcessorOutputView(out, types.Get(), &od, &output), "Create video processor output view");
        const RECT inputBox{0, 0, LONG(a.Width), LONG(a.Height)};
        const RECT outputBox{0, 0, LONG(b.Width), LONG(b.Height)};
        context->VideoProcessorSetStreamSourceRect(processor.Get(), 0, TRUE, &inputBox);
        context->VideoProcessorSetStreamDestRect(processor.Get(), 0, TRUE, &outputBox);
        context->VideoProcessorSetOutputTargetRect(processor.Get(), TRUE, &outputBox);
        context->VideoProcessorSetStreamAutoProcessingMode(processor.Get(), 0, FALSE);
        context->VideoProcessorSetStreamFrameFormat(processor.Get(), 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
        context->VideoProcessorSetStreamOutputRate(processor.Get(), 0, D3D11_VIDEO_PROCESSOR_OUTPUT_RATE_NORMAL, FALSE, nullptr);
        if (s.hdr) {
            unsigned supported = 0;
            hr(context->VideoProcessorGetStreamExtension(processor.Get(), 0, &hdrExtension, sizeof(supported), &supported),
               "Query NVIDIA RTX Video HDR");
            if (!supported) throw std::runtime_error("NVIDIA RTX Video HDR is unavailable");
            unsigned settings[]{4, 3, 1};
            hr(context->VideoProcessorSetStreamExtension(processor.Get(), 0, &hdrExtension, sizeof(settings), settings),
               "Enable NVIDIA RTX Video HDR");
        }
        context->VideoProcessorSetStreamColorSpace1(processor.Get(), 0, sdrSpace);
        context->VideoProcessorSetOutputColorSpace1(processor.Get(), s.hdr ? hdrSpace : sdrSpace);
        if (s.sr) {
            unsigned settings[]{1, 2, 1};
            hr(context->VideoProcessorSetStreamExtension(processor.Get(), 0, &srExtension, sizeof(settings), settings),
               "Enable NVIDIA RTX Super Resolution");
        }
    }
    void Run() {
        D3D11_VIDEO_PROCESSOR_STREAM stream{};
        stream.Enable = TRUE;
        stream.InputFrameOrField = frame;
        stream.pInputSurface = input.Get();
        hr(context->VideoProcessorBlt(processor.Get(), output.Get(), frame, 1, &stream), "NVIDIA video processing");
        ++frame;
    }
};

static const char* shaders = R"(
Texture2D<float4> image : register(t0);
Buffer<uint> pixels : register(t1);
RWBuffer<uint> packed : register(u0);
SamplerState linearClamp : register(s0);
cbuffer Config : register(b0) { uint width; uint height; uint inputPQ; uint outputPQ; float white; float peak; float2 pad; };
struct Vertex { float4 position : SV_Position; float2 uv : TEXCOORD0; };
Vertex VS(uint id : SV_VertexID) {
    Vertex result;
    result.uv = float2((id << 1) & 2, id & 2);
    result.position = float4(result.uv * float2(2,-2) + float2(-1,1),0,1);
    return result;
}
float3 srgb(float3 light) {
    return lerp(light * 12.92, 1.055 * pow(light, 1.0/2.4) - 0.055, step(0.0031308, light));
}
float3 pq(float3 value) {
    float3 y = pow(saturate(value), 2610.0/16384.0);
    return pow((3424.0/4096.0 + (2413.0/128.0)*y) / (1 + (2392.0/128.0)*y), 2523.0/32.0);
}
float3 fromPQ(float3 code) {
    float3 v = pow(saturate(code), 32.0/2523.0);
    return pow(max(v - 3424.0/4096.0, 0) / max((2413.0/128.0) - (2392.0/128.0)*v, 1e-6), 16384.0/2610.0);
}
float4 Normalize(Vertex i) : SV_Target {
    float3 sc = image.SampleLevel(linearClamp, i.uv, 0).rgb;
    if (inputPQ != 0) {
        float3 bt2020 = mul(float3x3(.627404,.329283,.043313,.069097,.919540,.011362,.016391,.088013,.895595), sc);
        return float4(pq(bt2020 * .008),1);
    }
    return float4(srgb(saturate(sc / white)),1);
}
[numthreads(16,16,1)]
void Pack(uint3 id : SV_DispatchThreadID) {
    if (id.x >= width || id.y >= height) return;
    float3 value = saturate(image[id.xy].rgb);
    uint3 c = (uint3)round(value * (inputPQ != 0 ? 1023 : 255));
    packed[id.x + width * id.y] = inputPQ != 0 ? (c.r | (c.g << 10) | (c.b << 20) | 0xc0000000)
                                               : (c.b | (c.g << 8) | (c.r << 16) | 0xff000000);
}
float3 unpack(uint2 xy) {
    uint p = pixels[xy.x + xy.y * width];
    return inputPQ != 0 ? float3(p & 1023, (p>>10)&1023, (p>>20)&1023)/1023.0
                       : float3((p>>16)&255, (p>>8)&255, p&255)/255.0;
}
float4 Present(Vertex i) : SV_Target {
    float2 coord = clamp(i.uv * float2(width,height) - .5, 0, float2(width-1,height-1));
    uint2 lo = (uint2)coord, hi = min(lo+1, uint2(width-1,height-1));
    float2 f = frac(coord);
    float3 c = lerp(lerp(unpack(lo),unpack(uint2(hi.x,lo.y)),f.x),lerp(unpack(uint2(lo.x,hi.y)),unpack(hi),f.x),f.y);
    if (inputPQ != 0 && outputPQ == 0) {
        float3 nits = fromPQ(c) * 10000;
        float3 rgb = mul(float3x3(1.66049,-.58764,-.07285,-.12455,1.13290,-.00835,-.01815,-.10058,1.11873),nits);
        float maxValue = max(max(rgb.r,rgb.g),rgb.b);
        c = srgb(saturate(rgb / max(100, maxValue)));
    }
    if (inputPQ != 0 && outputPQ != 0 && peak > 0)
        c = min(c, pq(peak / 10000));
    return float4(c,1);
}
)";

struct Frame {
    ComPtr<ID3D11Buffer> data;
    ComPtr<ID3D11ShaderResourceView> view;
    ComPtr<ID3D11UnorderedAccessView> write;
    CUgraphicsResource cuda{};
};
struct Group {
    std::array<Frame, 8> frames;
    int64_t sequence{};
    int64_t captured{};
    int count{};
};
struct Config { unsigned width, height, inputPQ, outputPQ; float white, peak, pad[2]; };

class Engine {
    Settings s;
    HWND hwnd;
    int width, height;
    bool hdrInput, hdrFrames, hdrOutput;
    ComPtr<ID3D11Device> dev;
    ComPtr<ID3D11DeviceContext> ctx;
    ComPtr<ID3D11Multithread> multi;
    struct ContextGuard {
        ID3D11Multithread* multi;
        explicit ContextGuard(ID3D11Multithread* protection) : multi(protection) { multi->Enter(); }
        ~ContextGuard() { multi->Leave(); }
    };
    ComPtr<ID3D11Texture2D> normalized, processed, captureCopy;
    ComPtr<ID3D11RenderTargetView> inputRT, backRT;
    ComPtr<ID3D11ShaderResourceView> normalizedView, processedView, captureView;
    struct InputBinding {
        ComPtr<ID3D11Texture2D> texture;
        ComPtr<ID3D11ShaderResourceView> view;
    };
    std::array<InputBinding, 3> inputBindings;
    size_t nextInputBinding{};
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> normalize, present;
    ComPtr<ID3D11ComputeShader> pack;
    ComPtr<ID3D11SamplerState> sampler;
    ComPtr<ID3D11Buffer> config;
    ComPtr<IDXGISwapChain3> swap;
    UINT displayWidth{}, displayHeight{};
    Vpp vpp;
    std::array<Frame, 2> pair;
    std::array<Group, 3> groups;
    std::unique_ptr<Compute> compute;
    int slot{};
    int64_t previous{};
    int64_t previousCapture{};
    int64_t frequency{};
    std::ofstream log;
    std::mutex gpuMutex, queueMutex;
    std::condition_variable signal;
    std::deque<int> available{0, 1, 2}, pending;
    std::thread output;
    std::atomic_bool stopping{false}, failed{false};
    std::atomic_int drops{0};
    std::string renderError;

    static ComPtr<ID3DBlob> Compile(const char* entry, const char* profile) {
        ComPtr<ID3DBlob> blob, error;
        const auto result = D3DCompile(shaders, std::strlen(shaders), nullptr, nullptr, nullptr,
                                       entry, profile, D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &blob, &error);
        if (FAILED(result) && error) throw std::runtime_error(static_cast<const char*>(error->GetBufferPointer()));
        hr(result, entry);
        return blob;
    }
    void MakeTexture(UINT w, UINT h, DXGI_FORMAT fmt, UINT bind, ID3D11Texture2D** out) {
        D3D11_TEXTURE2D_DESC d{};
        d.Width = w; d.Height = h; d.MipLevels = d.ArraySize = d.SampleDesc.Count = 1;
        d.Format = fmt; d.BindFlags = bind;
        hr(dev->CreateTexture2D(&d, nullptr, out), "Create video texture");
    }
    void MakeFrame(Frame& frame, bool writable, bool cuda) {
        D3D11_BUFFER_DESC d{};
        d.ByteWidth = width * height * 4;
        d.BindFlags = D3D11_BIND_SHADER_RESOURCE | (writable ? D3D11_BIND_UNORDERED_ACCESS : 0);
        hr(dev->CreateBuffer(&d, nullptr, &frame.data), "Create packed video buffer");
        D3D11_SHADER_RESOURCE_VIEW_DESC srv{};
        srv.Format = DXGI_FORMAT_R32_UINT;
        srv.ViewDimension = D3D11_SRV_DIMENSION_BUFFER;
        srv.Buffer.NumElements = width * height;
        hr(dev->CreateShaderResourceView(frame.data.Get(), &srv, &frame.view), "Create packed shader view");
        if (writable) {
            D3D11_UNORDERED_ACCESS_VIEW_DESC uav{};
            uav.Format = DXGI_FORMAT_R32_UINT;
            uav.ViewDimension = D3D11_UAV_DIMENSION_BUFFER;
            uav.Buffer.NumElements = width * height;
            hr(dev->CreateUnorderedAccessView(frame.data.Get(), &uav, &frame.write), "Create packed UAV");
        }
        if (cuda) frame.cuda = compute->Register(frame.data.Get());
    }
    Config Values(bool pq, bool outputPQ) const {
        return {UINT(width), UINT(height), pq ? 1u : 0u, outputPQ ? 1u : 0u, s.white, float(s.peak), {}};
    }
    void Draw(ID3D11RenderTargetView* rt, ID3D11PixelShader* ps, UINT w, UINT h,
              ID3D11ShaderResourceView* texture, ID3D11ShaderResourceView* buffer, const Config& values) {
        ID3D11ShaderResourceView* inputs[]{texture, buffer};
        ID3D11ShaderResourceView* empty[2]{};
        ID3D11Buffer* cb = config.Get();
        ID3D11SamplerState* clamp = sampler.Get();
        D3D11_VIEWPORT viewport{0, 0, float(w), float(h), 0, 1};
        ctx->UpdateSubresource(cb, 0, nullptr, &values, 0, 0);
        ctx->OMSetRenderTargets(1, &rt, nullptr);
        ctx->RSSetViewports(1, &viewport);
        ctx->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        ctx->VSSetShader(vs.Get(), nullptr, 0);
        ctx->PSSetShader(ps, nullptr, 0);
        ctx->PSSetShaderResources(0, 2, inputs);
        ctx->PSSetSamplers(0, 1, &clamp);
        ctx->PSSetConstantBuffers(0, 1, &cb);
        ctx->Draw(3, 0);
        ctx->OMSetRenderTargets(0, nullptr, nullptr);
        ctx->PSSetShaderResources(0, 2, empty);
    }
    void Prepare(ID3D11Texture2D* texture, Frame& dst) {
        auto binding = std::find_if(inputBindings.begin(), inputBindings.end(),
            [texture](const InputBinding& entry) { return entry.texture.Get() == texture; });
        if (binding == inputBindings.end()) {
            binding = inputBindings.begin() + nextInputBinding;
            nextInputBinding = (nextInputBinding + 1) % inputBindings.size();
            D3D11_TEXTURE2D_DESC source{};
            texture->GetDesc(&source);
            binding->view.Reset();
            if (source.BindFlags & D3D11_BIND_SHADER_RESOURCE) {
                hr(dev->CreateShaderResourceView(texture, nullptr, &binding->view), "Capture shader view");
            } else if (!captureCopy) {
                MakeTexture(source.Width, source.Height, source.Format, D3D11_BIND_SHADER_RESOURCE, &captureCopy);
                hr(dev->CreateShaderResourceView(captureCopy.Get(), nullptr, &captureView), "Create capture view");
            }
            binding->texture = texture;
        }
        auto* sourceView = binding->view.Get();
        if (!sourceView) {
            ctx->CopyResource(captureCopy.Get(), texture);
            sourceView = captureView.Get();
        }
        Draw(inputRT.Get(), normalize.Get(), s.width, s.height, sourceView, nullptr, Values(hdrInput, false));
        if (processed) vpp.Run();
        auto* source = processed ? processedView.Get() : normalizedView.Get();
        ID3D11UnorderedAccessView* write = dst.write.Get();
        ID3D11Buffer* cb = config.Get();
        const auto values = Values(hdrFrames, false);
        ctx->UpdateSubresource(cb, 0, nullptr, &values, 0, 0);
        ctx->CSSetShader(pack.Get(), nullptr, 0);
        ctx->CSSetShaderResources(0, 1, &source);
        ctx->CSSetUnorderedAccessViews(0, 1, &write, nullptr);
        ctx->CSSetConstantBuffers(0, 1, &cb);
        ctx->Dispatch((width + 15) / 16, (height + 15) / 16, 1);
        source = nullptr;
        write = nullptr;
        ctx->CSSetShaderResources(0, 1, &source);
        ctx->CSSetUnorderedAccessViews(0, 1, &write, nullptr);
    }
    void RenderLoop() noexcept {
        try {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
            constexpr DWORD highResolution = 0x00000002;
            const std::unique_ptr<void, decltype(&CloseHandle)> timer(
                CreateWaitableTimerExW(nullptr, nullptr, highResolution, TIMER_MODIFY_STATE | SYNCHRONIZE),
                CloseHandle);
            const auto precisionWindow = std::max<int64_t>(1, frequency / 5000);
            int64_t next = 0, lastSample = 0;
            int64_t rendered = 0, lastSequence = 0;
            const auto interval = frequency / (int64_t(s.fps) * s.factor);
            while (!stopping) {
                int index;
                {
                    std::unique_lock lock(queueMutex);
                    signal.wait(lock, [&]{ return stopping || !pending.empty(); });
                    if (stopping) break;
                    index = pending.front();
                    pending.pop_front();
                }
                auto& group = groups[index];
                const auto now = qpc();
                if (!next || next < now - interval || group.sequence > lastSequence + 1)
                    next = now;
                RECT size{};
                GetClientRect(hwnd, &size);
                for (int i = 0; i < group.count && !stopping; ++i) {
                    if (qpc() - group.captured > frequency / 5) {
                        drops += group.count - i;
                        next = 0;
                        break;
                    }
                    while (!stopping) {
                        int64_t left = next - qpc();
                        if (left <= 0) break;
                        if (timer && left > frequency / 1000) {
                            LARGE_INTEGER due{};
                            due.QuadPart = -std::max<int64_t>(1, (left - precisionWindow) * 10'000'000 / frequency);
                            if (SetWaitableTimer(timer.get(), &due, 0, nullptr, nullptr, FALSE)
                                && WaitForSingleObject(timer.get(), 100) == WAIT_OBJECT_0)
                                continue;
                        }
                        if (left > frequency / 400) Sleep(1);
                        else SwitchToThread();
                    }
                    if (stopping) break;
                    {
                        std::lock_guard lock(gpuMutex);
                        // NR shares this context. Protect the entire set-state/draw sequence.
                        ContextGuard context(multi.Get());
                        if (size.right && size.bottom && (UINT(size.right) != displayWidth || UINT(size.bottom) != displayHeight))
                            Resize(size.right, size.bottom);
                        Draw(backRT.Get(), present.Get(), displayWidth, displayHeight,
                             nullptr, group.frames[i].view.Get(), Values(hdrFrames, hdrOutput));
                    }
                    const auto presented = swap->Present(0, DXGI_PRESENT_DO_NOT_WAIT);
                    if (presented == DXGI_ERROR_WAS_STILL_DRAWING) {
                        drops++;
                    } else if (presented != DXGI_STATUS_OCCLUDED) {
                        hr(presented, "DXGI Present");
                        ++rendered;
                        const auto t = qpc();
                        if (t - lastSample >= frequency * 2 / 5) {
                            double pts = (group.sequence - 1 + i / double(s.factor)) / s.fps;
                            log << "PANE_RENDER " << rendered << ' ' << t << ' ' << std::fixed << std::setprecision(8)
                                << pts << ' ' << drops.load() << std::endl;
                            lastSample = t;
                        }
                    }
                    next += interval;
                }
                lastSequence = group.sequence;
                {
                    std::lock_guard lock(queueMutex);
                    available.push_back(index);
                }
            }
        } catch (const std::exception& error) {
            renderError = error.what();
            failed.store(true);
        }
    }
    void Resize(UINT w, UINT h) {
        backRT.Reset();
        if (displayWidth) hr(swap->ResizeBuffers(0, w, h, DXGI_FORMAT_UNKNOWN, 0), "ResizeBuffers");
        displayWidth = w; displayHeight = h;
        ComPtr<ID3D11Texture2D> back;
        hr(swap->GetBuffer(0, IID_PPV_ARGS(&back)), "Get swapchain buffer");
        hr(dev->CreateRenderTargetView(back.Get(), nullptr, &backRT), "Swapchain render target");
        if (hdrOutput) hr(swap->SetColorSpace1(hdrSpace), "Set HDR10 swapchain color space");
    }
public:
    Engine(ID3D11Texture2D* texture, HWND window, const Settings& options, const wchar_t* sdk, const wchar_t* logPath)
        : s(options), hwnd(window), width(s.sr ? std::max(s.width + 2, (int(s.width * s.scale)) & ~1) : s.width),
          height(s.sr ? std::max(s.height + 2, (int(s.height * s.scale)) & ~1) : s.height),
          hdrInput(s.displayHdr && !s.sr && !s.hdr), hdrFrames(s.hdr || hdrInput),
          hdrOutput(s.displayHdr && hdrFrames), log(std::filesystem::path(logPath), std::ios::out | std::ios::trunc) {
        LARGE_INTEGER rate; QueryPerformanceFrequency(&rate); frequency = rate.QuadPart;
        if (!log) throw std::runtime_error("Could not open the render log");
        texture->GetDevice(&dev);
        dev->GetImmediateContext(&ctx);
        hr(ctx.As(&multi), "D3D11 multithread protection");
        multi->SetMultithreadProtected(TRUE);
        ComPtr<IDXGIDevice> dxgi;
        ComPtr<IDXGIAdapter> adapter;
        ComPtr<IDXGIFactory2> factory;
        hr(dev.As(&dxgi), "DXGI Device");
        hr(dxgi->GetAdapter(&adapter), "Capture adapter");
        hr(adapter->GetParent(IID_PPV_ARGS(&factory)), "DXGI factory");

        if (s.factor > 1) {
            compute = std::make_unique<Compute>();
            compute->Setup(adapter.Get(), s, width, height, sdk);
        }
        auto vertex = Compile("VS", "vs_5_0");
        auto ps = Compile("Normalize", "ps_5_0");
        auto draw = Compile("Present", "ps_5_0");
        auto cvt = Compile("Pack", "cs_5_0");
        hr(dev->CreateVertexShader(vertex->GetBufferPointer(), vertex->GetBufferSize(), nullptr, &vs), "Create vertex shader");
        hr(dev->CreatePixelShader(ps->GetBufferPointer(), ps->GetBufferSize(), nullptr, &normalize), "Create normalization shader");
        hr(dev->CreatePixelShader(draw->GetBufferPointer(), draw->GetBufferSize(), nullptr, &present), "Create present shader");
        hr(dev->CreateComputeShader(cvt->GetBufferPointer(), cvt->GetBufferSize(), nullptr, &pack), "Create packing shader");
        D3D11_SAMPLER_DESC sample{};
        sample.Filter = D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT;
        sample.AddressU = sample.AddressV = sample.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
        sample.MaxLOD = D3D11_FLOAT32_MAX;
        hr(dev->CreateSamplerState(&sample, &sampler), "Create sampler");
        D3D11_BUFFER_DESC constants{};
        constants.ByteWidth = sizeof(Config);
        constants.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
        hr(dev->CreateBuffer(&constants, nullptr, &config), "Create shader constants");
        MakeTexture(s.width, s.height, hdrInput ? DXGI_FORMAT_R10G10B10A2_UNORM : DXGI_FORMAT_B8G8R8A8_UNORM,
                    D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, &normalized);
        hr(dev->CreateRenderTargetView(normalized.Get(), nullptr, &inputRT), "Normalize target");
        hr(dev->CreateShaderResourceView(normalized.Get(), nullptr, &normalizedView), "Normalize input");
        if (s.sr || s.hdr) {
            MakeTexture(width, height, s.hdr ? DXGI_FORMAT_R10G10B10A2_UNORM : DXGI_FORMAT_B8G8R8A8_UNORM,
                        D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, &processed);
            hr(dev->CreateShaderResourceView(processed.Get(), nullptr, &processedView), "Video processor view");
            vpp.Setup(dev.Get(), ctx.Get(), normalized.Get(), processed.Get(), s);
        }
        for (auto& f : pair) MakeFrame(f, true, s.factor > 1);
        for (auto& group : groups) for (int i = 0; i < s.factor; ++i)
            MakeFrame(group.frames[i], false, i > 0);
        RECT size{};
        GetClientRect(hwnd, &size);
        DXGI_SWAP_CHAIN_DESC1 desc{};
        desc.Width = std::max(1L, size.right); desc.Height = std::max(1L, size.bottom);
        desc.Format = hdrOutput ? DXGI_FORMAT_R10G10B10A2_UNORM : DXGI_FORMAT_B8G8R8A8_UNORM;
        desc.SampleDesc.Count = 1;
        desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        desc.BufferCount = 3;
        desc.Scaling = DXGI_SCALING_STRETCH;
        desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
        ComPtr<IDXGISwapChain1> chain;
        hr(factory->CreateSwapChainForHwnd(dev.Get(), hwnd, &desc, nullptr, nullptr, &chain), "Create Panevanta swapchain");
        hr(chain.As(&swap), "Query DXGI SwapChain3");
        Resize(desc.Width, desc.Height);
        log << "Panevanta native D3D11/CUDA " << width << "x" << height << std::endl;
        if (s.sr) log << "NVIDIA RTX Super Resolution enabled." << std::endl;
        if (s.hdr) log << "NVIDIA RTX Video HDR enabled." << std::endl;
        if (hdrOutput) log << "Output color space RGB_FULL_G2084_NONE_P2020" << std::endl;
        output = std::thread(&Engine::RenderLoop, this);
    }
    void Check() {
        if (failed.load()) throw std::runtime_error(renderError);
    }
    void Submit(ID3D11Texture2D* texture, int64_t sequence, int64_t captured) {
        Check();
        int index = -1;
        bool generation = compute && previous && sequence == previous + 1;
        bool show = !compute || previous;
        if (show) {
            std::lock_guard lock(queueMutex);
            if (!pending.empty()) {
                index = pending.back();
                pending.pop_back();
                drops += groups[index].count;
            } else if (!available.empty()) {
                index = available.front(); available.pop_front();
            }
        }
        {
            std::lock_guard lock(gpuMutex);
            ContextGuard context(multi.Get());
            Prepare(texture, pair[slot]);
            if (index != -1) {
                auto& g = groups[index];
                g.count = generation ? s.factor : 1;
                g.sequence = compute ? previous : sequence;
                g.captured = compute ? previousCapture : captured;
                ctx->CopyResource(g.frames[0].data.Get(), pair[compute ? 1 - slot : slot].data.Get());
            }
        }
        if (index != -1) {
            auto& g = groups[index];
            if (generation) {
                std::vector<CUgraphicsResource> dst;
                for (int i = 1; i < s.factor; ++i) dst.push_back(g.frames[i].cuda);
                compute->Generate(pair[1 - slot].cuda, pair[slot].cuda, dst, width, height, hdrFrames);
            }
            std::lock_guard lock(queueMutex);
            pending.push_back(index);
            signal.notify_one();
        }
        previous = sequence;
        previousCapture = captured;
        slot = 1 - slot;
    }
    ~Engine() {
        stopping.store(true);
        signal.notify_all();
        if (output.joinable()) output.join();
    }
};

static void errorText(char* message, size_t length, const std::exception& error) {
    if (message && length) std::snprintf(message, length, "%s", error.what());
}

extern "C" __declspec(dllexport) HRESULT PaneCreateCaptureDevice(ID3D11Device** device) {
    ComPtr<IDXGIFactory1> factory;
    HRESULT result = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
    if (FAILED(result)) return result;
    for (UINT number = 0; ; ++number) {
        ComPtr<IDXGIAdapter1> adapter;
        result = factory->EnumAdapters1(number, &adapter);
        if (FAILED(result)) return result;
        DXGI_ADAPTER_DESC1 description{};
        if (SUCCEEDED(adapter->GetDesc1(&description)) && description.VendorId == 0x10de &&
            !(description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE)) {
            result = D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                nullptr, 0, D3D11_SDK_VERSION, device, nullptr, nullptr);
            if (SUCCEEDED(result)) return result;
        }
    }
}

extern "C" __declspec(dllexport) void* PaneCreate(ID3D11Texture2D* texture, HWND window,
    const Settings* settings, const wchar_t* sdk, const wchar_t* log, char* message, size_t length) {
    try { return new Engine(texture, window, *settings, sdk, log); }
    catch (const std::exception& error) { errorText(message, length, error); return nullptr; }
}
extern "C" __declspec(dllexport) int PaneSubmit(Engine* engine, ID3D11Texture2D* texture,
    int64_t sequence, int64_t captured, char* message, size_t length) {
    try { if (texture) engine->Submit(texture, sequence, captured); else engine->Check(); return 1; }
    catch (const std::exception& error) { errorText(message, length, error); return 0; }
}
extern "C" __declspec(dllexport) void PaneDestroy(Engine* engine) { delete engine; }
