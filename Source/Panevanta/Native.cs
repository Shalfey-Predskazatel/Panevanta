using System.Runtime.InteropServices;
using Windows.Foundation;
using Windows.Graphics.Capture;
using Windows.Graphics.DirectX;
using Windows.Graphics.DirectX.Direct3D11;
using Windows.Graphics.Imaging;
using WinRT;

namespace Veyra;

internal static partial class Native
{
    [DllImport("combase.dll")] private static extern int WindowsCreateString([MarshalAs(UnmanagedType.LPWStr)] string text, int length, out nint value);
    [DllImport("combase.dll")] private static extern int WindowsDeleteString(nint value);
    [DllImport("combase.dll")] private static extern int RoGetActivationFactory(nint name, in Guid iid, out nint factory);
    [DllImport("d3d11.dll")] private static extern int D3D11CreateDevice(nint adapter, int type, nint software, uint flags,
        nint levels, uint numLevels, uint sdk, out nint device, out int feature, out nint context);
    [DllImport("d3d11.dll")] private static extern int CreateDirect3D11DeviceFromDXGIDevice(nint dxgi, out nint device);
    [DllImport("user32.dll")] internal static extern bool SetLayeredWindowAttributes(nint window, uint key, byte alpha, uint flags);
    [DllImport("user32.dll")] private static extern void mouse_event(uint flags, uint dx, uint dy, uint data, nint extra);

    [DllImport("user32.dll")] private static extern bool SetWindowPos(nint window, nint after, int x, int y, int w, int h, uint flags);
    internal static void RaiseOverlay(nint window) => SetWindowPos(window, -1, 0, 0, 0, 0, 0x13);

    [DllImport("winmm.dll")] internal static extern uint timeBeginPeriod(uint period);
    [DllImport("winmm.dll")] internal static extern uint timeEndPeriod(uint period);

    [DllImport("user32.dll")] private static extern void keybd_event(byte key, byte scan, uint flags, nint extra);

    internal static void TestHotkey(uint modifiers, uint key)
    {
        var sequence = new List<byte>();
        if ((modifiers & 4) != 0) sequence.Add(0x10);
        if ((modifiers & 2) != 0) sequence.Add(0x11);
        if ((modifiers & 1) != 0) sequence.Add(0x12);
        sequence.Add((byte)key);
        foreach (byte value in sequence) keybd_event(value, 0, 0, 0);
        for (int i = sequence.Count - 1; i >= 0; i--) keybd_event(sequence[i], 0, 2, 0);
    }

    internal static void TestClick(System.Drawing.Point pos)
    {
        Cursor.Position = pos;
        mouse_event(0x0002, 0, 0, 0, 0);
        mouse_event(0x0004, 0, 0, 0, 0);
    }

