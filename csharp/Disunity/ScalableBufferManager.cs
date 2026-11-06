namespace UnityEngine;

/// <summary>
/// The scale factors of dynamically scaled buffers. <see cref="ResizeBuffers"/> stores the factors and
/// <see cref="widthScaleFactor"/> and <see cref="heightScaleFactor"/> return them, from 1 at the start. The engine
/// draws no dynamically scaled buffer, so a factor changes nothing else (the engine's choice). A factor outside
/// 0 to 1, or a NaN, throws <see cref="ArgumentOutOfRangeException"/>.
/// </summary>
public static class ScalableBufferManager
{
    private static float _width = 1f;
    private static float _height = 1f;

    public static float widthScaleFactor => _width;
    public static float heightScaleFactor => _height;

    public static void ResizeBuffers(float widthScale, float heightScale)
    {
        if (!(widthScale is >= 0f and <= 1f)) throw new ArgumentOutOfRangeException(nameof(widthScale), widthScale, "a scale factor is between 0 and 1");
        if (!(heightScale is >= 0f and <= 1f)) throw new ArgumentOutOfRangeException(nameof(heightScale), heightScale, "a scale factor is between 0 and 1");
        _width = widthScale;
        _height = heightScale;
    }
}
