using System.Runtime.InteropServices;
using Windows.Graphics.Capture;
using Windows.Graphics.DirectX;

namespace Veyra;

internal static partial class Native
{
    [DllImport("user32.dll")] private static extern int GetDisplayConfigBufferSizes(uint flags, out uint paths, out uint modes);
    [DllImport("user32.dll")] private static extern unsafe int QueryDisplayConfig(uint flags, ref uint paths, byte* path, ref uint modes, byte* mode, nint topology);
    [DllImport("user32.dll")] private static extern unsafe int DisplayConfigGetDeviceInfo(byte* packet);

    internal static unsafe (float WhiteScale, double Hz, bool Hdr) DisplaySettings(string name)
    {
        if (GetDisplayConfigBufferSizes(2, out uint paths, out uint modes) != 0 || paths == 0)
            return (1, 60, false);
        var pathData = new byte[checked((int)paths * 72)];
        var modeData = new byte[checked((int)modes * 64)];
        fixed (byte* path = pathData, mode = modeData)
        {
            if (QueryDisplayConfig(2, ref paths, path, ref modes, mode, 0) != 0) return (1, 60, false);
            byte* packet = stackalloc byte[84];
            for (int i = 0; i < paths; i++)
            {
                var current = path + i * 72;
                Packet(packet, 1, 84, current);
                if (DisplayConfigGetDeviceInfo(packet) != 0 || !string.Equals(new string((char*)(packet + 20)), name, StringComparison.OrdinalIgnoreCase))
                    continue;
                uint numerator = *(uint*)(current + 48), denominator = *(uint*)(current + 52);
                double hz = denominator == 0 ? 60 : (double)numerator / denominator;
                Packet(packet, 11, 24, current + 20);
                var white = DisplayConfigGetDeviceInfo(packet) == 0 ? *(uint*)(packet + 20) : 1000u;
                Packet(packet, 15, 36, current + 20);
                bool hdr;
                if (DisplayConfigGetDeviceInfo(packet) == 0)
                    hdr = *(uint*)(packet + 32) == 2;
                else
                {
                    // Windows 10 uses the original advanced-color HDR status packet.
                    Packet(packet, 9, 32, current + 20);
                    hdr = DisplayConfigGetDeviceInfo(packet) == 0 && (*(uint*)(packet + 20) & 2) != 0;
                }
                return (Math.Clamp(white / 1000f, 1, 6), hz is >= 24 and <= 1000 ? hz : 60, hdr);
            }
        }
        return (1, 60, false);
    }

    private static unsafe void Packet(byte* packet, int type, int size, byte* pathEndpoint)
    {
        new Span<byte>(packet, 84).Clear();
        *(int*)packet = type;
        *(int*)(packet + 4) = size;
        new ReadOnlySpan<byte>(pathEndpoint, 12).CopyTo(new Span<byte>(packet + 8, 12));
    }
}

internal sealed class Cadence : IDisposable
{
    private readonly CancellationTokenSource stop = new();
    private readonly Task probe;
    private int rate;
    public int Rate => Volatile.Read(ref rate);

    public Cadence(Source source)
    {
        var item = Native.CaptureItem(source);
        probe = Task.Run(() => Measure(item, stop.Token));
    }

    private async Task Measure(GraphicsCaptureItem item, CancellationToken token)
    {
        using var device = Native.CaptureDevice();
        var size = item.Size;
        using var pool = Direct3D11CaptureFramePool.CreateFreeThreaded(device, DirectXPixelFormat.R16G16B16A16Float, 2, size);
        using var session = pool.CreateCaptureSession(item);
        session.IsCursorCaptureEnabled = false;
        using var pending = new SemaphoreSlim(0, 1);
        int width = Math.Min(192, size.Width) & ~1, height = Math.Min(144, size.Height) & ~1;
        var previous = new ushort[width * height / 4];
        var matches = new int[] { 24, 25, 30, 48, 50, 60, 72, 75, 90, 100, 120, 144, 165, 180 };
        FrameReader? reader = null;
        int changes = 0, candidate = 0, repeat = 0;
        double start = -1, lastChange = -1, mouseAt = double.NegativeInfinity;
        Point mouse = Cursor.Position;
        var intervals = new List<double>(240);
        pool.FrameArrived += OnFrame;
        session.StartCapture();
        try
        {
            while (true)
            {
                await pending.WaitAsync(token);
                using var frame = Pipeline.Latest(pool);
                if (frame is null || frame.ContentSize.Width != size.Width || frame.ContentSize.Height != size.Height) continue;
                reader ??= new FrameReader(frame.Surface, (size.Width - width) / 2, (size.Height - height) / 2, width, height);
                double now = frame.SystemRelativeTime.TotalSeconds;
                var currentMouse = Cursor.Position;
                if (currentMouse != mouse)
                {
                    mouse = currentMouse;
                    mouseAt = now;
                    repeat = 0;
                }
                if (now - mouseAt < 1.25)
                {
                    start = lastChange = -1;
                    changes = 0;
                    intervals.Clear();
                    continue;
                }
                if (reader.Difference(frame.Surface, previous) >= 0.004f)
                {
                    changes++;
                    if (lastChange >= 0 && now > lastChange) intervals.Add(now - lastChange);
                    lastChange = now;
                }
                if (start < 0) start = now;
                double elapsed = now - start;
                if (elapsed < 2) continue;
                double measured = changes / elapsed;
                if (intervals.Count >= 12)
                {
                    intervals.Sort();
                    double middle = intervals[intervals.Count / 2];
                    double typical = 1 / middle;
                    if (typical > measured * 1.10 && typical < measured * 1.25 && measured >= 35) measured = typical;
                }
                intervals.Clear();
                int best = matches.MinBy(value => Math.Abs(value - measured));
                if (Math.Abs(best - measured) / best <= 0.085)
                {
                    repeat = best == candidate ? repeat + 1 : 1;
                    candidate = best;
                    if (repeat >= 2)
                    {
                        // Hold the first stable cadence until the user restarts capture.
                        // End the extra capture session to free resources for VFG.
                        Volatile.Write(ref rate, candidate);
                        return;
                    }
                }
                else repeat = 0;
                changes = 0;
                start = now;
            }
        }
        finally
        {
            pool.FrameArrived -= OnFrame;
            reader?.Dispose();
        }

        void OnFrame(Direct3D11CaptureFramePool sender, object args)
        {
            try { if (pending.CurrentCount == 0) pending.Release(); }
            catch (ObjectDisposedException) { }
            catch (SemaphoreFullException) { }
        }
    }

    public void Dispose()
    {
        stop.Cancel();
        _ = probe.ContinueWith(task =>
        {
            _ = task.Exception;
            stop.Dispose();
        }, TaskScheduler.Default);
    }

    // Allow for refresh rates reported slightly below a nominal integer (e.g. 179.82 Hz).
    public static int Multiplier(int fps, double hz) =>
        Math.Clamp((int)Math.Floor(hz / fps + 0.012), 1, 8);
}






