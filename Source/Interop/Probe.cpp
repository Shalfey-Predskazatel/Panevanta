#include <windows.h>
#include <d3d12.h>
#include <dxgi1_4.h>
#include <wrl/client.h>
#include <cuda.h>
#include <nvVideoEffects.h>

#include <cstdint>
#include <chrono>
#include <cstdio>
#include <filesystem>
#include <stdexcept>
#include <string>

using Microsoft::WRL::ComPtr;

static void Check(HRESULT result, const char* operation) {
    if (FAILED(result))
        throw std::runtime_error(std::string(operation) + " HRESULT " + std::to_string(result));
}

static void CheckCuda(CUresult result, const char* operation) {
    if (result != CUDA_SUCCESS)
        throw std::runtime_error(std::string(operation) + " CUDA " + std::to_string(result));
}

template<class T>
T Load(HMODULE module, const char* name) {
    const auto address = GetProcAddress(module, name);
    if (!address) throw std::runtime_error(std::string("Driver export missing: ") + name);
    return reinterpret_cast<T>(address);
}

#define SYMBOL_NAME_IMPL(symbol) #symbol
#define SYMBOL_NAME(symbol) SYMBOL_NAME_IMPL(symbol)
#define CUDA_FN(symbol) const auto symbol##Fn = Load<decltype(&symbol)>(cuda, SYMBOL_NAME(symbol))
#define CUDA_CALL(symbol, ...) CheckCuda(symbol##Fn(__VA_ARGS__), #symbol)

static ComPtr<ID3D12Resource> Buffer(ID3D12Device* device, D3D12_HEAP_TYPE type,
                                      D3D12_HEAP_FLAGS flags, uint64_t size,
                                      D3D12_RESOURCE_STATES state) {
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = type;
    heap.CreationNodeMask = heap.VisibleNodeMask = 1;
    D3D12_RESOURCE_DESC desc{};
    desc.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    desc.Width = size;
    desc.Height = desc.DepthOrArraySize = desc.MipLevels = desc.SampleDesc.Count = 1;
    desc.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    ComPtr<ID3D12Resource> result;
    Check(device->CreateCommittedResource(&heap, flags, &desc, state, nullptr,
                                          IID_PPV_ARGS(&result)), "CreateCommittedResource");
    return result;
}

static HANDLE Share(ID3D12Device* device, ID3D12DeviceChild* resource) {
    HANDLE handle = nullptr;
    Check(device->CreateSharedHandle(resource, nullptr, GENERIC_ALL, nullptr, &handle),
          "CreateSharedHandle");
    return handle;
}

