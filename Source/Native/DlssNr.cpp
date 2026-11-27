// Panevanta D3D11/D3D12 bridge for a locally installed NVIDIA NR runtime.
#include <windows.h>
#include <d3d11_4.h>
#include <d3d12.h>
#include <d3dcompiler.h>
#include <dxgi1_4.h>
#include <wrl/client.h>
#include <wintrust.h>
#include <softpub.h>
#include <filesystem>
#include <array>
#include <algorithm>
#include <cstdio>
#include <initializer_list>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
#include "nvsdk_ngx.h"

using Microsoft::WRL::ComPtr;
using namespace std::string_literals;

static void check(HRESULT r, const char* action) {
    if (FAILED(r)) {
        char message[180]{};
        std::snprintf(message, sizeof(message), "%s: 0x%08lX", action, static_cast<unsigned long>(r));
        throw std::runtime_error(message);
    }
}

static void ngx_check(NVSDK_NGX_Result r, const char* action) {
    if (NVSDK_NGX_FAILED(r)) check(static_cast<HRESULT>(r), action);
}

struct WinHandle {
    HANDLE value{};
    ~WinHandle() { if (value) CloseHandle(value); }
};

static void verify_runtime(const std::filesystem::path& path) {
    const auto size = GetFileVersionInfoSizeW(path.c_str(), nullptr);
    std::vector<unsigned char> version(size);
    VS_FIXEDFILEINFO* fixed = nullptr;
    UINT length = 0;
    if (!size || !GetFileVersionInfoW(path.c_str(), 0, size, version.data()) ||
        !VerQueryValueW(version.data(), L"\\", reinterpret_cast<void**>(&fixed), &length) ||
        !fixed || length < sizeof(*fixed) || fixed->dwSignature != VS_FFI_SIGNATURE ||
        fixed->dwFileVersionMS < MAKELONG(9,310) ||
        (fixed->dwFileVersionMS == MAKELONG(9,310) && HIWORD(fixed->dwFileVersionLS) < 16)) {
        throw std::runtime_error("DLSS NR requires NVIDIA runtime 310.9.16 or later");
    }
    WINTRUST_FILE_INFO file{};
    file.cbStruct = sizeof(file);
    file.pcwszFilePath = path.c_str();
    WINTRUST_DATA info{};
    info.cbStruct = sizeof(info);
    info.dwUIChoice = WTD_UI_NONE;
    info.fdwRevocationChecks = WTD_REVOKE_NONE;
    info.dwUnionChoice = WTD_CHOICE_FILE;
    info.pFile = &file;
    info.dwStateAction = WTD_STATEACTION_VERIFY;
    info.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
    GUID policy = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    const auto result = WinVerifyTrust(nullptr, &policy, &info);
    bool nvidia = false;
    if (result == ERROR_SUCCESS && info.hWVTStateData) {
        const auto* provider = WTHelperProvDataFromStateData(info.hWVTStateData);
        const auto* signer = provider ? WTHelperGetProvSignerFromChain(const_cast<CRYPT_PROVIDER_DATA*>(provider), 0, FALSE, 0) : nullptr;
        if (signer && signer->csCertChain && signer->pasCertChain) {
            wchar_t company[256]{};
            CertGetNameStringW(signer->pasCertChain[0].pCert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, nullptr, company, 256);
            nvidia = wcscmp(company, L"NVIDIA Corporation") == 0;
        }
    }
    info.dwStateAction = WTD_STATEACTION_CLOSE;
    WinVerifyTrust(nullptr, &policy, &info);
    if (!nvidia) throw std::runtime_error("The DLSS NR runtime needs a valid NVIDIA Authenticode signature");
}

