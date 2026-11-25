using System.Globalization;
using System.Runtime.InteropServices;
using Windows.Graphics.DirectX.Direct3D11;

namespace Veyra;

internal sealed unsafe class NativeEngine : IDisposable
{
    [StructLayout(LayoutKind.Sequential)]
    private struct EngineSettings
    {
        public int Width, Height, Fps, Factor, Quality, Sr, Hdr, DisplayHdr, Peak;
        public float White, Scale;
    }

    [DllImport("Panevanta.Native.dll", CharSet = CharSet.Unicode)]
    private static extern nint PaneCreate(nint texture, nint window, in EngineSettings settings,
        string sdk, string log, byte* message, nuint length);
    [DllImport("Panevanta.Native.dll")]
    private static extern int PaneSubmit(nint engine, nint texture, long sequence, long captured,
        byte* message, nuint length);
    [DllImport("Panevanta.Native.dll")]
    private static extern void PaneDestroy(nint engine);

    private readonly nint handle;

    public NativeEngine(IDirect3DSurface first, nint window, Effects effects, int width, int height, string root, string log)
    {
        var settings = new EngineSettings
        {
            Width = width,
            Height = height,
            Fps = effects.Fps,
            Factor = effects.Multiplier,
            Quality = effects.Quality switch { "low" => 0, "high" => 2, _ => 1 },
            Sr = effects.Sr ? 1 : 0,
            Hdr = effects.Hdr ? 1 : 0,
            DisplayHdr = effects.DisplayHdr ? 1 : 0,
            Peak = int.TryParse(effects.Peak, out int peak) ? peak : 0,
            White = effects.WhiteScale,
            Scale = float.Parse(effects.Scale, CultureInfo.InvariantCulture)
        };
        string sdk = Path.GetFullPath(Path.Combine(root, "..", "Dependencies", "NVIDIA-VideoFX"));
        if (!File.Exists(Path.Combine(sdk, "bin", "NVVideoEffects.dll")))
            sdk = Environment.GetEnvironmentVariable("NV_VIDEO_EFFECTS_PATH") ?? sdk;
        nint texture = FrameReader.OpenTexture(first);
        try
        {
            byte* error = stackalloc byte[2048];
            handle = PaneCreate(texture, window, settings, sdk, log, error, 2048);
            if (handle == 0) throw Error(error);
        }
        finally { Marshal.Release(texture); }
    }

    public void Submit(IDirect3DSurface? surface, long sequence, long captured = 0)
    {
        nint texture = surface is null ? 0 : FrameReader.OpenTexture(surface);
        try
        {
            byte* error = stackalloc byte[2048];
            if (PaneSubmit(handle, texture, sequence, captured, error, 2048) == 0) throw Error(error);
        }
        finally
        {
            if (texture != 0) Marshal.Release(texture);
        }
    }

    private static Exception Error(byte* message) => new InvalidOperationException(
        Marshal.PtrToStringUTF8((nint)message) ?? "Ошибка движка Panevanta.");

    public void Dispose() => PaneDestroy(handle);
}