template<class Save, class Sync>
static void RunVfg(const std::filesystem::path& sdk, CUstream stream, CUdeviceptr src, CUdeviceptr next,
                   CUdeviceptr dst, bool hdr, int pairs, NvCVImage (&images)[3], Save save, Sync sync) {
    const auto base = sdk / L"bin";
    const auto feature = sdk / L"features/nvvfxvideoframegeneration/bin";
    if (!SetEnvironmentVariableW(L"NV_VIDEO_EFFECTS_PATH", sdk.c_str()) ||
        !AddDllDirectory(base.c_str()) || !AddDllDirectory(feature.c_str()))
        throw std::runtime_error("Could not register the NVIDIA SDK directories");
    const auto flags = LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS;
    HMODULE core = LoadLibraryExW((base / L"NVVideoEffects.dll").c_str(), nullptr, flags);
    HMODULE image = LoadLibraryExW((base / L"NVCVImage.dll").c_str(), nullptr, flags);
    if (!core || !image) throw std::runtime_error("Could not load the NVIDIA SDK");
    const auto init = Load<decltype(&NvCVImage_Init)>(image, "NvCVImage_Init");
    const auto create = Load<decltype(&NvVFX_CreateEffect)>(core, "NvVFX_CreateEffect");
    const auto load = Load<decltype(&NvVFX_Load)>(core, "NvVFX_Load");
    const auto set = Load<decltype(&NvVFX_SetU32)>(core, "NvVFX_SetU32");
    const auto bind = Load<decltype(&NvVFX_SetImage)>(core, "NvVFX_SetImage");
    const auto setStream = Load<decltype(&NvVFX_SetCudaStream)>(core, "NvVFX_SetCudaStream");
    const auto run = Load<decltype(&NvVFX_Run)>(core, "NvVFX_Run");
    auto check = [](NvCV_Status status, const char* operation) {
        if (status != NVCV_SUCCESS)
            throw std::runtime_error(std::string(operation) + " NvVFX " + std::to_string(status));
    };
    auto makeImage = [&](CUdeviceptr pointer, NvCVImage& result) {
        check(init(&result, 1920, 1080, 1920 * 4, reinterpret_cast<void*>(pointer),
                   hdr ? NVCV_RGB10A2 : NVCV_BGRA, hdr ? NVCV_P32 : NVCV_U8,
                   NVCV_CHUNKY, NVCV_CUDA), "NvCVImage_Init");
    };
    NvVFX_Handle effect{};
    auto& [previous, current, output] = images;
    makeImage(src, previous);
    makeImage(next, current);
    makeImage(dst, output);
    check(create("VideoFrameGeneration", &effect), "NvVFX_CreateEffect");
    check(setStream(effect, NVVFX_CUDA_STREAM, stream), "NvVFX_SetCudaStream");
    check(set(effect, "InputWidth", 1920), "InputWidth");
    check(set(effect, "InputHeight", 1080), "InputHeight");
    check(set(effect, "Mode", 1), "Mode");
    check(set(effect, "AutomaticShotChangeDetectionEnabled", 0), "AutomaticShotChangeDetection");
    check(load(effect), "NvVFX_Load");
    check(set(effect, "FrameMultiplier", 4), "FrameMultiplier");
    check(bind(effect, NVVFX_OUTPUT_IMAGE, &output), "Bind output");
    check(set(effect, "ShotChange", 0), "ShotChange");
    auto pair = [&](bool reverse, bool keep) {
        check(bind(effect, NVVFX_INPUT_IMAGE_0, reverse ? &current : &previous), "Bind previous");
        check(bind(effect, NVVFX_INPUT_IMAGE_1, reverse ? &previous : &current), "Bind current");
        for (unsigned index = 1; index < 4; ++index) {
            check(set(effect, "FrameIndex", index), "FrameIndex");
            check(run(effect, 0), "NvVFX_Run");
            if (keep) save(index);
        }
    };
    if (pairs) {
        pair(false, false);
        sync();
        const auto start = std::chrono::steady_clock::now();
        for (int i = 0; i < pairs; ++i) pair(i % 2 == 0, true);
        sync();
        const double ms = std::chrono::duration<double, std::milli>(
            std::chrono::steady_clock::now() - start).count();
        std::printf("%d synthetic pairs, three outputs per pair: %.2f ms/pair.\n", pairs, ms / pairs);
    } else {
        pair(false, true);
    }
}