static const char* shader = R"(
cbuffer Options:register(b0) { float white; float hdr; float detail; float useHistory; };
Texture2D source : register(t0);
Texture2D input : register(t1);
Texture2D enhanced : register(t2);
Texture2D previousInput : register(t3);
Texture2D previousEnhanced : register(t4);
SamplerState linearSampler : register(s0);
struct Vertex { float4 pos:SV_Position; float2 uv:TEXCOORD0; };
Vertex vs(uint id:SV_VertexID) {
    Vertex v; v.uv=float2((id<<1)&2,id&2); v.pos=float4(v.uv*float2(2,-2)+float2(-1,1),0,1); return v;
}
float3 encode(float3 p) {
    p=saturate(p);
    return lerp(12.92*p, 1.055*pow(max(p,0.000001),1.0/2.4)-0.055, step(0.0031308,p));
}
float3 decode(float3 p) {
    p=saturate(p);
    return lerp(p/12.92, pow((p+0.055)/1.055,2.4), step(0.04045,p));
}
float4 toNr(Vertex v):SV_Target {
    return float4(encode(source.SampleLevel(linearSampler,v.uv,0).rgb/max(1,white)),1);
}
float3 correction(float2 uv) {
    return (decode(enhanced.SampleLevel(linearSampler,uv,0).rgb)
           -decode(input.SampleLevel(linearSampler,uv,0).rgb))*max(1,white);
}
float4 fromNr(Vertex v):SV_Target {
    float3 original=source.SampleLevel(linearSampler,v.uv,0).rgb;
    float3 delta=correction(v.uv);
    // The previous frame contributes only its correction where the source color stayed still.
    if(useHistory>0.5) {
        float3 current=input.SampleLevel(linearSampler,v.uv,0).rgb;
        float3 previous=previousInput.SampleLevel(linearSampler,v.uv,0).rgb;
        float3 difference=abs(current-previous);
        float amount=.45*(1-smoothstep(.008,.035,max(difference.r,max(difference.g,difference.b))));
        if(amount>0) {
            float3 old=(decode(previousEnhanced.SampleLevel(linearSampler,v.uv,0).rgb)-decode(previous))*max(1,white);
            delta=lerp(delta,old,amount);
        }
    }
    float lower=hdr>0.5 ? -0.5 : 0;
    float upper=hdr>0.5 ? 125.0 : 1.0;
    if(detail<0.5) {
        uint w,h;
        input.GetDimensions(w,h);
        float2 reach=3.0/float2(w,h);
        delta=delta*.5+.125*(correction(v.uv+float2(reach.x,0))+correction(v.uv-float2(reach.x,0))+
            correction(v.uv+float2(0,reach.y))+correction(v.uv-float2(0,reach.y)));
        // Clamp the shared luminance change before it can clip just one color channel.
        float3 base=clamp(original,lower,upper);
        float darkest=min(base.r,min(base.g,base.b));
        float brightest=max(base.r,max(base.g,base.b));
        float shift=clamp(dot(delta,float3(0.2126,0.7152,0.0722)),lower-darkest,upper-brightest);
        return float4(base+shift.xxx,1);
    }
    return float4(clamp(original+delta,lower,upper),1);
}
)";

static ComPtr<ID3DBlob> compile(const char* entry, const char* profile) {
    ComPtr<ID3DBlob> blob, errors;
    auto status = D3DCompile(shader, std::char_traits<char>::length(shader), "Panevanta NR", nullptr, nullptr,
        entry, profile, D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &blob, &errors);
    if (FAILED(status) && errors) throw std::runtime_error(static_cast<const char*>(errors->GetBufferPointer()));
    check(status, "Compile NR transfer");
    return blob;
}

struct SharedImage {
    ComPtr<ID3D12Resource> d12;
    ComPtr<ID3D11Texture2D> d11;
};

struct NrOptions {
    float tone = 1.f;
    float structure = 1.f;
    float skin = 1.f;
    UINT style = 0;
    UINT passes = 1;
    int detail = 1;
    int autoMask = 1;
    int uiCorrection = 1;

    bool same(const NrOptions& o) const {
        return tone == o.tone && structure == o.structure && skin == o.skin && style == o.style
            && passes == o.passes && detail == o.detail && autoMask == o.autoMask
            && uiCorrection == o.uiCorrection;
    }
};

