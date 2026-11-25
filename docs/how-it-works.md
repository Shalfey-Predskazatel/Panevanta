# How Panevanta 0.7 works

## From capture to output

1. Panevanta selects an NVIDIA adapter and captures window or display textures through Windows Graphics Capture. The source player or browser remains responsible for video decoding and audio.
2. The textures enter Panevanta.Native.dll as Direct3D 11 surfaces. A shader scales and normalizes colors. SDR uses the Windows white-level information; HDR preservation converts scRGB to BT.2020/PQ.
3. When RTX Video Super Resolution or HDR is enabled, Panevanta uses the NVIDIA RTX Video extensions through the D3D11 Video Processor. These effects run on the source frames before interpolation. If Chrome already applies SR and HDR, leave only VFG enabled in Panevanta.
4. Direct3D 11 buffers are registered with CUDA. NVIDIA VFG reads two source frames and writes intermediate frames into GPU memory. At 4x, it generates three intermediate frames. Processed frames do not pass through a child process or shared system-memory buffer. The automatic cadence detector reads only a small image region.
5. A D3D11 swap chain presents the result over the source window. HDR10 is used for HDR frames on an HDR display. The overlay is excluded from capture and passes clicks through. HDR peak brightness can follow the system or use a manual limit.
6. The queue holds at most one pending frame group. When settings change, the overlay hides, the engine is recreated, and processing resumes with fresh frames. The source video remains visible during the transition. Metrics also detect excessive latency and a stalled output while capture is active.

## Frame rate and metrics

VFG offers manual 2x–6x multipliers and Auto. Manual 4x is the default. Auto selects the largest whole-number multiplier that fits the display refresh rate, up to 8x. Capture rate and multiplier are configured separately.

Capture reports incoming frames. Present reports frames submitted to DXGI. Frame-before-Present measures from the capture timestamp to the presentation call. Neither value alone proves the physical display refresh rate or audio synchronization.

The 1080p limit sets the processing input height. RTX Super Resolution can make the processed output taller than 1080 lines. If performance is limited, reduce VFG quality, processing size, or multiplier.

## Troubleshooting

- If the NVIDIA GPU is not detected, check the GeForce driver in NVIDIA App or Device Manager.
- If VFG does not load, keep Dependencies/NVIDIA-VideoFX beside Program after extracting the portable archive.
- If HDR looks washed out, check Windows HDR and the browser's HDR state. If the browser already applies HDR, turn off SR and HDR in Panevanta and leave VFG enabled.
- The app's Log button opens diagnostics. The current run log is stored in the Windows temporary directory.
