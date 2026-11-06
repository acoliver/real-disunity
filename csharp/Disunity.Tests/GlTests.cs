using UnityEngine;

/// <summary>GL.GetGPUProjectionMatrix keeps the engine's projection convention for both kinds of target.</summary>
internal static class GlTests
{
    public static void Run()
    {
        var projection = Matrix4x4.Perspective(60f, 1.5f, 0.3f, 100f);
        foreach (var intoTexture in new[] { false, true })
        {
            var converted = GL.GetGPUProjectionMatrix(projection, intoTexture);
            for (var i = 0; i < 16; i++) Expect.Equal(projection[i], converted[i], $"element {i} for renderIntoTexture {intoTexture}");
        }
        Console.WriteLine("PASS: GL.GetGPUProjectionMatrix returns the projection in the engine's convention for screen and texture targets");
    }
}
