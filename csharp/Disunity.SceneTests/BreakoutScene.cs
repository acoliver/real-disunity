using Disunity.Hosting;
using UnityEngine;
using static Check;

/// <summary>Breakout from its scene plan: the brick and ball prefabs stay templates, Start
/// instantiates a brick per column and brick type and launches a ball, and the tile walls keep
/// the ball in play.</summary>
internal static class BreakoutScene
{
    public static void Run(Game game)
    {
        DisunityHost.LoadWorld();
        try
        {
            using var scene = DisunityHost.LoadScene(game.Plan, game.Options);
            var brick = game.Scripts.GetType("Brick", true)!;
            var ball = game.Scripts.GetType("Ball", true)!;
            var spawner = game.Scripts.GetType("BrickSpawner", true)!;
            Require(Find(brick).Length == 0 && Find(ball).Length == 0, "prefab templates are not in the world");
            Require(scene.Differences.Any(d => d.Contains("tiles draw unlit")), "the lit tilemap is reported");
            Steps(1);
            Bricks(scene, Find(spawner).Single(), Find(brick));
            var balls = Find(ball);
            Require(balls.Length == 1 && ((Component)balls[0]).gameObject.name == "Ball(Clone)", "BallSpawner.Start instantiates one ball");
            var speed = ((Component)balls[0]).GetComponent<Rigidbody2D>()!.velocity.magnitude;
            Require(Math.Abs(speed - 6) < 1e-3, $"the ball keeps Ball.speed's initializer 6 over the prefab's stale startSpeed, got {speed}");
            StaysInPlay(ball);
            Console.WriteLine("PASS: Breakout loads from its plan, instantiates its prefabs and keeps its ball inside the tile walls");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static void Bricks(LoadedScene scene, UnityEngine.Object spawner, UnityEngine.Object[] bricks)
    {
        var rows = ((Array)spawner.GetType().GetField("brickTypes")!.GetValue(spawner)!).Length;
        var parent = scene.Objects.Single(o => o.name == "BrickSpawner").transform;
        var objects = bricks.Select(b => ((Component)b).gameObject).ToList();
        Require(rows > 1 && objects.Count == rows * 8, $"one brick per column and brick type, got {objects.Count} for {rows} types");
        Require(objects.All(o => o.name == "Brick(Clone)" && o.transform.parent == parent), "bricks are clones under the spawner");
        var sprites = objects.Select(o => o.GetComponent<SpriteRenderer>()!.sprite!.rect).Distinct().Count();
        Require(sprites == rows, $"Brick.Setup gives each row its type's sprite, got {sprites} sprites");
    }

    /// <summary>The left wall's cells span x from -10 to -8. A ball sent at it must bounce
    /// with its centre a radius from the wall's face, then keep playing inside the walls.</summary>
    private static void StaysInPlay(Type ball)
    {
        var body = ((Component)Find(ball).Single()).GetComponent<Rigidbody2D>()!;
        body.transform.position = new Vector3(-6, 1, 0);
        body.velocity = new Vector2(-6, 0);
        var leftmost = float.MaxValue;
        for (var step = 0; step < 50; step++)
        {
            Steps(1);
            leftmost = Math.Min(leftmost, body.position.x);
        }
        Require(leftmost > -8.05f && leftmost < -7.3f && body.velocity.x > 0,
            $"the ball bounces off the sprite-shaped wall tiles, reaching x {leftmost} with velocity {body.velocity}");
        for (var step = 0; step < 200; step++)
        {
            Steps(1);
            foreach (var position in Find(ball).Select(b => ((Component)b).transform.position))
                Require(Math.Abs(position.x) < 10.7f && position.y < 10 && position.y > -6.5f,
                    $"a ball left the tile walls at {position} after {step + 1} steps");
        }
    }
}