struct NrBridge {
    UINT width, height;
    UINT sourceWidth{}, sourceHeight{};
    float white;
    bool displayHdr, details;
    UINT64 lastHistoryTime{};
    NrOptions options;
    ComPtr<ID3D11Device> dev11;
    ComPtr<ID3D11DeviceContext4> ctx;
    ComPtr<ID3D11DeviceContext> drawContext;
    ComPtr<ID3D11Multithread> multithread;
    ComPtr<ID3D12Device> dev12;
    ComPtr<ID3D12CommandQueue> queue;
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> list;
    ComPtr<ID3D12Fence> fence12;
    ComPtr<ID3D11Fence> fence11;
    WinHandle event;
    UINT64 step{}, complete{};
    SharedImage input, output, velocity;
    ComPtr<ID3D12Resource> networkOutput;
    ComPtr<ID3D11Texture2D> composed, captureCopy, previousInput, previousOutput;
    ComPtr<ID3D11RenderTargetView> inputRT, composeRT;
    ComPtr<ID3D11ShaderResourceView> inputSR, outputSR, captureView, previousInputSR, previousOutputSR;
    struct CaptureBinding {
        ID3D11Texture2D* texture{};
        ComPtr<ID3D11ShaderResourceView> view;
    };
    std::array<CaptureBinding, 2> bindings;
    size_t nextBinding{};
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> encode, decode;
    ComPtr<ID3D11SamplerState> sampler;
    ComPtr<ID3D11Buffer> constants;
    NVSDK_NGX_Parameter* params{};
    NVSDK_NGX_Handle* feature{};
    bool ngx{};

    NrBridge(ID3D11Texture2D* source, UINT w, UINT h, float sdrWhite, bool hdr, bool skin,
             const wchar_t* runtime, const wchar_t* cache)
      : width(w), height(h), white(sdrWhite), displayHdr(hdr), details(skin) {
        try { initialize(source, runtime, cache); } catch (...) { shutdown(); throw; }
    }

    SharedImage image(DXGI_FORMAT format) {
        D3D11_TEXTURE2D_DESC desc{};
        desc.Width = width;
        desc.Height = height;
        desc.ArraySize = desc.MipLevels = desc.SampleDesc.Count = 1;
        desc.Format = format;
        desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
        desc.MiscFlags = D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_NTHANDLE;
        SharedImage result;
        check(dev11->CreateTexture2D(&desc,nullptr,&result.d11), "Create shared NR texture");
        ComPtr<IDXGIResource1> resource;
        check(result.d11.As(&resource), "Query NR shared texture");
        WinHandle handle;
        check(resource->CreateSharedHandle(nullptr,DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE,nullptr,&handle.value),"Share NR texture");
        check(dev12->OpenSharedHandle(handle.value,IID_PPV_ARGS(&result.d12)),"Open NR texture in D3D12");
        return result;
    }

    void create_network_output() {
        D3D12_HEAP_PROPERTIES heap{};
        heap.Type = D3D12_HEAP_TYPE_DEFAULT;
        heap.CreationNodeMask = heap.VisibleNodeMask = 1;
        D3D12_RESOURCE_DESC desc{};
        desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
        desc.Width = width;
        desc.Height = height;
        desc.DepthOrArraySize = desc.MipLevels = 1;
        desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
        desc.SampleDesc.Count = 1;
        desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
        check(dev12->CreateCommittedResource(&heap,D3D12_HEAP_FLAG_NONE,&desc,D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            nullptr,IID_PPV_ARGS(&networkOutput)), "Create NR UAV");
    }

