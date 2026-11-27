# Building Panevanta

**English** · [Русский](building.ru.md)

## Current application 0.9.7

The current Rust and Tauri application is in [`Source/Rust`](../Source/Rust/). Follow the [English build guide](../Source/Rust/README.md) or the [detailed Russian guide](../Source/Rust/README.ru.md). Building the native GPU components requires the applicable NVIDIA SDK files, which are not included in the public source repository.

## Legacy application 0.7

The instructions below apply to the earlier managed application and native engine, not to the current 0.9.7 Rust interface.
The source folder contains the application and native engine. The public Git repository does not include the offline toolchain cache or NVIDIA SDK files from the recovery bundle.

## Required components

- Windows x64.
- .NET SDK 8.0.428.
- MSYS2 CLANG64 with clang and clang++.
- CUDA 12 runtime headers.
- NVIDIA Video Effects SDK files required by the native engine, obtained and used under the applicable NVIDIA license.
- NuGet packages required by the project.

The supplied Build-Panevanta.ps1 expects this directory layout beside the Source folder:

    Dependencies/Build/
    Dependencies/NVIDIA-VideoFX/
    Source/

It extracts the .NET SDK, MSYS2 and CUDA headers into a separate work directory, publishes the self-contained app, and compiles Panevanta.Native.dll. Run it from the root of a complete recovery bundle:

    ./Source/Build-Panevanta.ps1 -WorkDirectory C:/temp/Panevanta-Rebuild

The work directory must be outside the recovery bundle. The resulting Program folder needs a licensed Dependencies/NVIDIA-VideoFX folder beside it to run VFG. The release's portable ZIP includes the runtime files and license notices.

The public source repository omits the dependency bundle intentionally. It cannot run the offline build script on its own. Do not redistribute SDK files unless their license permits it.



