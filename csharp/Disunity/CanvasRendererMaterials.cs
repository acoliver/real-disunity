using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>The material and pop material lists of a <see cref="CanvasRenderer"/>.</summary>
public sealed partial class CanvasRenderer
{
    /// <summary>The number of materials, 0 through <see cref="MaxMaterials"/>. Lowering it drops the materials past
    /// the new count.</summary>
    public int materialCount
    {
        get => (int)Read().material_count;
        set => Update(r => { r.material_count = CheckCount(value); return r; });
    }

    /// <summary>The number of pop materials, 0 through <see cref="MaxMaterials"/>.</summary>
    public int popMaterialCount
    {
        get => (int)Read().pop_material_count;
        set => Update(r => { r.pop_material_count = CheckCount(value); return r; });
    }

    private static uint CheckCount(int count) =>
        count is < 0 or > MaxMaterials
            ? throw new ArgumentOutOfRangeException(nameof(count), count, $"a material count is 0 through {MaxMaterials}")
            : (uint)count;

    /// <summary>Sets material <paramref name="index"/>, which must be below <see cref="materialCount"/>; null clears it.</summary>
    public void SetMaterial(Material? material, int index) => SetSlot(0, material, index);

    /// <summary>Sets material 0 and the main texture and makes the material count 1, dropping any other material.</summary>
    public void SetMaterial(Material? material, Texture? texture)
    {
        materialCount = 1;
        SetSlot(0, material, 0);
        SetTexture(texture);
    }

    /// <summary>Material 0, or null when the count is 0 or the slot is empty.</summary>
    public Material? GetMaterial() => GetMaterial(0);

    /// <summary>The material at <paramref name="index"/>, or null for an empty slot or an index outside the count.</summary>
    public Material? GetMaterial(int index) => GetSlot(0, index);

    public void SetPopMaterial(Material? material, int index) => SetSlot(1, material, index);

    public Material? GetPopMaterial(int index) => GetSlot(1, index);

    private unsafe void SetSlot(uint pop, Material? material, int index)
    {
        var count = pop == 0 ? materialCount : popMaterialCount;
        if (index < 0 || index >= count)
            throw new ArgumentOutOfRangeException(nameof(index), index, $"the material index must be below the count, {count}");
        var id = IdOf(material, nameof(material));
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_material_write(DisunityHost.WorldHandle, Handle, pop, (uint)index, id));
    }

    private unsafe Material? GetSlot(uint pop, int index)
    {
        var count = pop == 0 ? materialCount : popMaterialCount;
        if (index < 0 || index >= count) return null;
        ulong id;
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_material_read(DisunityHost.WorldHandle, Handle, pop, (uint)index, &id));
        return Find<Material>(id);
    }
}
