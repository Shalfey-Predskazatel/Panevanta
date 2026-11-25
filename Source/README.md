# Panevanta source

**English** · [Русский](README.ru.md)

This folder contains the Panevanta 0.7.0 managed application, native D3D11/CUDA engine, optional interop probe, build script, and app assets.

- Panevanta: the Windows desktop application.
- Native: the D3D11, CUDA, RTX Video, and presentation engine.
- Interop: a separate D3D12/CUDA experiment; it is not used by the released application.
- Build-Panevanta.ps1: builds from a complete recovery bundle with the required dependency folders.

The public repository does not contain the offline toolchain archive or NVIDIA SDK payload. Read [the build guide](../docs/building.md) before attempting a rebuild.

The original Panevanta source code and build scripts are licensed under the [MIT License](../LICENSE). Product branding and third-party components are excluded; see [third-party notes](../docs/third-party.md).
