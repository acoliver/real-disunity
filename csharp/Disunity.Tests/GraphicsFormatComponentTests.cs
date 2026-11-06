using UnityEngine.Experimental.Rendering;

/// <summary>GraphicsFormatUtility counts the color and alpha channels a format holds.</summary>
internal static class GraphicsFormatComponentTests
{
    public static void Run()
    {
        (GraphicsFormat Format, uint Color, uint Alpha)[] cases =
        [
            (GraphicsFormat.None, 0, 0), (GraphicsFormat.R8_UNorm, 1, 0), (GraphicsFormat.R8G8_UNorm, 2, 0), (GraphicsFormat.R8G8B8_SRGB, 3, 0),
            (GraphicsFormat.R8G8B8A8_UNorm, 3, 1), (GraphicsFormat.B8G8R8A8_SRGB, 3, 1), (GraphicsFormat.R16G16B16A16_SFloat, 3, 1),
            (GraphicsFormat.R32_SFloat, 1, 0), (GraphicsFormat.A2B10G10R10_UNormPack32, 3, 1), (GraphicsFormat.B10G11R11_UFloatPack32, 3, 0),
            (GraphicsFormat.E5B9G9R9_UFloatPack32, 3, 0), (GraphicsFormat.R5G6B5_UNormPack16, 3, 0), (GraphicsFormat.R4G4B4A4_UNormPack16, 3, 1),
            (GraphicsFormat.RGBA_DXT5_UNorm, 3, 1), (GraphicsFormat.RGBA_BC7_UNorm, 3, 1), (GraphicsFormat.R_BC4_UNorm, 1, 0),
            (GraphicsFormat.RG_BC5_UNorm, 2, 0), (GraphicsFormat.RGB_BC6H_UFloat, 3, 0),
            (GraphicsFormat.D32_SFloat, 0, 0), (GraphicsFormat.D24_UNorm_S8_UInt, 0, 0), (GraphicsFormat.S8_UInt, 0, 0),
        ];
        foreach (var (format, color, alpha) in cases)
        {
            Expect.Equal(color, GraphicsFormatUtility.GetColorComponentCount(format), $"{format} color channels");
            Expect.Equal(alpha, GraphicsFormatUtility.GetAlphaComponentCount(format), $"{format} alpha channels");
            Expect.Equal(color + alpha, GraphicsFormatUtility.GetComponentCount(format), $"{format} channels");
            Expect.Equal(alpha > 0, GraphicsFormatUtility.HasAlphaChannel(format), $"{format} has alpha");
        }
        Console.WriteLine("PASS: GraphicsFormatUtility counts the color, alpha and all channels of a format and reports whether it has alpha");
    }
}
