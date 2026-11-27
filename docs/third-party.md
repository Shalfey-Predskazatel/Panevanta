# Third-party components

**English** · [Русский](third-party.ru.md)

Panevanta 0.9.7 combines original project code with components from NVIDIA, Microsoft, and open-source projects. The portable release includes third-party notices in its `Licenses` folder.

- NVIDIA Video Effects SDK, VFG, CUDA, and related redistributables remain subject to NVIDIA terms.
- The DLSS 5 NR bridge uses the NVIDIA NGX loader and a signed NVIDIA Neural Rendering model. SDK and model files retain their own license terms.
- Microsoft Edge WebView2 and the Microsoft Visual C++ runtime are subject to Microsoft terms. The portable package may include an optional Fixed Version WebView2 runtime.
- The desktop shell uses Tauri and `windows-rs` under MIT / Apache-2.0. The interface uses React, Tailwind CSS, Radix Primitives, and related components under their respective open-source licenses. Lucide icons use ISC terms and IBM Plex Sans uses SIL Open Font License 1.1.

The detailed component list and references are in [Source/Rust/THIRD-PARTY.md](../Source/Rust/THIRD-PARTY.md). Dependency versions are locked in `Source/Rust/Cargo.lock` and `Source/Rust/frontend/package-lock.json`.

The MIT License in this repository covers original Panevanta source code and build scripts only. It does not grant rights to product branding, artwork, NVIDIA or Microsoft components, SDKs, models, or other third-party materials. Check the terms shipped with each component before redistributing it.