// Standalone probe. OS process teardown also releases allocations on any failed step.
int wmain(int argc, wchar_t** argv) {
    try {
        bool hdr = false;
        int pairs = 0;
        for (int i = 2; i < argc; ++i) {
            const std::wstring_view option = argv[i];
            if (option == L"--hdr10") hdr = true;
            else if (option == L"--bench") pairs = 60;
            else throw std::runtime_error("Usage: panevanta-gpu-probe [NVIDIA-SDK-directory [--hdr10] [--bench]]");
        }
        const bool vfg = argc >= 2;
        constexpr uint64_t bytes = 1920ull * 1080 * 4;
        HMODULE cuda = LoadLibraryExW(L"nvcuda.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (!cuda) throw std::runtime_error("NVIDIA CUDA driver is unavailable");
        CUDA_FN(cuInit);
        CUDA_FN(cuDeviceGet);
        CUDA_FN(cuDeviceGetLuid);
        CUDA_FN(cuDevicePrimaryCtxRetain);
        CUDA_FN(cuCtxSetCurrent);
        CUDA_FN(cuStreamCreate);
        CUDA_FN(cuStreamSynchronize);
        CUDA_FN(cuImportExternalMemory);
        CUDA_FN(cuExternalMemoryGetMappedBuffer);
        CUDA_FN(cuImportExternalSemaphore);
        CUDA_FN(cuWaitExternalSemaphoresAsync);
        CUDA_FN(cuMemcpyDtoDAsync);
        CUDA_FN(cuSignalExternalSemaphoresAsync);
        CUDA_CALL(cuInit, 0);
        CUdevice cudaDevice;
        CUDA_CALL(cuDeviceGet, &cudaDevice, 0);
        LUID luid{};
        unsigned int node = 0;
        CUDA_CALL(cuDeviceGetLuid, reinterpret_cast<char*>(&luid), &node, cudaDevice);
        CUcontext context{};
        CUDA_CALL(cuDevicePrimaryCtxRetain, &context, cudaDevice);
        CUDA_CALL(cuCtxSetCurrent, context);
        CUstream stream{};
        CUDA_CALL(cuStreamCreate, &stream, CU_STREAM_NON_BLOCKING);

        ComPtr<IDXGIFactory4> factory;
        ComPtr<IDXGIAdapter1> adapter;
        ComPtr<ID3D12Device> device;
        Check(CreateDXGIFactory1(IID_PPV_ARGS(&factory)), "CreateDXGIFactory1");
        Check(factory->EnumAdapterByLuid(luid, IID_PPV_ARGS(&adapter)), "EnumAdapterByLuid");
        Check(D3D12CreateDevice(adapter.Get(), D3D_FEATURE_LEVEL_11_0, IID_PPV_ARGS(&device)), "D3D12CreateDevice");
        auto source = Buffer(device.Get(), D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_FLAG_SHARED,
                             bytes, D3D12_RESOURCE_STATE_COMMON);
        auto nextSource = vfg ? Buffer(device.Get(), D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_FLAG_SHARED,
                                       bytes, D3D12_RESOURCE_STATE_COMMON) : nullptr;
        const uint64_t targetSize = vfg ? 4 * bytes : bytes;
        const uint64_t readbackSize = vfg ? 3 * bytes : bytes;
        auto target = Buffer(device.Get(), D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_FLAG_SHARED,
                             targetSize, D3D12_RESOURCE_STATE_COMMON);
        auto upload = Buffer(device.Get(), D3D12_HEAP_TYPE_UPLOAD, D3D12_HEAP_FLAG_NONE,
                             vfg ? 2 * bytes : bytes, D3D12_RESOURCE_STATE_GENERIC_READ);
        auto readback = Buffer(device.Get(), D3D12_HEAP_TYPE_READBACK, D3D12_HEAP_FLAG_NONE,
                               readbackSize, D3D12_RESOURCE_STATE_COPY_DEST);

        auto mapCuda = [&](ID3D12Resource* resource) {
            const auto desc = resource->GetDesc();
            const auto allocation = device->GetResourceAllocationInfo(0, 1, &desc);
            CUDA_EXTERNAL_MEMORY_HANDLE_DESC handle{};
            handle.type = CU_EXTERNAL_MEMORY_HANDLE_TYPE_D3D12_RESOURCE;
            handle.handle.win32.handle = Share(device.Get(), resource);
            handle.size = allocation.SizeInBytes;
            handle.flags = CUDA_EXTERNAL_MEMORY_DEDICATED;
            CUexternalMemory memory{};
            const auto imported = cuImportExternalMemoryFn(&memory, &handle);
            CloseHandle(handle.handle.win32.handle);
            CheckCuda(imported, "cuImportExternalMemory");
            CUDA_EXTERNAL_MEMORY_BUFFER_DESC view{};
            view.size = desc.Width;
            CUdeviceptr ptr{};
            CUDA_CALL(cuExternalMemoryGetMappedBuffer, &ptr, memory, &view);
            return ptr;
        };
        const auto src = mapCuda(source.Get());
        const auto next = vfg ? mapCuda(nextSource.Get()) : 0;
        const auto dst = mapCuda(target.Get());
        D3D12_COMMAND_QUEUE_DESC queueDesc{};
        ComPtr<ID3D12CommandQueue> queue;
        ComPtr<ID3D12Fence> fence;
        Check(device->CreateCommandQueue(&queueDesc, IID_PPV_ARGS(&queue)), "CreateCommandQueue");
        Check(device->CreateFence(0, D3D12_FENCE_FLAG_SHARED, IID_PPV_ARGS(&fence)), "CreateFence");
        CUDA_EXTERNAL_SEMAPHORE_HANDLE_DESC handle{};
        handle.type = CU_EXTERNAL_SEMAPHORE_HANDLE_TYPE_D3D12_FENCE;
        handle.handle.win32.handle = Share(device.Get(), fence.Get());
        CUexternalSemaphore semaphore{};
        const auto imported = cuImportExternalSemaphoreFn(&semaphore, &handle);
        CloseHandle(handle.handle.win32.handle);
        CheckCuda(imported, "cuImportExternalSemaphore");

        auto pattern = [](uint64_t i) { return uint8_t((i ^ (i >> 9) ^ 0x73) & 255); };
        void* mapped = nullptr;
        const D3D12_RANGE empty{0, 0};
        Check(upload->Map(0, &empty, &mapped), "Map upload");
        if (vfg) {
            auto* pixels = static_cast<uint32_t*>(mapped);
            for (int frame = 0; frame < 2; ++frame)
                for (int y = 0; y < 1080; ++y)
                    for (int x = 0; x < 1920; ++x) {
                        const int left = 560 + frame * 48;
                        const bool inside = x >= left && x < left + 180 && y >= 360 && y < 720;
                        const uint32_t color = inside ? 180u + ((x - left + y) % 36) : 25u;
                        *pixels++ = hdr ? ((color * 4u) * 0x00100401u | 0xc0000000u)
                                        : (color * 0x00010101u | 0xff000000u);
                    }
        } else {
            auto* data = static_cast<uint8_t*>(mapped);
            for (uint64_t i = 0; i < bytes; ++i) data[i] = pattern(i);
        }
        upload->Unmap(0, nullptr);

        ComPtr<ID3D12CommandAllocator> allocators[2];
        ComPtr<ID3D12GraphicsCommandList> lists[2];
        for (int i = 0; i < 2; ++i) {
            Check(device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                                IID_PPV_ARGS(&allocators[i])), "CreateCommandAllocator");
            Check(device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocators[i].Get(),
                                            nullptr, IID_PPV_ARGS(&lists[i])), "CreateCommandList");
        }
        auto submit = [&](ID3D12GraphicsCommandList* list) {
            Check(list->Close(), "Close command list");
            ID3D12CommandList* commands[]{list};
            queue->ExecuteCommandLists(1, commands);
        };
        lists[0]->CopyBufferRegion(source.Get(), 0, upload.Get(), 0, bytes);
        if (vfg) lists[0]->CopyBufferRegion(nextSource.Get(), 0, upload.Get(), bytes, bytes);
        submit(lists[0].Get());
        Check(queue->Signal(fence.Get(), 1), "Signal D3D12 input");

        CUDA_EXTERNAL_SEMAPHORE_WAIT_PARAMS wait{};
        wait.params.fence.value = 1;
        CUDA_CALL(cuWaitExternalSemaphoresAsync, &semaphore, &wait, 1, stream);
        NvCVImage images[3]{};
        if (vfg) RunVfg(std::filesystem::absolute(argv[1]), stream, src, next, dst, hdr, pairs, images,
                         [&](unsigned index) { CUDA_CALL(cuMemcpyDtoDAsync, dst + bytes * index, dst, bytes, stream); },
                         [&] { CUDA_CALL(cuStreamSynchronize, stream); });
        else CUDA_CALL(cuMemcpyDtoDAsync, dst, src, bytes, stream);
        CUDA_EXTERNAL_SEMAPHORE_SIGNAL_PARAMS signal{};
        signal.params.fence.value = 2;
        CUDA_CALL(cuSignalExternalSemaphoresAsync, &semaphore, &signal, 1, stream);

        Check(queue->Wait(fence.Get(), 2), "Wait for CUDA output");
        lists[1]->CopyBufferRegion(readback.Get(), 0, target.Get(), vfg ? bytes : 0, readbackSize);
        submit(lists[1].Get());
        Check(queue->Signal(fence.Get(), 3), "Signal readback");
        const HANDLE completed = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        if (!completed) throw std::runtime_error("CreateEvent");
        Check(fence->SetEventOnCompletion(3, completed), "SetEventOnCompletion");
        const DWORD result = WaitForSingleObject(completed, 5000);
        CloseHandle(completed);
        if (result != WAIT_OBJECT_0) throw std::runtime_error("GPU fence timeout");

        const D3D12_RANGE entire{0, readbackSize};
        Check(readback->Map(0, &entire, &mapped), "Map readback");
        if (vfg) {
            const auto* pixels = static_cast<const uint32_t*>(mapped);
            for (int frame = 0; frame < 3; ++frame) {
                uint64_t count = 0, xSum = 0;
                for (int y = 0; y < 1080; ++y)
                    for (int x = 0; x < 1920; ++x) {
                        const uint32_t value = *pixels++ & (hdr ? 1023u : 255u);
                        if (value > (hdr ? 480u : 120u)) { ++count; xSum += x; }
                    }
                const double center = count ? static_cast<double>(xSum) / count : 0;
                const double expected = 661.5 + 12 * frame;
                std::printf("Frame %d: center %.2f pixels (expected %.2f)\n", frame + 1, center, expected);
                if (std::abs(center - expected) > 7 || count < 50000 || count > 80000)
                    throw std::runtime_error("Motion does not match the expected intermediate position");
            }
        } else {
            const auto* data = static_cast<const uint8_t*>(mapped);
            for (uint64_t i = 0; i < bytes; ++i)
                if (data[i] != pattern(i)) throw std::runtime_error("GPU data mismatch at " + std::to_string(i));
        }
        readback->Unmap(0, &empty);
        std::printf("PASS: D3D12 -> CUDA%s -> D3D12, 1920x1080, %s.\n",
                    vfg ? " / NVIDIA VFG x4" : "", hdr ? "RGB10A2" : "BGRA8");
        return 0;
    } catch (const std::exception& error) {
        std::fprintf(stderr, "%s\n", error.what());
        return 1;
    }
}
