using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Tilemaps;

/// Breakout's wall: a 2x1 grid of brick tiles whose Sprite collider bounces the ball.
internal static class TilemapTests
{
    private sealed class Ball : MonoBehaviour
    {
        public Collider2D? Hit;
        private void OnCollisionEnter2D(Collision2D collision) => Hit ??= collision.collider;
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        var (tilemap, wall) = BreakoutWall();
        Require(tilemap.tileAnchor == new Vector3(0.5f, 0.5f, 0), "tile anchor");
        Require(Physics2D.Raycast(Vector2.zero, new Vector2(-1, 0), 20).collider == wall, "the left wall blocks a ray");
        var ball = DisunityHost.AddComponent(new Ball());
        ball.transform.position = new Vector3(-7, 0, 0);
        var body = ball.gameObject.AddComponent<Rigidbody2D>();
        (body.gravityScale, body.velocity) = (0, new Vector2(-10, 0));
        ball.gameObject.AddComponent<CircleCollider2D>().radius = 0.25f;
        for (var i = 0; i < 30 && ball.Hit is null; i++) DisunityHost.Step(0.02f);
        Require(ball.Hit is TilemapCollider2D hit && ReferenceEquals(hit, wall) && hit.gameObject == tilemap.gameObject,
            "the ball's collision names the tilemap's collider");
        UnityEngine.Object.Destroy(ball.gameObject);
        DisunityHost.Step(0.02f);
        DisunityHost.SetCells(tilemap, [(new Vector2Int(4, 0), 0)]);
        Require(Physics2D.Raycast(Vector2.zero, new Vector2(-1, 0), 20).collider is null, "cell edits open the wall");
        Rejects(() => DisunityHost.SetCells(tilemap, [(Vector2Int.zero, 0), (Vector2Int.zero, 0)]), "repeated cell");
        Rejects(() => DisunityHost.SetCells(tilemap, [(Vector2Int.zero, 1)]), "cell outside the palette");
        Rejects(() => DisunityHost.ConfigureTilemap(tilemap, new TilemapSettings(new Vector2(2, 0), default, default)),
            "flat cells");
        Require(Physics2D.Raycast(new Vector2(0, 0.5f), new Vector2(1, 0), 20).collider == wall, "rejected edits keep the cell");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: tilemap layout, palette and cells drive TilemapCollider2D contacts");
    }

    private static (Tilemap, TilemapCollider2D) BreakoutWall()
    {
        var owner = DisunityHost.CreateGameObject();
        var tilemap = owner.AddComponent<Tilemap>();
        var wall = owner.AddComponent<TilemapCollider2D>();
        DisunityHost.ConfigureTilemap(tilemap, new TilemapSettings(new Vector2(2, 1), Vector2.zero, new Vector2(0.5f, 0.5f)));
        var brick = DisunityHost.CreateSprite(7, 64, 64, new Rect(0, 0, 32, 16), new Vector2(0.5f, 0.5f), 16);
        Vector2[] outline = [new(-1, -0.5f), new(1, -0.5f), new(1, 0.5f), new(-1, 0.5f)];
        DisunityHost.SetTile(tilemap, 0, new TileDefinition(brick, Color.white, TileColliderType.Sprite, [outline]));
        var cells = new List<(Vector2Int, int)>();
        for (var x = -5; x <= 4; x++) cells.AddRange([(new Vector2Int(x, -6), 0), (new Vector2Int(x, 9), 0)]);
        for (var y = -5; y <= 8; y++) cells.AddRange([(new Vector2Int(-5, y), 0), (new Vector2Int(4, y), 0)]);
        DisunityHost.SetCells(tilemap, cells);
        return (tilemap, wall);
    }

    private static void Rejects(Action action, string message)
    {
        try { action(); }
        catch (DisunityException) { return; }
        throw new InvalidOperationException($"accepted: {message}");
    }

    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
