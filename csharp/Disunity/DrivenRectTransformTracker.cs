namespace UnityEngine;

/// <summary>
/// Declares which properties of a <see cref="RectTransform"/> a layout script drives. Driving only
/// marks those properties as controlled by a script in an editor. The engine has no editor, so the
/// tracker keeps nothing and every member returns without effect; that is the engine's choice.
/// </summary>
public struct DrivenRectTransformTracker
{
    /// <exception cref="ArgumentNullException"><paramref name="rectTransform"/> is null.</exception>
    public void Add(Object driver, RectTransform rectTransform, DrivenTransformProperties drivenProperties)
    {
        if (rectTransform is null) throw new ArgumentNullException(nameof(rectTransform));
    }

    public void Clear() { }

    public static void StartRecordingUndo() { }

    public static void StopRecordingUndo() { }
}
