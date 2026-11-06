using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>What the host draws each material slot of a renderer with: a colour and a texture identity per slot, and the
/// world material of the slot, sent whenever the shared materials are set. The colour and texture identity are not sent
/// again when the material changes; the world material follows its own changes. A property block set for every
/// material is sent to the world, and one set for a single material index stays in managed memory.</summary>
public abstract partial class Renderer
{
    /// <summary>Sends, per slot, the material's <c>_Color</c> or else <c>_BaseColor</c> as the material holds it (white
    /// when it has neither, or when the slot is empty) and the renderer identity of its main texture when that is a
    /// <see cref="Texture2D"/> with pixels (0 for no texture, any other texture type, or an empty slot).</summary>
    private unsafe void SendDrawnMaterials(IReadOnlyList<Material?> materials)
    {
        var slots = new DisunityDrawnMaterial[materials.Count];
        for (var i = 0; i < slots.Length; i++) slots[i] = DrawnOf(materials[i]);
        fixed (DisunityDrawnMaterial* pointer = slots)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_slots_write(
                DisunityHost.WorldHandle, Handle, slots.Length == 0 ? null : pointer, (uint)slots.Length));
        WorldGraphics.WriteRendererMaterials(Handle, materials.Select(material => material is { } own && own ? own.WorldLink.Id : 0UL).ToArray());
    }

    /// <summary>Whether the native component is a mesh renderer, which holds world materials and a property block.</summary>
    internal virtual bool HoldsWorldMaterials => true;

    /// <summary>Replaces the world's property block of this renderer with <paramref name="block"/>, or removes it.</summary>
    private void SendPropertyBlock(MaterialPropertyBlock? block)
    {
        if (!HoldsWorldMaterials || !DisunityHost.IsLoaded) return;
        var target = new WorldGraphics.Link(DisunityHost.WorldHandle, Handle);
        WorldGraphics.ClearProperties(WorldGraphics.ScopeBlock, target);
        if (block is not null) WorldGraphics.SendAll(WorldGraphics.ScopeBlock, target, block.Properties);
    }

    private static DisunityDrawnMaterial DrawnOf(Material? material)
    {
        if (material is null) return new DisunityDrawnMaterial { r = 1, g = 1, b = 1, a = 1 };
        var color = material.HasProperty("_Color") ? material.GetColor("_Color")
            : material.HasProperty("_BaseColor") ? material.GetColor("_BaseColor")
            : Color.white;
        var texture = material.mainTexture is Texture2D { width: > 0, height: > 0 } plain ? plain.RendererIdentity : 0UL;
        return new DisunityDrawnMaterial { r = Channel(color.r), g = Channel(color.g), b = Channel(color.b), a = Channel(color.a), texture = texture };
    }

    /// <summary>The host takes finite, non-negative channels: a negative one is 0 and a value that is not a number is 1.</summary>
    private static float Channel(float value) => float.IsFinite(value) ? Math.Max(0f, value) : 1f;
}
