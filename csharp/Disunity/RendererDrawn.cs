using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>What the host draws each material slot of a renderer with: the slot's world material, sent whenever the shared
/// materials are set. The world material follows its own changes, and the host draws a slot from the material's current
/// <c>_Color</c> or <c>_BaseColor</c> and <c>_MainTex</c>, and white with no texture when the slot has no material. A
/// property block set for every material is sent to the world and applies to every slot; one set for a single material
/// index is sent too and applies to that slot, above the all-material block.</summary>
public abstract partial class Renderer
{
    /// <summary>Sends the world material of each slot, 0 for an empty slot.</summary>
    private void SendSlotMaterials(IReadOnlyList<Material?> materials)
    {
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
}
