namespace UnityEngine;

/// <summary>
/// A clip of audio. The engine has no audio output, so no clip exists: every member throws
/// <see cref="NotSupportedException"/> naming itself. The type exists so code that refers to it loads.
/// </summary>
public class AudioClip : Object
{
    public delegate void PCMReaderCallback(float[] data);
    public delegate void PCMSetPositionCallback(int position);

    private protected AudioClip() { }

    public int channels => throw Unsupported("channels");
    public int samples => throw Unsupported("samples");
    public float length => throw Unsupported("length");

    public bool GetData(float[] data, int offsetSamples) => throw Unsupported("GetData");
    public bool SetData(float[] data, int offsetSamples) => throw Unsupported("SetData");

    public static AudioClip Create(string name, int lengthSamples, int channels, int frequency, bool stream) =>
        throw Unsupported("Create");

    public static AudioClip Create(string name, int lengthSamples, int channels, int frequency, bool stream,
        PCMReaderCallback pcmreadercallback, PCMSetPositionCallback pcmsetpositioncallback) =>
        throw Unsupported("Create");

    private static NotSupportedException Unsupported(string member) =>
        new($"AudioClip.{member}: the engine has no audio output yet");
}
