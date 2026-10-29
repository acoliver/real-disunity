namespace UnityEngine.U2D;

/// <summary>
/// A group of sprites a game asks for by name or all at once. The engine does not load atlases from
/// player data yet, so only the engine builds one, from the sprites it is given.
/// <see cref="GetSprites"/> returns copies, which is the engine's choice: a game that renames the
/// sprites it gets back does not rename the atlas's own. <see cref="GetSprite"/> returns the atlas's
/// own sprite, the one <see cref="CanBindTo"/> accepts.
/// </summary>
public class SpriteAtlas : Object
{
    private readonly Sprite[] _sprites;

    internal SpriteAtlas(string name, string tag, bool isVariant, IReadOnlyList<Sprite> sprites)
    {
        ArgumentNullException.ThrowIfNull(name);
        ArgumentNullException.ThrowIfNull(tag);
        ArgumentNullException.ThrowIfNull(sprites);
        this.name = name;
        this.tag = tag;
        this.isVariant = isVariant;
        _sprites = [.. sprites];
    }

    public int spriteCount => _sprites.Length;
    public string tag { get; }
    public bool isVariant { get; }

    /// <summary>Fills <paramref name="sprites"/> from its start with copies of the atlas's sprites, as many as fit, and returns how many it wrote.</summary>
    public int GetSprites(Sprite[] sprites)
    {
        ArgumentNullException.ThrowIfNull(sprites);
        var count = Math.Min(sprites.Length, _sprites.Length);
        for (var i = 0; i < count; i++) sprites[i] = _sprites[i].Copy();
        return count;
    }

    /// <summary>The atlas's first sprite with this name, or null.</summary>
    public Sprite? GetSprite(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        return Array.Find(_sprites, sprite => sprite.name == name);
    }

    /// <summary>Whether <paramref name="sprite"/> is one of the atlas's own sprites; a copy from <see cref="GetSprites"/> is not.</summary>
    public bool CanBindTo(Sprite sprite) => sprite is not null && Array.IndexOf(_sprites, sprite) >= 0;
}
