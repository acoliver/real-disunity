using Disunity.Hosting;
using UnityEngine;

internal static class SpriteTests
{
    /// <summary>The sprite's sorting order lives in its native record; the Renderer members read it from there.</summary>
    private static unsafe void SortingOrderReadsTheNativeRecord(SpriteRenderer renderer)
    {
        Disunity.Native.DisunitySprite value;
        DisunityHost.ThrowIfError(Disunity.Native.NativeBindings.disunity_sprite_read(DisunityHost.WorldHandle, renderer.Handle, &value));
        value.sorting_order = 31;
        DisunityHost.ThrowIfError(Disunity.Native.NativeBindings.disunity_sprite_write(DisunityHost.WorldHandle, renderer.Handle, &value));
        Require(renderer.sortingOrder == 31, "sortingOrder reads the native sprite record");
        renderer.sortingOrder = 9;
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        var owner = DisunityHost.CreateGameObject();
        var renderer = owner.AddComponent<SpriteRenderer>();
        Require(renderer.sprite == null && renderer.enabled && renderer.color == Color.white, "native sprite defaults");
        var sprite = DisunityHost.CreateSprite(7, 8, 8, new Rect(0, 0, 2, 4), new Vector2(0.5f, 0.5f), 2);
        renderer.sprite = sprite;
        Require(ReferenceEquals(renderer.sprite, sprite) && ReferenceEquals(renderer.sprite, renderer.sprite), "sprite assignment and reads preserve identity");
        var shared = DisunityHost.CreateGameObject().AddComponent<SpriteRenderer>();
        shared.sprite = sprite;
        Require(ReferenceEquals(shared.sprite, renderer.sprite), "renderers share immutable sprite identity");
        renderer.color = new Color(0.2f, 0.4f, 0.6f, 0.5f);
        renderer.flipX = true;
        renderer.flipY = true;
        renderer.sortingOrder = 9;
        renderer.sortingLayerID = -2;
        Require(sprite.associatedAlphaSplitTexture == null, "a sprite has no split alpha texture");
        Require(renderer.sprite!.rect == sprite.rect && renderer.sprite.pixelsPerUnit == 2, "native sprite content roundtrip");
        Require(renderer.color.a == 0.5f && renderer.flipX && renderer.flipY, "native tint and flips");
        Require(renderer.sortingOrder == 9 && renderer.sortingLayer == -2, "native sorting");
        owner.transform.position = new Vector3(2, 3, 4);
        Require(renderer.bounds.center == new Vector3(2, 3, 4) && renderer.bounds.size == new Vector3(1, 2, 0), "native world sprite bounds");
        var clone = UnityEngine.Object.Instantiate(renderer);
        Require(ReferenceEquals(clone.sprite, sprite) && clone.color == renderer.color, "clone retains immutable sprite identity and native renderer state");
        clone.sprite = null;
        Require(renderer.sprite != null, "clone state is independent");
        try { renderer.color = new Color(float.NaN, 0, 0); throw new Exception("invalid color accepted"); }
        catch (DisunityException) { }
        Require(renderer.color.a == 0.5f, "invalid write is atomic");
        renderer.enabled = false;
        Require(!renderer.enabled, "native renderer disable");
        SortingOrderReadsTheNativeRecord(renderer);
        AffineBounds();
        DisunityHost.UnloadWorld();
        try { _ = renderer.bounds; throw new Exception("stale renderer survived unload"); }
        catch (DisunityException) { }
        Console.WriteLine("PASS: native sprites, tint, flips, sorting, bounds, clone and unload");
    }
    private static void AffineBounds()
    {
        var parent = DisunityHost.CreateGameObject();
        parent.transform.localScale = new Vector3(2, 3, 1);
        parent.transform.position = new Vector3(3, -2, 1);
        var child = DisunityHost.CreateGameObject();
        child.transform.SetParent(parent.transform, false);
        child.transform.localRotation = new Quaternion(0, 0, MathF.Sin(MathF.PI / 8), MathF.Cos(MathF.PI / 8));
        var renderer = child.AddComponent<SpriteRenderer>();
        renderer.sprite = DisunityHost.CreateSprite(7, 8, 8, new Rect(0, 0, 2, 4), new Vector2(0.25f, 0.75f), 2);
        renderer.flipX = true;
        renderer.flipY = true;
        var c = MathF.Sqrt(0.5f);
        var corners = new[] { (-0.75f, -0.5f), (0.25f, -0.5f), (0.25f, 1.5f), (-0.75f, 1.5f) }
            .Select(p => new Vector3(3 + 2 * c * (p.Item1 - p.Item2), -2 + 3 * c * (p.Item1 + p.Item2), 1)).ToArray();
        var min = new Vector3(corners.Min(p => p.x), corners.Min(p => p.y), 1);
        var max = new Vector3(corners.Max(p => p.x), corners.Max(p => p.y), 1);
        var bounds = renderer.bounds;
        Require(MathF.Abs(bounds.center.x - (min.x + max.x) / 2) < 0.00001f
            && MathF.Abs(bounds.center.y - (min.y + max.y) / 2) < 0.00001f
            && MathF.Abs(bounds.size.x - (max.x - min.x)) < 0.00001f
            && MathF.Abs(bounds.size.y - (max.y - min.y)) < 0.00001f, "off-center flipped bounds enclose all sheared hierarchy corners");
    }
    private static void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
}