    void initialize(ID3D11Texture2D* source, const wchar_t* runtime, const wchar_t* cache) {
        verify_runtime(std::filesystem::path(runtime) / L"nvngx_dlssnr.dll");
        source->GetDevice(&dev11);
        ComPtr<ID3D11DeviceContext> immediate;
        dev11->GetImmediateContext(&immediate);
        check(immediate.As(&ctx), "Query NR D3D11 fence context");
        check(ctx.As(&multithread), "Query NR D3D11 threading");
        multithread->SetMultithreadProtected(TRUE);
        check(dev11->CreateDeferredContext(0,&drawContext), "Create NR deferred draw context");
        ComPtr<IDXGIDevice> dxgi;
        ComPtr<IDXGIAdapter> adapter;
        check(dev11.As(&dxgi), "Query NR DXGI device");
        check(dxgi->GetAdapter(&adapter), "Query NR adapter");
        check(D3D12CreateDevice(adapter.Get(),D3D_FEATURE_LEVEL_12_0,IID_PPV_ARGS(&dev12)), "Create NR D3D12 device");
        D3D12_COMMAND_QUEUE_DESC q{};
        q.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
        check(dev12->CreateCommandQueue(&q,IID_PPV_ARGS(&queue)), "Create NR command queue");
        check(dev12->CreateCommandAllocator(q.Type,IID_PPV_ARGS(&allocator)), "Create NR command allocator");
        check(dev12->CreateCommandList(0,q.Type,allocator.Get(),nullptr,IID_PPV_ARGS(&list)), "Create NR command list");
        check(dev12->CreateFence(0,D3D12_FENCE_FLAG_SHARED,IID_PPV_ARGS(&fence12)), "Create shared NR fence");
        ComPtr<ID3D11Device5> fenceDevice;
        check(dev11.As(&fenceDevice), "Query D3D11 fence device");
        WinHandle handle;
        check(dev12->CreateSharedHandle(fence12.Get(),nullptr,GENERIC_ALL,nullptr,&handle.value), "Share NR fence");
        check(fenceDevice->OpenSharedFence(handle.value,IID_PPV_ARGS(&fence11)), "Open NR fence in D3D11");
        event.value = CreateEventW(nullptr,FALSE,FALSE,nullptr);
        if (!event.value) check(HRESULT_FROM_WIN32(GetLastError()), "Create NR synchronisation event");

        auto vertex = compile("vs","vs_5_0");
        auto toNr = compile("toNr","ps_5_0");
        auto fromNr = compile("fromNr","ps_5_0");
        check(dev11->CreateVertexShader(vertex->GetBufferPointer(),vertex->GetBufferSize(),nullptr,&vs),"Create NR vertex shader");
        check(dev11->CreatePixelShader(toNr->GetBufferPointer(),toNr->GetBufferSize(),nullptr,&encode),"Create NR encoding shader");
        check(dev11->CreatePixelShader(fromNr->GetBufferPointer(),fromNr->GetBufferSize(),nullptr,&decode),"Create NR decoding shader");
        D3D11_SAMPLER_DESC sample{};
        sample.Filter = D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT;
        sample.AddressU = sample.AddressV = sample.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
        sample.MaxLOD = D3D11_FLOAT32_MAX;
        check(dev11->CreateSamplerState(&sample,&sampler),"Create NR sampler");
        D3D11_BUFFER_DESC cb{};
        cb.ByteWidth = 16;
        cb.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
        const float values[4]{white,displayHdr ? 1.f : 0.f,details ? 1.f : 0.f,0.f};
        D3D11_SUBRESOURCE_DATA init{};
        init.pSysMem = values;
        check(dev11->CreateBuffer(&cb,&init,&constants),"Create NR shader options");
        D3D11_TEXTURE2D_DESC result{};
        D3D11_TEXTURE2D_DESC sourceDesc{};
        source->GetDesc(&sourceDesc);
        result.Width = sourceWidth = sourceDesc.Width;
        result.Height = sourceHeight = sourceDesc.Height;
        result.MipLevels = result.ArraySize = result.SampleDesc.Count = 1;
        result.Format = DXGI_FORMAT_R16G16B16A16_FLOAT;
        result.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
        check(dev11->CreateTexture2D(&result,nullptr,&composed),"Create NR composition texture");
        check(dev11->CreateRenderTargetView(composed.Get(),nullptr,&composeRT),"Create NR composition view");

        NVSDK_NGX_FeatureCommonInfo info{};
        info.PathListInfo = {&runtime, 1};
        ngx_check(NVSDK_NGX_D3D12_Init_with_ProjectID("c9e06f8e-e546-4308-a840-d9db35f40c8b",
            NVSDK_NGX_ENGINE_TYPE_CUSTOM,"0.8",cache,dev12.Get(),&info,NVSDK_NGX_Version_API),"Initialize NVIDIA DLSS NR");
        ngx = true;
        ngx_check(NVSDK_NGX_D3D12_AllocateParameters(&params),"Allocate NVIDIA NR parameters");
        setup_resolution();
    }

