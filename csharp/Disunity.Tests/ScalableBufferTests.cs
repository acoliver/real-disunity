using UnityEngine;

/// <summary>ScalableBufferManager stores the scale factors it is given and starts at one.</summary>
internal static class ScalableBufferTests
{
    public static void Run()
    {
        Expect.That(ScalableBufferManager.widthScaleFactor == 1f && ScalableBufferManager.heightScaleFactor == 1f, "the factors start at one");
        ScalableBufferManager.ResizeBuffers(0.5f, 0.25f);
        Expect.Near(0.5f, ScalableBufferManager.widthScaleFactor, "the width factor is stored");
        Expect.Near(0.25f, ScalableBufferManager.heightScaleFactor, "the height factor is stored");
        Expect.Throws<ArgumentOutOfRangeException>(() => ScalableBufferManager.ResizeBuffers(1.5f, 1f), "a width factor above one");
        Expect.Throws<ArgumentOutOfRangeException>(() => ScalableBufferManager.ResizeBuffers(1f, -0.1f), "a negative height factor");
        Expect.Throws<ArgumentOutOfRangeException>(() => ScalableBufferManager.ResizeBuffers(float.NaN, 1f), "a NaN factor");
        Expect.Near(0.5f, ScalableBufferManager.widthScaleFactor, "a refused call changes nothing");
        ScalableBufferManager.ResizeBuffers(1f, 1f);
        Console.WriteLine("PASS: ScalableBufferManager stores the scale factors from one and refuses factors outside zero to one");
    }
}
