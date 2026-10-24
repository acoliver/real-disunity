using System.Text.Json;
using UnityEngine;

internal static class TileColliderBehavior
{
    public static void Run()
    {
        var owner = new GameObject("native-tile-collider");
        using var added = NativeWorld.Request("component.add", ("object", owner.NativeHandle),
            ("kind", "TilemapCollider2D"), ("fields", new { cells = Array.Empty<object>() }));
        var handle = NativeWorld.Data(added).GetProperty("handle").GetUInt64();
        var resolved = NativeIdentity.Resolve(handle);
        BinderBehavior.Check(resolved.GetType().FullName == "UnityEngine.Tilemaps.TilemapCollider2D",
            "native tile collider must preserve its public namespace/type identity");
        BinderBehavior.Check(resolved is Collider2D, "native tile collider must share Collider2D forwarding");
        var collider = (Collider2D)resolved;
        collider.isTrigger = true;
        collider.offset = new Vector2(3, 4);
        using var native = NativeWorld.Request("component.get", ("component", handle));
        var fields = NativeWorld.Data(native).GetProperty("fields");
        BinderBehavior.Check(fields.GetProperty("is_trigger").GetBoolean()
            && fields.GetProperty("offset")[0].GetSingle() == 3,
            "tile collider fields must be native authoritative");
        BinderBehavior.Check(ReferenceEquals(resolved, owner.GetComponent<Collider2D>()),
            "tile collider identity must be shared by base-component lookup");
        BinderBehavior.Check(collider.attachedRigidbody is null, "wall collider must remain static without a body");
        Console.WriteLine("PASS tile collider: native identity, inherited forwarding and static ownership");
    }
}
