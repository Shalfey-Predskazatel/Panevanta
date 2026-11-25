![Panevanta by Shalfey Predskazatel](assets/social-preview.png)

# Panevanta

**English** · [Русский](README.ru.md)

**A portable Windows tool that adds NVIDIA frame generation, RTX Video Super Resolution, and HDR to a selected window or display.**

[Download Panevanta 0.7 for Windows x64](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Portable-win-x64.zip) · [Source code](https://github.com/Shalfey-Predskazatel/Panevanta/tree/main/Source) · [Source archive](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip) · [Release and SHA-256](https://github.com/Shalfey-Predskazatel/Panevanta/releases/tag/v0.7)

Panevanta captures a selected window or display and shows the processed image in a click-through overlay. The player or browser continues to decode video and play audio.

- NVIDIA Video Frame Generation, RTX Video Super Resolution, and RTX Video HDR.
- Windows Graphics Capture, Direct3D 11, and CUDA processing.
- Requires a compatible NVIDIA RTX GPU and GeForce driver for the selected effects.
- The portable release includes the runtime files, third-party notices, and Russian quick-start guide.

Extract the complete portable ZIP and run `Program/Panevanta.exe`. Select a window or display and enable RTX. The default shortcut is **Shift + Alt + S**.

## Source and build

This repository includes the Panevanta 0.7 application sources, native engine sources, build scripts, and documentation. Browse [Source](Source/) or download the [source archive](https://github.com/Shalfey-Predskazatel/Panevanta/releases/download/v0.7/Panevanta-0.7-Source.zip).

The 3.1 GB offline toolchain and SDK cache from the recovery bundle is not stored in GitHub. A rebuild needs the pinned .NET toolchain, MSYS2 CLANG64, CUDA headers, and a properly licensed NVIDIA Video Effects SDK. See [Building](docs/building.md) and [third-party notes](docs/third-party.md).

## License

Original Panevanta source code and build scripts are licensed under the [MIT License](LICENSE). The license does not cover product names, logos, artwork, or third-party components. NVIDIA, Microsoft, .NET, CUDA, and other bundled or required components remain subject to their own terms; see [third-party notes](docs/third-party.md).

## Notes

The overlay does not decode video or handle audio. Frame rate shown by the app represents capture or DXGI presentation, not necessarily the display's physical refresh rate or audio synchronization.

Panevanta is an independent project and is not affiliated with or endorsed by NVIDIA or Microsoft. Product and company names identify compatible technologies.