    void setup_resolution() {
        input = image(DXGI_FORMAT_R8G8B8A8_UNORM);
        output = image(DXGI_FORMAT_R8G8B8A8_UNORM);
        velocity = image(DXGI_FORMAT_R16G16_FLOAT);
        lastHistoryTime = 0;
        D3D11_TEXTURE2D_DESC history{};
        input.d11->GetDesc(&history);
        history.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        history.MiscFlags = 0;
        check(dev11->CreateTexture2D(&history,nullptr,previousInput.ReleaseAndGetAddressOf()),"Create NR history input");
        check(dev11->CreateTexture2D(&history,nullptr,previousOutput.ReleaseAndGetAddressOf()),"Create NR history output");
        check(dev11->CreateShaderResourceView(previousInput.Get(),nullptr,previousInputSR.ReleaseAndGetAddressOf()),"Read NR history input");
        check(dev11->CreateShaderResourceView(previousOutput.Get(),nullptr,previousOutputSR.ReleaseAndGetAddressOf()),"Read NR history output");
        networkOutput.Reset();
        create_network_output();
        check(dev11->CreateRenderTargetView(input.d11.Get(),nullptr,inputRT.ReleaseAndGetAddressOf()), "Create NR input view");
        check(dev11->CreateShaderResourceView(input.d11.Get(),nullptr,inputSR.ReleaseAndGetAddressOf()), "Create NR input sampling view");
        check(dev11->CreateShaderResourceView(output.d11.Get(),nullptr,outputSR.ReleaseAndGetAddressOf()), "Create NR output sampling view");
        ComPtr<ID3D11RenderTargetView> motionView;
        check(dev11->CreateRenderTargetView(velocity.d11.Get(),nullptr,&motionView), "Create NR motion view");
        const float zero[4]{};
        ctx->ClearRenderTargetView(motionView.Get(),zero);
        params->Reset();
        for (const char* key : {"Width","OutWidth","DLSSNR.Width","DLSSNR.InputWidth","DLSSNR.OutputWidth"}) params->Set(key,width);
        for (const char* key : {"Height","OutHeight","DLSSNR.Height","DLSSNR.InputHeight","DLSSNR.OutputHeight"}) params->Set(key,height);
        params->Set("CreationNodeMask",1u);
        params->Set("VisibilityNodeMask",1u);
        params->Set("DLSSNR.Hint.Render.Preset",1u);
        ngx_check(NVSDK_NGX_D3D12_CreateFeature(list.Get(),NVSDK_NGX_Feature_Reserved18,params,&feature),"Create DLSS NR preset A");
        submit();
        wait(complete);
    }

    void resize(UINT w, UINT h) {
        wait(complete);
        if (feature) {
            ngx_check(NVSDK_NGX_D3D12_ReleaseFeature(feature),"Release previous NR feature");
            feature=nullptr;
        }
        width=w;
        height=h;
        check(allocator->Reset(),"Reset NR resize allocator");
        check(list->Reset(allocator.Get(),nullptr),"Reset NR resize command list");
        setup_resolution();
    }

    void wait(UINT64 target) {
        const auto completed = fence12->GetCompletedValue();
        if (completed == UINT64_MAX) check(dev12->GetDeviceRemovedReason(),"NR device removed");
        if (completed >= target) return;
        check(fence12->SetEventOnCompletion(target,event.value),"Register NR fence event");
        if (WaitForSingleObject(event.value,2500) != WAIT_OBJECT_0) throw std::runtime_error("DLSS NR timed out waiting for the GPU");
    }

    void submit() {
        check(list->Close(),"Close NR command list");
        ID3D12CommandList* lists[]{list.Get()};
        queue->ExecuteCommandLists(1,lists);
        const auto signaled = ++step;
        check(queue->Signal(fence12.Get(),signaled),"Signal NR output");
        complete = signaled;
    }

    void transition(ID3D12Resource* resource, D3D12_RESOURCE_STATES before, D3D12_RESOURCE_STATES after) {
        D3D12_RESOURCE_BARRIER b{};
        b.Type=D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        b.Transition={resource,D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,before,after};
        list->ResourceBarrier(1,&b);
    }

