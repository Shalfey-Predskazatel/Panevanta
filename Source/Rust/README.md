# Panevanta 0.9.7 source and build

**English** · [Русский](README.ru.md)

The current Windows application uses Rust and Tauri with a React interface. Rust manages window and display capture, settings, shortcuts, and the streamer output window. The native library in `../Native` provides the Direct3D 11, CUDA, and NVIDIA Video Effects processing engine.

## Build the application

Use Windows x64 with the MSVC Rust target, MSVC Build Tools and Windows SDK, and Node.js 22.12 or newer. From an x64 Native Tools Command Prompt or Developer PowerShell, build the interface and application:

```powershell
cd Source/Rust/frontend
npm ci
npm run build
cd ..
cargo build --release --locked --target x86_64-pc-windows-msvc
```

Rebuilding the native GPU engine requires MSYS2 CLANG64, CUDA headers, and the NVIDIA Video Effects SDK. The separate DLSS 5 NR bridge and model require their corresponding NVIDIA SDK and runtime files. The public source repository does not contain the complete offline toolchain or all licensed SDK payloads.

The portable release is available on the [release page](https://github.com/Shalfey-Predskazatel/Panevanta/releases/latest). For detailed Russian instructions covering the portable layout, effects, DLSS 5 NR, auto-tuning, and OBS output, see [README.ru.md](README.ru.md).