    [DllImport("user32.dll")] internal static extern bool SetWindowDisplayAffinity(nint window, uint affinity);
    [DllImport("user32.dll")] internal static extern bool IsWindow(nint window);
    [DllImport("user32.dll")] internal static extern bool IsIconic(nint window);
    [DllImport("user32.dll")] internal static extern bool IsWindowVisible(nint window);
    [DllImport("user32.dll")] internal static extern uint GetWindowThreadProcessId(nint window, out uint pid);
    [DllImport("user32.dll")] internal static extern nint GetForegroundWindow();
    [DllImport("user32.dll")] internal static extern nint WindowFromPoint(System.Drawing.Point point);
    [DllImport("user32.dll")] internal static extern nint GetAncestor(nint window, uint flags);
    [DllImport("user32.dll")] internal static extern nint MonitorFromPoint(System.Drawing.Point point, uint flags);
    [DllImport("user32.dll")] internal static extern bool RegisterHotKey(nint window, int id, uint modifiers, uint key);
    [DllImport("user32.dll")] internal static extern bool UnregisterHotKey(nint window, int id);
    [DllImport("dwmapi.dll")] private static extern int DwmGetWindowAttribute(nint window, int attr, out Rect value, int size);
    [DllImport("dwmapi.dll")] private static extern int DwmGetWindowAttribute(nint window, int attr, out int value, int size);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(nint window, System.Text.StringBuilder value, int size);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc callback, nint data);
    [DllImport("user32.dll")] private static extern bool EnumChildWindows(nint parent, EnumWindowsProc callback, nint data);
    [DllImport("user32.dll")] private static extern bool EnableWindow(nint window, bool enabled);
    internal static void DisableChildInput(nint parent) =>
        EnumChildWindows(parent, (child, _) => { EnableWindow(child, false); return true; }, 0);
    private delegate bool EnumWindowsProc(nint window, nint data);
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left, Top, Right, Bottom; }

    internal static Rectangle Bounds(nint window) =>
        DwmGetWindowAttribute(window, 9, out Rect rect, 16) == 0
            ? Rectangle.FromLTRB(rect.Left, rect.Top, rect.Right, rect.Bottom) : Rectangle.Empty;

    internal static IEnumerable<Source> Windows(nint excluded)
    {
        var sources = new List<Source>();
        EnumWindows((window, _) =>
        {
            if (window == excluded || !IsWindowVisible(window) || IsIconic(window) ||
                (DwmGetWindowAttribute(window, 14, out int cloaked, 4) == 0 && cloaked != 0))
                return true;
            var title = new System.Text.StringBuilder(384);
            if (GetWindowText(window, title, title.Capacity) > 0 && !Bounds(window).IsEmpty)
                sources.Add(new(title.ToString(), window, null));
            return true;
        }, 0);
        return sources;
    }

    internal static unsafe GraphicsCaptureItem CaptureItem(Source source)
    {
        const string name = "Windows.Graphics.Capture.GraphicsCaptureItem";
        var interop = new Guid("3628E81B-3CAC-4C60-B7F4-23CE0E0C3356");
        var itemId = new Guid("79C3F95B-31F7-4EC2-A464-632EF5D30760");
        Marshal.ThrowExceptionForHR(WindowsCreateString(name, name.Length, out var hstring));
        nint factory = 0, item = 0;
        try
        {
            Marshal.ThrowExceptionForHR(RoGetActivationFactory(hstring, in interop, out factory));
            var table = *(nint**)factory;
            var create = (delegate* unmanaged[Stdcall]<nint, nint, Guid*, nint*, int>)table[source.Screen is null ? 3 : 4];
            Marshal.ThrowExceptionForHR(create(factory, source.Handle, &itemId, &item));
            return MarshalInterface<GraphicsCaptureItem>.FromAbi(item);
        }
        finally
        {
            if (item != 0) Marshal.Release(item);
            if (factory != 0) Marshal.Release(factory);
            WindowsDeleteString(hstring);
        }
    }

    internal static IDirect3DDevice CaptureDevice(bool videoSupport = false)
    {
        var dxgiId = new Guid("54EC77FA-1377-44E6-8C32-88FD5F44C84C");
        nint device = 0, context = 0, dxgi = 0, projected = 0;
        try
        {
            if (videoSupport)
            {
                int result = PaneCreateCaptureDevice(out device);
                if (result < 0) throw new InvalidOperationException("Не удалось создать устройство Direct3D 11 на GPU NVIDIA.",
                    Marshal.GetExceptionForHR(result));
            }
            else
                Marshal.ThrowExceptionForHR(D3D11CreateDevice(0, 1, 0, 0x20, 0, 0, 7, out device, out _, out context));
            Marshal.ThrowExceptionForHR(Marshal.QueryInterface(device, in dxgiId, out dxgi));
            Marshal.ThrowExceptionForHR(CreateDirect3D11DeviceFromDXGIDevice(dxgi, out projected));
            return MarshalInterface<IDirect3DDevice>.FromAbi(projected);
        }
        finally
        {
            if (projected != 0) Marshal.Release(projected);
            if (dxgi != 0) Marshal.Release(dxgi);
            if (context != 0) Marshal.Release(context);
            if (device != 0) Marshal.Release(device);
        }
    }

    [DllImport("Panevanta.Native.dll")]
    private static extern int PaneCreateCaptureDevice(out nint device);
}




