using System.Diagnostics;
using Windows.Graphics.Capture;
using Windows.Graphics.DirectX;

namespace Veyra;

internal sealed record Source(string Name, nint Handle, Screen? Screen)
{
    public Rectangle Bounds => Screen?.Bounds ?? Native.Bounds(Handle);
    public override string ToString() => Name;
}

internal sealed record Effects(int Multiplier, string Quality, bool Sr, bool Hdr, int Fps, string Scale, string Peak, float WhiteScale, int LimitHeight, bool DisplayHdr)
{
    public bool PreserveHdr => DisplayHdr && !Sr && !Hdr;


}

internal sealed class Overlay : Form
{
    protected override bool ShowWithoutActivation => true;
    protected override CreateParams CreateParams
    {
        get
        {
            var p = base.CreateParams;
            p.ExStyle |= 0x08000000 | 0x80 | 0x20 | 0x80000;
            return p;
        }
    }

    protected override void WndProc(ref Message message)
    {
        if (message.Msg == 0x84) { message.Result = -1; return; }
        if (message.Msg == 0x21) { message.Result = 3; return; }
        base.WndProc(ref message);
    }

    public Overlay(Rectangle bounds)
    {
        Bounds = bounds;
        StartPosition = FormStartPosition.Manual;
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        Enabled = false;
        BackColor = Color.Black;
        TopMost = true;
    }
}

internal sealed class SourceSizeChanged(int width, int height) : Exception("Источник изменил размер.")
{
    public Size Dimensions { get; } = new(width, height);
}

internal sealed class Pipeline
{
    private readonly CancellationTokenSource stop = new();
    public readonly Overlay Overlay;
    public readonly Task Capture;
    public readonly string Log;
    private readonly long[] sequences = new long[1024];
    private readonly long[] captureTimes = new long[1024];
    private long captured;
    public string Transfer { get; private set; } = "";
    public int ProcessId => Environment.ProcessId;
    public int Width { get; }
    public int Height { get; }
    public long CaptureTime(long sequence)
    {
        int slot = (int)(sequence & 1023);
        return Volatile.Read(ref sequences[slot]) == sequence ? Volatile.Read(ref captureTimes[slot]) : 0;
    }
    public long Captured => Interlocked.Read(ref captured);

