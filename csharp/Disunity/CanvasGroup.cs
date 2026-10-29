using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// A native group of UI settings applied to the object it is on and everything under it. Every
/// setting is read from and written to the group in the native world, so a clone from
/// <c>Instantiate</c> has its own settings. UI text is drawn with its alpha multiplied by the
/// <see cref="alpha"/> of each enabled group above it, stopping at a group that
/// <see cref="ignoreParentGroups"/>. The engine sends no UI pointer events yet, so
/// <see cref="interactable"/> and <see cref="blocksRaycasts"/> are stored and nothing reads them
/// except <see cref="IsRaycastLocationValid"/>. The native side brings an alpha into 0 through 1 and
/// rejects a NaN alpha with a <see cref="DisunityException"/>. A group is a <see cref="Behaviour"/>
/// and not a <see cref="MonoBehaviour"/>, so a search for scripts does not find it.
/// </summary>
public sealed class CanvasGroup : Behaviour, ICanvasRaycastFilter
{
    private unsafe DisunityCanvasGroup Read()
    {
        DisunityCanvasGroup value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_group_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Write(DisunityCanvasGroup value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_group_write(DisunityHost.WorldHandle, Handle, &value));
    private void Update(Func<DisunityCanvasGroup, DisunityCanvasGroup> change) => Write(change(Read()));

    private protected override bool EnabledState { get => Read().enabled == 1; set => Update(g => { g.enabled = value ? 1u : 0u; return g; }); }

    /// <summary>Clamped to 0 through 1.</summary>
    public float alpha { get => Read().alpha; set => Update(g => { g.alpha = value; return g; }); }
    public bool interactable { get => Read().interactable == 1; set => Update(g => { g.interactable = value ? 1u : 0u; return g; }); }
    public bool blocksRaycasts { get => Read().blocks_raycasts == 1; set => Update(g => { g.blocks_raycasts = value ? 1u : 0u; return g; }); }
    public bool ignoreParentGroups { get => Read().ignore_parent_groups == 1; set => Update(g => { g.ignore_parent_groups = value ? 1u : 0u; return g; }); }

    /// <summary>True when the group blocks raycasts, wherever the location is.</summary>
    public bool IsRaycastLocationValid(Vector2 sp, Camera eventCamera) => blocksRaycasts;
}
