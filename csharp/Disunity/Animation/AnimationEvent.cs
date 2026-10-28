namespace UnityEngine;

/// <summary>A call an animation clip makes at a point in its timeline. The engine plays no clips, so events are only data.</summary>
public sealed class AnimationEvent
{
    public AnimationEvent() { }

    public string data { get; set; } = "";
    public string stringParameter { get; set; } = "";
    public float floatParameter { get; set; }
    public int intParameter { get; set; }
    public Object? objectReferenceParameter { get; set; }
    public string functionName { get; set; } = "";
    public float time { get; set; }
    public SendMessageOptions messageOptions { get; set; }

    /// <summary>False: no playback fires events.</summary>
    public bool isFiredByLegacy => false;
    public bool isFiredByAnimator => false;
    public AnimatorStateInfo animatorStateInfo => default;
    public AnimatorClipInfo animatorClipInfo => default;

    internal AnimationEvent Copy() => (AnimationEvent)MemberwiseClone();
}