    public Pipeline(Source source, Effects effects, string root, string log, Size? liveSize = null)
    {
        var item = Native.CaptureItem(source);
        var size = liveSize is Size current ? new Windows.Graphics.SizeInt32 { Width = current.Width, Height = current.Height } : item.Size;
        if (size.Width < 16 || size.Height < 16 || size.Width > 8192 || size.Height > 8192)
            throw new InvalidOperationException("Неподдерживаемый размер источника.");
        Height = effects.LimitHeight > 0 ? Math.Min(size.Height, effects.LimitHeight) : size.Height;
        Width = Height == size.Height ? size.Width : Math.Max(16, ((int)Math.Round((double)size.Width * Height / size.Height)) & ~1);
        Log = log;
        Overlay = new Overlay(source.Bounds);
        var handle = Overlay.Handle;
        if (!Native.SetLayeredWindowAttributes(handle, 0, 255, 2) || !Native.SetWindowDisplayAffinity(handle, 0x11))
        {
            Overlay.Dispose();
            throw new InvalidOperationException("Windows не удалось исключить слой вывода из захвата.");
        }
        var surface = new Panel { Dock = DockStyle.Fill, Enabled = false, BackColor = Color.Black };
        Overlay.Controls.Add(surface);
        var renderWindow = surface.Handle;
        Capture = Task.Factory.StartNew(() => Pump(item, size, effects, source.Screen is null,
            renderWindow, root, stop.Token), CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
    }

    private void Pump(GraphicsCaptureItem item, Windows.Graphics.SizeInt32 size, Effects effects, bool eventCapture, nint window, string root, CancellationToken cancellation)
    {
        if (!GraphicsCaptureSession.IsSupported())
            throw new InvalidOperationException("Захват Windows Graphics Capture недоступен.");
        using var device = Native.CaptureDevice(videoSupport: true);
        using var pool = Direct3D11CaptureFramePool.CreateFreeThreaded(device, DirectXPixelFormat.R16G16B16A16Float, 2, size);
        using var session = pool.CreateCaptureSession(item);
        session.IsCursorCaptureEnabled = false;
        using var pending = eventCapture ? new AutoResetEvent(false) : null;
        WaitHandle[]? frameWait = pending is null ? null : [cancellation.WaitHandle, pending];
        if (pending is not null) pool.FrameArrived += OnFrame;
        using var timer = eventCapture ? null : new CaptureClock(effects.Fps, cancellation);
        NativeEngine? renderer = null;
        long firstSequence = 0, lastFrameTime = 0, captureAnchor = 0, eventSequence = 0;
        session.StartCapture();
        try
        {
            while (Next(out long tick))
            {
                using var newest = Latest(pool);
                if (newest is null)
                {
                    renderer?.Submit(null, 0);
                    continue;
                }
                if (newest.ContentSize.Width != size.Width || newest.ContentSize.Height != size.Height)
                    throw new SourceSizeChanged(newest.ContentSize.Width, newest.ContentSize.Height);
                long now = Stopwatch.GetTimestamp();
                long frameTime = (long)(newest.SystemRelativeTime.TotalSeconds * Stopwatch.Frequency);
                if (frameTime <= lastFrameTime || now - frameTime > Stopwatch.Frequency / 5) continue;
                if (eventCapture)
                {
                    if (captureAnchor == 0) captureAnchor = frameTime;
                    tick = 1 + (long)Math.Round((frameTime - captureAnchor) * (double)effects.Fps / Stopwatch.Frequency);
                    if (tick <= eventSequence) continue;
                    eventSequence = tick;
                }
                if (firstSequence == 0) firstSequence = tick;
                long sequence = tick - firstSequence + 1;

                if (renderer is null)
                {
                    renderer = new NativeEngine(newest.Surface, window, effects, Width, Height, root, Log);
                    Transfer = effects.Hdr || effects.PreserveHdr ? "D3D11/CUDA HDR10" : "D3D11/CUDA";
                    if (Stopwatch.GetTimestamp() - frameTime > Stopwatch.Frequency / 5) continue;
                }
                int captureSlot = (int)(sequence & 1023);
                Volatile.Write(ref captureTimes[captureSlot], frameTime);
                Volatile.Write(ref sequences[captureSlot], sequence);
                renderer.Submit(newest.Surface, sequence, frameTime);
                lastFrameTime = frameTime;

                Interlocked.Increment(ref captured);
            }
        }
        finally
        {
            if (pending is not null) pool.FrameArrived -= OnFrame;
            renderer?.Dispose();
        }

        bool Next(out long tick)
        {
            if (timer is not null) return timer.Next(out tick);
            tick = 0;
            // A timeout also checks the renderer when the source stops sending frames.
            return WaitHandle.WaitAny(frameWait!, 150) != 0;
        }

        void OnFrame(Direct3D11CaptureFramePool sender, object args)
        {
            try { pending!.Set(); }
            catch (ObjectDisposedException) { }
        }
    }

    internal static Direct3D11CaptureFrame? Latest(Direct3D11CaptureFramePool pool)
    {
        var frame = pool.TryGetNextFrame();
        var next = pool.TryGetNextFrame();
        if (next is null) return frame;
        frame?.Dispose();
        return next;
    }

    public async Task Close()
    {
        stop.Cancel();
        Overlay.Hide();
        try { await Capture; }
        catch (Exception) when (stop.IsCancellationRequested) { }
        Overlay.Dispose();
        stop.Dispose();
    }
}
