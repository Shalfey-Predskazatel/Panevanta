using Microsoft.Win32.SafeHandles;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Veyra;

internal sealed class CaptureClock : IDisposable
{
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeWaitHandle CreateWaitableTimerEx(nint attrs, string? name, uint flags, uint access);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetWaitableTimer(SafeWaitHandle timer, ref long due, int interval, nint callback, nint context, bool resume);

    private readonly EventWaitHandle timer = new(false, EventResetMode.AutoReset);
    private readonly WaitHandle[] wait;
    private readonly long start = Stopwatch.GetTimestamp();
    private readonly int rate;
    private long step;

    public CaptureClock(int rate, CancellationToken cancellation)
    {
        this.rate = rate;
        var highResolution = CreateWaitableTimerEx(0, null, 2, 0x100002);
        if (highResolution.IsInvalid)
        {
            highResolution.Dispose();
            highResolution = CreateWaitableTimerEx(0, null, 0, 0x100002);
        }
        if (highResolution.IsInvalid)
        {
            highResolution.Dispose();
            timer.Dispose();
            throw new System.ComponentModel.Win32Exception();
        }
        var original = timer.SafeWaitHandle;
        timer.SafeWaitHandle = highResolution;
        original.Dispose();
        wait = [cancellation.WaitHandle, timer];
    }

    public bool Next(out long sequence)
    {
        long now = Stopwatch.GetTimestamp();
        step = Math.Max(step + 1, (long)Math.Floor((now - start) * (double)rate / Stopwatch.Frequency));
        long until = start + step * Stopwatch.Frequency / rate - now;
        long delay = -Math.Max(1, (long)(until * (10_000_000.0 / Stopwatch.Frequency)));
        if (!SetWaitableTimer(timer.SafeWaitHandle, ref delay, 0, 0, 0, false))
            throw new System.ComponentModel.Win32Exception();
        sequence = step;
        return WaitHandle.WaitAny(wait) == 1;
    }

    public void Dispose() => timer.Dispose();
}
