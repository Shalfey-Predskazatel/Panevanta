using System.Runtime.InteropServices;
using Windows.Graphics.DirectX.Direct3D11;
using WinRT;

namespace Veyra;

internal sealed unsafe class FrameReader : IDisposable
{
    private readonly nint texture;
    private readonly nint context;
    private readonly int width, height, left, top;
    private static readonly Guid AccessId = new("A9B3D012-3DF2-4EE3-B8D1-8695F457D3C1");
    private static readonly Guid TextureId = new("6F15AAF2-D208-4E89-9AB4-489535D34F9C");
    [StructLayout(LayoutKind.Sequential)]
    private struct Description
    {
        public uint Width, Height, Mips, Array, Format, Samples, Quality, Usage, Bind, Cpu, Misc;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct Box { public uint Left, Top, Front, Right, Bottom, Back; }
    [StructLayout(LayoutKind.Sequential)]
    private struct Mapped { public nint Data; public uint RowPitch, DepthPitch; }

    public FrameReader(IDirect3DSurface surface, int left, int top, int width, int height)
    {
        this.left = left;
        this.top = top;
        this.width = width;
        this.height = height;
        nint src = OpenTexture(surface), device = 0, staging = 0;
        try
        {
            ((delegate* unmanaged[Stdcall]<nint, nint*, void>)(*(nint**)src)[3])(src, &device);
            Description desc = new() { Width = (uint)width, Height = (uint)height, Mips = 1, Array = 1,
                Format = 10, Samples = 1, Usage = 3, Cpu = 0x20000 };
            Marshal.ThrowExceptionForHR(((delegate* unmanaged[Stdcall]<nint, Description*, nint, nint*, int>)(*(nint**)device)[5])
                (device, &desc, 0, &staging));
            nint immediate;
            ((delegate* unmanaged[Stdcall]<nint, nint*, void>)(*(nint**)device)[40])(device, &immediate);
            texture = staging;
            context = immediate;
        }
        finally
        {
            if (device != 0) Marshal.Release(device);
            Marshal.Release(src);
        }
    }

    internal static nint OpenTexture(IDirect3DSurface surface)
    {
        nint projected = MarshalInterface<IDirect3DSurface>.FromManaged(surface);
        nint access = 0;
        try
        {
            Marshal.ThrowExceptionForHR(Marshal.QueryInterface(projected, in AccessId, out access));
            Guid id = TextureId;
            nint source;
            Marshal.ThrowExceptionForHR(((delegate* unmanaged[Stdcall]<nint, Guid*, nint*, int>)(*(nint**)access)[3])(access, &id, &source));
            return source;
        }
        finally
        {
            if (access != 0) Marshal.Release(access);
            Marshal.Release(projected);
        }
    }

    private Mapped Map(IDirect3DSurface surface)
    {
        nint src = OpenTexture(surface);
        var table = *(nint**)context;
        Box box = new() { Left = (uint)left, Top = (uint)top, Right = (uint)(left + width), Bottom = (uint)(top + height), Back = 1 };
        try
        {
            ((delegate* unmanaged[Stdcall]<nint, nint, uint, uint, uint, uint, nint, uint, Box*, void>)table[46])
                (context, texture, 0, 0, 0, 0, src, 0, &box);
        }
        finally { Marshal.Release(src); }
        Mapped mapped;
        Marshal.ThrowExceptionForHR(((delegate* unmanaged[Stdcall]<nint, nint, uint, uint, uint, Mapped*, int>)table[14])
            (context, texture, 0, 1, 0, &mapped));
        return mapped;
    }

    private void Unmap() => ((delegate* unmanaged[Stdcall]<nint, nint, uint, void>)(*(nint**)context)[15])(context, texture, 0);

    public float Difference(IDirect3DSurface surface, ushort[] previous)
    {
        var mapped = Map(surface);
        int changes = 0, index = 0;
        try
        {
            for (int y = 0; y < height; y += 2)
            {
                var row = (ushort*)((byte*)mapped.Data + y * mapped.RowPitch);
                for (int x = 0; x < width; x += 2)
                {
                    ushort value = row[x * 4 + 1];
                    if (Math.Abs(value - previous[index]) > 10) changes++;
                    previous[index++] = value;
                }
            }
        }
        finally { Unmap(); }
        return (float)changes / index;
    }

    public void Dispose()
    {
        Marshal.Release(context);
        Marshal.Release(texture);
    }
}

