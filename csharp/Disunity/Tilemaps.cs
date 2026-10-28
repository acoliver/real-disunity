using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine.Tilemaps;

/// <summary>A native tilemap. Hosts set its layout, palette and cells with
/// <see cref="DisunityHost.ConfigureTilemap"/>, <see cref="DisunityHost.SetTile"/> and
/// <see cref="DisunityHost.SetCells"/>.</summary>
public sealed class Tilemap : NativeComponent
{
    internal unsafe DisunityTilemap Read()
    {
        DisunityTilemap value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_tilemap_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    internal unsafe void Write(DisunityTilemap value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_tilemap_write(DisunityHost.WorldHandle, Handle, &value));
    public bool enabled { get => Read().enabled == 1; set { var v = Read(); v.enabled = value ? 1u : 0u; Write(v); } }
    public Color color
    {
        get { var v = Read(); return new Color(v.r, v.g, v.b, v.a); }
        set { var v = Read(); (v.r, v.g, v.b, v.a) = (value.r, value.g, value.b, value.a); Write(v); }
    }
    public Vector3 tileAnchor
    {
        get { var v = Read(); return new Vector3(v.anchor_x, v.anchor_y, 0); }
        set { var v = Read(); (v.anchor_x, v.anchor_y) = (value.x, value.y); Write(v); }
    }
}

/// <summary>Collides with the Tilemap on the same GameObject. Its shapes follow the cells, so
/// tile edits change what collides without rewriting the collider.</summary>
public sealed class TilemapCollider2D : Collider2D;