    void draw(ID3D11RenderTargetView* target, ID3D11PixelShader* pixel, ID3D11ShaderResourceView** views, UINT count, UINT w, UINT h) {
        struct Guard { ID3D11Multithread* lock; ~Guard() { lock->Leave(); } };
        multithread->Enter();
        const Guard guard{multithread.Get()};
        constexpr D3D11_PRIMITIVE_TOPOLOGY triangle = D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
        const D3D11_VIEWPORT viewport{0,0,float(w),float(h),0,1};
        ctx->RSSetViewports(1,&viewport);
        ctx->IASetInputLayout(nullptr);
        ctx->IASetPrimitiveTopology(triangle);
        ctx->VSSetShader(vs.Get(),nullptr,0);
        ctx->PSSetShader(pixel,nullptr,0);
        ID3D11SamplerState* sampling=sampler.Get();
        ID3D11Buffer* options=constants.Get();
        ctx->PSSetSamplers(0,1,&sampling);
        ctx->PSSetConstantBuffers(0,1,&options);
        ctx->OMSetBlendState(nullptr,nullptr,~0u);
        ctx->OMSetRenderTargets(1,&target,nullptr);
        ctx->PSSetShaderResources(0,count,views);
        ctx->Draw(3,0);
        ID3D11ShaderResourceView* empty[5]{};
        ctx->PSSetShaderResources(0,count,empty);
        ctx->OMSetRenderTargets(0,nullptr,nullptr);
    }

    ID3D11ShaderResourceView* source_view(ID3D11Texture2D* source) {
        for (const auto& binding : bindings) {
            if (binding.texture == source)
                return binding.view.Get();
        }
        D3D11_TEXTURE2D_DESC desc{};
        source->GetDesc(&desc);
        if (desc.BindFlags & D3D11_BIND_SHADER_RESOURCE) {
            auto& binding = bindings[nextBinding];
            check(dev11->CreateShaderResourceView(source,nullptr,binding.view.ReleaseAndGetAddressOf()),"Create NR source view");
            binding.texture = source;
            nextBinding = (nextBinding + 1) % bindings.size();
            return binding.view.Get();
        }
        if (!captureCopy) {
            desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;
            desc.MiscFlags = 0;
            check(dev11->CreateTexture2D(&desc,nullptr,&captureCopy),"Create NR capture texture");
            check(dev11->CreateShaderResourceView(captureCopy.Get(),nullptr,&captureView),"Make NR capture view");
        }
        ctx->CopyResource(captureCopy.Get(),source);
        return captureView.Get();
    }

    void configure(const NrOptions& value) {
        if (!options.same(value)) lastHistoryTime = 0;
        options = value;
        details = value.detail != 0;
    }

    ID3D11Texture2D* process(ID3D11Texture2D* source) {
        const auto now = GetTickCount64();
        const bool recent = lastHistoryTime && now - lastHistoryTime < 150;
        const float values[4]{white,displayHdr ? 1.f : 0.f,details ? 1.f : 0.f,recent ? 1.f : 0.f};
        ctx->UpdateSubresource(constants.Get(),0,nullptr,values,0,0);
        ID3D11ShaderResourceView* views[]{source_view(source),inputSR.Get(),outputSR.Get(),
                                         previousInputSR.Get(),previousOutputSR.Get()};
        draw(inputRT.Get(),encode.Get(),views,1,width,height);
        check(ctx->Signal(fence11.Get(),++step),"Signal NR input");
        ctx->Flush();
        check(queue->Wait(fence12.Get(),step),"Wait for NR input");
        check(allocator->Reset(),"Reset NR command allocator");
        check(list->Reset(allocator.Get(),nullptr),"Reset NR command list");
        constexpr auto read = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
        transition(input.d12.Get(),D3D12_RESOURCE_STATE_COMMON,read);
        transition(velocity.d12.Get(),D3D12_RESOURCE_STATE_COMMON,read);
        // Feed the previous result back to NR and keep the original encoded frame for composition.
        const UINT passes = std::clamp(options.passes, 1u, 3u);
        for (UINT pass = 0; pass < passes; ++pass) {
            evaluate(pass == 0 ? input.d12.Get() : output.d12.Get());
            constexpr auto write = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
            transition(networkOutput.Get(),write,D3D12_RESOURCE_STATE_COPY_SOURCE);
            transition(output.d12.Get(),pass == 0 ? D3D12_RESOURCE_STATE_COMMON : read,D3D12_RESOURCE_STATE_COPY_DEST);
            list->CopyResource(output.d12.Get(),networkOutput.Get());
            transition(output.d12.Get(),D3D12_RESOURCE_STATE_COPY_DEST,pass + 1 < passes ? read : D3D12_RESOURCE_STATE_COMMON);
            transition(networkOutput.Get(),D3D12_RESOURCE_STATE_COPY_SOURCE,write);
        }
        transition(input.d12.Get(),read,D3D12_RESOURCE_STATE_COMMON);
        transition(velocity.d12.Get(),read,D3D12_RESOURCE_STATE_COMMON);
        submit();
        // Finish evaluation before exposing the texture. Do not stall a shared D3D11 queue on a failed NR command.
        wait(complete);
        draw(composeRT.Get(),decode.Get(),views,5,sourceWidth,sourceHeight);
        ctx->CopyResource(previousInput.Get(),input.d11.Get());
        ctx->CopyResource(previousOutput.Get(),output.d11.Get());
        lastHistoryTime = now;
        return composed.Get();
    }

