using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>What the host draws each material slot of a renderer with: a colour and a texture identity per slot, and the
/// world material of the slot, sent whenever the shared materials are set. The colour and texture identity are not sent
/// again when the material changes; the world material follows its own changes, and the host draws a slot that has a
/// world material from the material's current <c>_Color</c> or <c>_BaseColor</c> and <c>_MainTex</c>, so the colour
/// and texture identity sent here are what it draws a slot with no world material with. A property block set for every
/// material is sent to the world and applies to every slot; one set for a single material index is sent too and applies
/// to that slot, above the all-material block.</summary>
public abstract partial class Renderer
{
    /// <summary>Sends, per slot, the material's <c>_Color</c> or else <c>_BaseColor</c> as the material holds it (white
    /// when it has neither, or when the slot is empty) and the renderer identity of its main texture when that is a
    /// <see cref="Texture2D"/> with pixels (0 for no texture, any other texture type, or an empty slot).</summary>
    private unsafe void SendDrawnMaterials(IReadOnlyList<Material?> materials)
    {
        var slots = new DisunityDrawnMaterial[materials.Count];
        for (var i = 0; i < slots.Length; i++) slots[i] = DrawnOf(materials[i]);
        if (this is MeshRenderer mesh) WhiteRenderers.NoteSent(mesh, materials.Count > 0 ? materials[0] : null);
        fixed (DisunityDrawnMaterial* pointer = slots)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_slots_write(
                DisunityHost.WorldHandle, Handle, slots.Length == 0 ? null : pointer, (uint)slots.Length));
        WorldGraphics.WriteRendererMaterials(Handle, materials.Select(material => material is { } own && own ? own.WorldLink.Id : 0UL).ToArray());
    }

    /// <summary>Whether the native component is a mesh renderer, which holds world materials and a property block.</summary>
    internal virtual bool HoldsWorldMaterials => true;

    /// <summary>The number of material slots a native renderer holds, so the first index no slot can draw with.</summary>
    private const int MaxMaterialSlots = 64;

    /// <summary>Replaces the world's property block of this renderer for the material <paramref name="index"/>, or for
    /// every material when it is <see cref="NoMaterialIndex"/>, with <paramref name="block"/>, or removes it.</summary>
    private void SendPropertyBlock(MaterialPropertyBlock? block, int index)
    {
        if (!HoldsWorldMaterials || !DisunityHost.IsLoaded || index >= MaxMaterialSlots) return;
        var (scope, target) = BlockScope(index);
        WorldGraphics.ClearProperties(scope, target);
        if (block is not null) WorldGraphics.SendAll(scope, target, block.Properties);
    }

    private unsafe (uint Scope, WorldGraphics.Link Target) BlockScope(int index)
    {
        if (index == NoMaterialIndex) return (WorldGraphics.ScopeBlock, new WorldGraphics.Link(DisunityHost.WorldHandle, Handle));
        ulong token;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_slot_block_target(DisunityHost.WorldHandle, Handle, (uint)index, &token));
        return (WorldGraphics.ScopeSlotBlock, new WorldGraphics.Link(DisunityHost.WorldHandle, token));
    }

    /// <summary>The main texture the host draws a slot with: a <see cref="Texture2D"/> with pixels, else null.</summary>
    internal static Texture2D? DrawableMainTexture(Material material) =>
        material.mainTexture is Texture2D { width: > 0, height: > 0 } plain ? plain : null;

    private static DisunityDrawnMaterial DrawnOf(Material? material)
    {
        if (material is null) return new DisunityDrawnMaterial { r = 1, g = 1, b = 1, a = 1 };
        var color = material.HasProperty("_Color") ? material.GetColor("_Color")
            : material.HasProperty("_BaseColor") ? material.GetColor("_BaseColor")
            : Color.white;
        var texture = DrawableMainTexture(material)?.RendererIdentity ?? 0UL;
        return new DisunityDrawnMaterial { r = Channel(color.r), g = Channel(color.g), b = Channel(color.b), a = Channel(color.a), texture = texture };
    }

    /// <summary>The host takes finite, non-negative channels: a negative one is 0 and a value that is not a number is 1.</summary>
    private static float Channel(float value) => float.IsFinite(value) ? Math.Max(0f, value) : 1f;
}
