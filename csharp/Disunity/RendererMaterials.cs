namespace UnityEngine;

/// <summary>
/// The materials and property blocks of a <see cref="Renderer"/>, which stay in managed memory; the native
/// component holds only the material count. <see cref="material"/> and <see cref="materials"/> give each renderer
/// its own copies: the first access builds them with <c>new Material(source)</c>, names each
/// "&lt;name&gt; (Instance)", assigns them as the shared materials and keeps them, so later accesses return the same
/// copies. Setting a shared material drops the copies. A clone from <c>Instantiate</c> shares its source's copies
/// (the engine's choice) until it replaces them. A renderer starts with no material. Property blocks are stored
/// and nothing reads them yet.
/// </summary>
public abstract partial class Renderer
{
    private List<Material?> _shared = [];
    private Material?[]? _instances;
    private readonly Dictionary<int, MaterialPropertyBlock> _blocks = new();

    /// <summary>The shared materials, as the renderer holds them.</summary>
    internal virtual IReadOnlyList<Material?> ReadShared() => _shared;

    /// <summary>Replaces the shared materials. The count must fit the native limit.</summary>
    internal virtual void WriteShared(IReadOnlyList<Material?> materials)
    {
        Update(r => { r.material_count = (uint)materials.Count; return r; });
        _shared = [.. materials];
    }

    private void Assign(IReadOnlyList<Material?> materials, bool instances)
    {
        WriteShared(materials);
        _instances = instances ? [.. materials] : null;
    }

    private Material?[] Instances()
    {
        if (_instances is { } known) return known;
        var copies = ReadShared().Select(Instance).ToArray();
        Assign(copies, true);
        return copies;
    }

    private static Material? Instance(Material? source)
    {
        if (source is null) return null;
        var copy = new Material(source) { name = source.name + " (Instance)" };
        return copy;
    }

    public Material? sharedMaterial
    {
        get => ReadShared() is [var first, ..] ? first : null;
        set
        {
            var list = ReadShared().ToList();
            if (list.Count == 0) list.Add(value); else list[0] = value;
            Assign(list, false);
        }
    }

    public Material? material
    {
        get => Instances() is [var first, ..] ? first : null;
        set
        {
            var list = Instances().ToList();
            if (list.Count == 0) list.Add(value); else list[0] = value;
            Assign(list, true);
        }
    }

    public Material?[] sharedMaterials
    {
        get => [.. ReadShared()];
        set => Assign(value ?? throw new ArgumentNullException(nameof(value)), false);
    }

    public Material?[] materials
    {
        get => [.. Instances()];
        set => Assign(value ?? throw new ArgumentNullException(nameof(value)), true);
    }

    /// <summary>Clears <paramref name="m"/> and fills it with the shared materials.</summary>
    public void GetSharedMaterials(List<Material> m) => Fill(m, ReadShared());

    /// <summary>Clears <paramref name="m"/> and fills it with the per-renderer copies, making them on the first call.</summary>
    public void GetMaterials(List<Material> m) => Fill(m, Instances());

    public void SetSharedMaterials(List<Material> m) => Assign(m ?? throw new ArgumentNullException(nameof(m)), false);

    public void SetMaterials(List<Material> m) => Assign(m ?? throw new ArgumentNullException(nameof(m)), true);

    private static void Fill(List<Material> into, IEnumerable<Material?> from)
    {
        ArgumentNullException.ThrowIfNull(into);
        into.Clear();
        into.AddRange(from!);
    }

    /// <summary>Stores a copy of <paramref name="properties"/> for every material; null removes it.</summary>
    public void SetPropertyBlock(MaterialPropertyBlock? properties) => SetBlock(properties, NoMaterialIndex);

    /// <summary>Stores a copy of <paramref name="properties"/> for one material; null removes it.</summary>
    public void SetPropertyBlock(MaterialPropertyBlock? properties, int materialIndex) => SetBlock(properties, CheckIndex(materialIndex));

    /// <summary>Clears <paramref name="properties"/> and copies the block of every material into it; it stays empty when none was set.</summary>
    public void GetPropertyBlock(MaterialPropertyBlock properties) => GetBlock(properties, NoMaterialIndex);

    public void GetPropertyBlock(MaterialPropertyBlock properties, int materialIndex) => GetBlock(properties, CheckIndex(materialIndex));

    /// <summary>Whether a block was set for every material.</summary>
    public bool HasPropertyBlock() => _blocks.ContainsKey(NoMaterialIndex);

    private static int CheckIndex(int index) =>
        index >= 0 ? index : throw new ArgumentOutOfRangeException(nameof(index), index, "the material index is not negative");

    private void SetBlock(MaterialPropertyBlock? properties, int index)
    {
        if (properties is null) _blocks.Remove(index);
        else _blocks[index] = CopyBlock(properties);
    }

    private void GetBlock(MaterialPropertyBlock into, int index)
    {
        ArgumentNullException.ThrowIfNull(into);
        into.Clear();
        if (_blocks.TryGetValue(index, out var stored)) into.Properties.CopyFrom(stored.Properties);
    }

    private static MaterialPropertyBlock CopyBlock(MaterialPropertyBlock source)
    {
        var copy = new MaterialPropertyBlock();
        copy.Properties.CopyFrom(source.Properties);
        return copy;
    }
}
