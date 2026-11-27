# Panevanta 0.9.7

[English](RELEASE-NOTES-0.9.7.md) · [Русский](RELEASE-NOTES-0.9.7.ru.md)

Panevanta 0.9.7 is a portable Windows x64 release for applying NVIDIA RTX effects to a selected window or display.

## Highlights

- Adds DLSS 5 Neural Rendering with selectable styles, processing resolutions, pass count, and face and texture enhancement mode.
- Adds a streamer mode with a separate resizable output window for OBS capture.
- Adds automatic source frame-rate estimation, an automatic frame-generation multiplier, and measurement-based auto-tuning for the current scene.
- Adds compact controls, tray actions, and saved settings while preserving live changes to VFG, RTX Video Super Resolution, HDR, and DLSS 5 NR.
- Includes the current Rust and Tauri application source, React interface, native GPU engine, and source/build documentation.

## Download and run

Download `Panevanta-0.9.7-Portable-win-x64.zip`, extract the full archive, and run `Program/Panevanta.exe`. The default capture shortcut is **Shift + Alt + S**. The portable archive also contains third-party notices and the Russian quick-start guide.

The NVIDIA effects require a compatible RTX GPU and GeForce driver. Actual output depends on the source, display mode, driver, and selected effects. The included Russian QA document records the release checks and their limits.

## Files

- `Panevanta-0.9.7-Portable-win-x64.zip` — portable Windows x64 application.
- `Panevanta-0.9.7-Source.zip` — source files and documentation.
- `.sha256` files — SHA-256 checksums for both archives.

Original Panevanta code and build scripts use the MIT License. NVIDIA SDKs, models, runtimes, and other third-party components remain under their own terms. See the source archive's third-party notices.
