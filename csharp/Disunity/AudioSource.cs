namespace UnityEngine;

/// <summary>
/// An audio player. The engine has no audio output, so an AudioSource cannot be attached to a GameObject
/// (<c>AddComponent&lt;AudioSource&gt;</c> throws) and every member throws <see cref="NotSupportedException"/>
/// naming itself. The type exists so code that refers to it loads and runs until it uses audio.
/// The inherited <c>enabled</c> is stored state.
/// </summary>
public class AudioSource : Behaviour
{
    private protected override bool EnabledState { get; set; } = true;

    public bool isPlaying => throw Unsupported("isPlaying");
    public AudioClip clip { set => throw Unsupported("clip"); }
    public float volume { set => throw Unsupported("volume"); }
    public float minDistance { set => throw Unsupported("minDistance"); }

    public void Play() => throw Unsupported("Play");
    public void Pause() => throw Unsupported("Pause");
    public void UnPause() => throw Unsupported("UnPause");
    public void Stop() => throw Unsupported("Stop");

    public static void PlayClipAtPoint(AudioClip clip, Vector3 position, float volume) => throw Unsupported("PlayClipAtPoint");

    private static NotSupportedException Unsupported(string member) =>
        new($"AudioSource.{member}: the engine has no audio output yet");
}
