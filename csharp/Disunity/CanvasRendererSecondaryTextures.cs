namespace UnityEngine;

/// <summary>
/// The secondary textures of a <see cref="CanvasRenderer"/>: a count of slots, each with a shader property name and a
/// texture. They stay in managed memory, nothing reads them yet, and a clone from <c>Instantiate</c> copies them. A
/// texture is held by reference, so the slot keeps it after it is destroyed (the engine's choice).
/// </summary>
public sealed partial class CanvasRenderer
{
    private readonly List<(string? Name, Texture? Texture)> _secondary = [];

    /// <summary>Makes the list <paramref name="count"/> slots long. New slots are empty and slots past the count are dropped.</summary>
    public void SetSecondaryTextureCount(int count)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(count);
        if (count < _secondary.Count) _secondary.RemoveRange(count, _secondary.Count - count);
        while (_secondary.Count < count) _secondary.Add((null, null));
    }

    /// <summary>Sets slot <paramref name="index"/>, which must be below the count, to a property name and a texture. A null texture empties the slot's texture.</summary>
    public void SetSecondaryTexture(int index, string name, Texture? texture)
    {
        if (index < 0 || index >= _secondary.Count)
            throw new ArgumentOutOfRangeException(nameof(index), index, $"the canvas renderer has {_secondary.Count} secondary texture slots");
        _secondary[index] = (name, texture);
    }

    internal int SecondaryTextureCount => _secondary.Count;

    internal (string? Name, Texture? Texture) SecondaryTextureAt(int index) => _secondary[index];

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        base.CopyManagedState(source, remap);
        _secondary.Clear();
        _secondary.AddRange(((CanvasRenderer)source)._secondary);
    }
}
