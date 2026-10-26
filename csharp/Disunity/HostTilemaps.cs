using Disunity.Native;
using UnityEngine;
using UnityEngine.Tilemaps;

namespace Disunity.Hosting;

/// <summary>A tilemap's <c>Grid</c> cell size and gap, its tile anchor, and its
/// <c>TilemapRenderer</c> sorting.</summary>
public readonly record struct TilemapSettings(Vector2 CellSize, Vector2 CellGap, Vector2 TileAnchor,
    int SortingLayer = 0, int SortingOrder = 0);

/// <summary>Unity's <c>Tile.ColliderType</c>.</summary>
public enum TileColliderType { None, Sprite, Grid }

/// <summary>A palette tile. <paramref name="PhysicsShape"/> is the sprite's physics shape, convex
/// paths in sprite units around its pivot, used when the collider type is Sprite.
/// <paramref name="Offset"/> moves the tile from its anchored point.</summary>
public sealed record TileDefinition(Sprite? Sprite, Color Color, TileColliderType Collider,
    IReadOnlyList<Vector2[]> PhysicsShape, Vector2 Offset = default);

public static partial class DisunityHost
{
    public static void ConfigureTilemap(Tilemap tilemap, TilemapSettings settings)
    {
        ArgumentNullException.ThrowIfNull(tilemap);
        var value = tilemap.Read();
        (value.cell_x, value.cell_y) = (settings.CellSize.x, settings.CellSize.y);
        (value.gap_x, value.gap_y) = (settings.CellGap.x, settings.CellGap.y);
        (value.anchor_x, value.anchor_y) = (settings.TileAnchor.x, settings.TileAnchor.y);
        (value.sorting_layer, value.sorting_order) = (settings.SortingLayer, settings.SortingOrder);
        tilemap.Write(value);
    }

    /// <summary>Sets palette entry <paramref name="index"/>, replacing it or, at the palette's
    /// length, appending it.</summary>
    public static unsafe void SetTile(Tilemap tilemap, int index, TileDefinition tile)
    {
        ArgumentNullException.ThrowIfNull(tilemap);
        ArgumentNullException.ThrowIfNull(tile);
        var record = new DisunityTile
        {
            collider = (uint)tile.Collider,
            r = tile.Color.r, g = tile.Color.g, b = tile.Color.b, a = tile.Color.a,
            m00 = 1, m11 = 1, tx = tile.Offset.x, ty = tile.Offset.y,
        };
        var slot = checked((uint)index);
        if (tile.Sprite is { } sprite)
        {
            var description = sprite.Description;
            ThrowIfError(NativeBindings.disunity_tilemap_set_tile(WorldHandle, tilemap.Handle, slot, &record, &description));
        }
        else
            ThrowIfError(NativeBindings.disunity_tilemap_set_tile(WorldHandle, tilemap.Handle, slot, &record, null));
        if (tile.Collider == TileColliderType.Sprite) SetTileShape(tilemap, slot, tile.PhysicsShape);
    }

    private static unsafe void SetTileShape(Tilemap tilemap, uint index, IReadOnlyList<Vector2[]> paths)
    {
        var lengths = paths.Select(path => checked((uint)path.Length)).ToArray();
        var points = paths.SelectMany(path => path).SelectMany(p => new[] { p.x, p.y }).ToArray();
        fixed (float* point = points)
        fixed (uint* length = lengths)
            ThrowIfError(NativeBindings.disunity_tilemap_set_tile_shape(WorldHandle, tilemap.Handle, index, point, length,
                checked((uint)lengths.Length)));
    }

    /// <summary>Replaces every cell: each position holds the palette entry it names.</summary>
    public static unsafe void SetCells(Tilemap tilemap, IReadOnlyList<(Vector2Int Position, int Tile)> cells)
    {
        ArgumentNullException.ThrowIfNull(tilemap);
        ArgumentNullException.ThrowIfNull(cells);
        var records = cells
            .Select(c => new DisunityTileCell { x = c.Position.x, y = c.Position.y, tile = checked((uint)c.Tile) })
            .ToArray();
        fixed (DisunityTileCell* pointer = records)
            ThrowIfError(NativeBindings.disunity_tilemap_set_cells(WorldHandle, tilemap.Handle, pointer,
                checked((uint)records.Length)));
    }
}
