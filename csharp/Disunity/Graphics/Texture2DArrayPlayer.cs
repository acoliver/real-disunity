using UnityEngine.Experimental.Rendering;

namespace UnityEngine;

/// <summary>What a player's resource plan says about a texture array before its pixels are read.</summary>
internal readonly record struct PlayerTextureArray(int Width, int Height, int Depth, int GraphicsFormat, int MipCount, bool Readable, long DataSize);

public sealed partial class Texture2DArray
{
    /// <summary>Makes a zeroed array of the planned size, slice count and format, whose raw size must equal
    /// <see cref="PlayerTextureArray.DataSize"/>. A format the engine cannot lay out keeps the planned size as raw bytes.
    /// Failures name the array.</summary>
    internal static Texture2DArray FromPlayer(string name, PlayerTextureArray plan)
    {
        var graphics = (GraphicsFormat)plan.GraphicsFormat;
        var format = LayoutFormat(graphics);
        Texture2DArray array;
        try { array = new Texture2DArray(plan.Width, plan.Height, plan.Depth, format, graphics, plan.MipCount, plan.DataSize); }
        catch (ArgumentException error) { throw new InvalidDataException($"resource texture array {name}: {error.Message}", error); }
        catch (NotSupportedException error) { throw new NotSupportedException($"resource texture array {name}: {error.Message}", error); }
        if (array._data.Length != plan.DataSize)
            throw new InvalidDataException($"resource texture array {name}: the plan holds {plan.DataSize} bytes, but a {plan.Width}x{plan.Height}x{plan.Depth} {format} array with {plan.MipCount} mip levels needs {array._data.Length}");
        array.name = name;
        array._readable = plan.Readable;
        return array;
    }

    /// <summary>The <see cref="TextureFormat"/> whose layout the array uses, or <c>null</c> when it has none the engine can allocate.</summary>
    private static TextureFormat? LayoutFormat(GraphicsFormat graphics)
    {
        try
        {
            var format = GraphicsFormats.ToTextureFormat(graphics);
            return TexelFormats.CanAllocate(format) && !TexelFormats.IsCrunched(format) ? format : null;
        }
        catch (NotSupportedException) { return null; }
    }
}