    void evaluate(ID3D12Resource* color) {
        params->Reset();
        params->Set("DLSSNR.Color",color);
        params->Set("DLSSNR.Output",networkOutput.Get());
        params->Set("DLSSNR.MVec",velocity.d12.Get());
        for (const char* key : {"DLSSNR.ColorSubrectWidth","DLSSNR.OutputSubrectWidth","DLSSNR.MVecSubrectWidth"}) params->Set(key,width);
        for (const char* key : {"DLSSNR.ColorSubrectHeight","DLSSNR.OutputSubrectHeight","DLSSNR.MVecSubrectHeight"}) params->Set(key,height);
        params->Set("DLSSNR.MVecScaleX",1.f);
        params->Set("DLSSNR.MVecScaleY",1.f);
        params->Set("DLSSNR.Enabled",1u);
        // Window capture has no engine motion vectors. Do not reuse misaligned history.
        params->Set("DLSSNR.Reset",1u);
        params->Set("DLSSNR.Intensity",1.f);
        params->Set("DLSSNR.Style",options.style);
        params->Set("DLSSNR.LocalToneStrength",options.tone);
        params->Set("DLSSNR.LocalStructureStrength",details ? options.structure : 0.f);
        params->Set("DLSSNR.SkinStructureStrength",details ? options.skin : 0.f);
        params->Set("DLSSNR.UseAutoMask",details && options.autoMask ? 1u : 0u);
        params->Set("DLSSNR.UICorrection",options.uiCorrection ? 1u : 0u);
        ngx_check(NVSDK_NGX_D3D12_EvaluateFeature(list.Get(),feature,params,nullptr),"Evaluate DLSS NR");
    }

    void shutdown() {
        if (complete) {
            try { wait(complete); } catch (...) {}
        }
        if (feature) NVSDK_NGX_D3D12_ReleaseFeature(feature);
        if (params) NVSDK_NGX_D3D12_DestroyParameters(params);
        if (ngx) NVSDK_NGX_D3D12_Shutdown1(dev12.Get());
        feature=nullptr; params=nullptr; ngx=false;
    }

    ~NrBridge() { shutdown(); }
};

static void error(char* buffer, size_t size, const std::exception& exception) {
    if (buffer && size) std::snprintf(buffer,size,"%s",exception.what());
}
extern "C" __declspec(dllexport) void* PaneNrCreate(ID3D11Texture2D* source, UINT width, UINT height,
    float white, int hdr, int details, const wchar_t* runtime, const wchar_t* cache, char* message, size_t size) {
    try { return new NrBridge(source,width,height,white,hdr != 0,details != 0,runtime,cache); }
    catch (const std::exception& ex) { error(message,size,ex); return nullptr; }
}
extern "C" __declspec(dllexport) ID3D11Texture2D* PaneNrProcess(NrBridge* bridge,ID3D11Texture2D* source,char* message,size_t size) {
    try { auto* texture=bridge->process(source); texture->AddRef(); return texture; }
    catch (const std::exception& ex) { error(message,size,ex); return nullptr; }
}
extern "C" __declspec(dllexport) void PaneNrDestroy(NrBridge* bridge) { delete bridge; }
extern "C" __declspec(dllexport) void PaneNrResetHistory(NrBridge* bridge) { bridge->lastHistoryTime = 0; }
extern "C" __declspec(dllexport) void PaneNrConfigureV2(NrBridge* bridge, const NrOptions* options) {
    bridge->configure(*options);
}
extern "C" __declspec(dllexport) int PaneNrResize(NrBridge* bridge, UINT width, UINT height, char* message, size_t size) {
    try { bridge->resize(width,height); return 1; }
    catch (const std::exception& exception) { error(message,size,exception); return 0; }
}
