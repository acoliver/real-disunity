using Disunity.Hosting;
using UnityEngine;
using static Check;

/// <summary>Snake from its scene plan: the GameConfig asset sets a 40x20 wrapping grid at 8 ticks
/// per second, segments are instantiated from the segment prefab, and steering the head onto the
/// food with W makes the snake grow.</summary>
internal static class SnakeScene
{
    private const string Segment = "SneakSegment(Clone)";

    public static void Run(Game game)
    {
        // Food appears at random cells; a fixed seed replays the same ones.
        UnityEngine.Random.InitState(3);
        DisunityHost.LoadWorld();
        try
        {
            using var scene = DisunityHost.LoadScene(game.Plan, game.Options);
            var food = scene.Objects.Single(o => o.name == "Food").transform;
            Require(Find(typeof(SpriteRenderer)).Cast<SpriteRenderer>().All(r => r.gameObject.name != "SneakSegment"),
                "the segment prefab is a template, not in the world");
            TicksAtTheConfiguredRate();
            EatsTheFood(food);
            Console.WriteLine("PASS: Snake loads from its plan, ticks at its configured rate, wraps and grows on food");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static Transform[] Segments() => Find(typeof(SpriteRenderer)).Cast<SpriteRenderer>()
        .Where(r => r.gameObject.name == Segment && r.gameObject.activeInHierarchy)
        .Select(r => r.transform).ToArray();

    private static Vector2Int Cell(Transform transform) =>
        new((int)Math.Round(transform.position.x), (int)Math.Round(transform.position.y));

    /// <summary>The first tick is due after 1/8 s: 0.12 s draws nothing, 0.14 s draws the head
    /// one cell right of the grid's centre.</summary>
    private static void TicksAtTheConfiguredRate()
    {
        Steps(6);
        Require(Segments().Length == 0, "no segment is drawn before the first tick");
        Steps(1);
        var segments = Segments();
        Require(segments.Length == 1 && Cell(segments[0]) == new Vector2Int(21, 10),
            $"the head starts at the centre of the 40x20 grid and moves right, got {string.Join(", ", segments.Select(Cell))}");
    }

    /// <summary>Keeps moving right, wrapping at x 40, until the head reaches the food's column,
    /// then turns up with W, wrapping at y 20, until it eats.</summary>
    private static void EatsTheFood(Transform food)
    {
        var target = Cell(food);
        Require(target.x is >= 0 and < 40 && target.y is >= 0 and < 20, $"food lies on the grid, got {target}");
        var turned = false;
        for (var frame = 0; frame < 600 && Segments().Length < 2; frame++)
        {
            if (!turned && Cell(Segments()[0]).x == target.x)
            {
                Key(KeyCode.W, true);
                Steps(1);
                Key(KeyCode.W, false);
                turned = true;
            }
            Steps(1);
        }
        var segments = Segments();
        Require(segments.Length == 2, $"the snake grows after eating, got {segments.Length} segments");
        Require(Cell(food) != target, "new food is spawned");
    }
}
