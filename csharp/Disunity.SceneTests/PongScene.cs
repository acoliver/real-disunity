using Disunity.Hosting;
using UnityEngine;
using UnityEngine.UI;
using static Check;

/// <summary>Pong from its scene plan: the ball launches, the left paddle follows the Vertical
/// axis, and a goal scores through the UnityEvent the scene wires to the match manager.</summary>
internal static class PongScene
{
    public static void Run(Game game)
    {
        DisunityHost.LoadWorld();
        try
        {
            using var scene = DisunityHost.LoadScene(game.Plan, game.Options);
            GameObject Named(string name) => scene.Objects.Single(o => o.name == name);
            var scores = new[] { Named("PlayerOneScore"), Named("PlayerTwoScore") }.Select(o => o.GetComponent<Text>()!).ToArray();
            Require(scores.All(s => s.text == "0"), "ScoreView.Awake shows zero");
            Require(Named("Ball").layer == 8 && Named("Canvas").layer == 5, "layers load");
            Require(scene.UnregisteredAxes.Any(a => a.StartsWith("Fire1: no key for `left ctrl`")), "axes without engine keys are reported");
            Require(scene.Differences.All(d => !d.Contains("polate")), "interpolation is applied, not reported");
            Require(scene.Differences.All(d => !d.Contains("pixel perfect")) && Named("Canvas").GetComponent<Canvas>()!.pixelPerfect,
                "the canvas is pixel perfect as authored, not reported");
            Require(new[] { "Ball", "Player", "Enemy" }.All(n => Named(n).GetComponent<Rigidbody2D>()!.interpolation == RigidbodyInterpolation2D.Interpolate),
                "the ball and both paddles interpolate as authored");
            Launches(Named("Ball").GetComponent<Rigidbody2D>()!);
            PaddleFollowsAxis(Named("Player"), Named("Enemy"));
            GoalScores(Named("Ball"), scores);
            Console.WriteLine("PASS: Pong loads from its plan, launches, reads its axes and scores through its UnityEvent");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static void Launches(Rigidbody2D ball)
    {
        Steps(1);
        var velocity = ball.velocity;
        Require(Math.Abs(Math.Abs(velocity.x) - 12) < 1e-3 && Math.Abs(Math.Abs(velocity.y) - 12) < 1e-3,
            $"Ball.Start launches diagonally at its authored speed 12, got {velocity}");
    }

    private static void PaddleFollowsAxis(GameObject player, GameObject enemy)
    {
        var (start, other) = (player.transform.position.y, enemy.transform.position.y);
        Key(KeyCode.W, true);
        Steps(10);
        Key(KeyCode.W, false);
        Steps(1);
        var moved = player.transform.position.y - start;
        Require(moved > 3.5f && moved < 4.5f, $"W moves the Vertical paddle at 20 units/s for 0.2 s, moved {moved}");
        Require(Math.Abs(enemy.transform.position.y - other) < 1e-3, "the AlternateVertical paddle ignores W");
    }

    private static void GoalScores(GameObject ball, Text[] scores)
    {
        var body = ball.GetComponent<Rigidbody2D>()!;
        ball.transform.position = new Vector3(11, 0, 0);
        body.velocity = new Vector2(12, 0);
        for (var i = 0; i < 30 && scores.All(s => s.text == "0"); i++) Steps(1);
        Require(scores.Count(s => s.text == "1") == 1 && scores.Count(s => s.text == "0") == 1,
            $"the goal's scoreEvent calls MatchManager.ScorePoint, scores {string.Join(",", scores.Select(s => s.text))}");
        Require(!ball.GetComponent<SpriteRenderer>()!.enabled, "ScorePoint hides the ball");
        Require(ball.transform.position == Vector3.zero, "ScorePoint returns the ball to its start");
        Steps(110);
        Require(ball.GetComponent<SpriteRenderer>()!.enabled && body.velocity.magnitude > 16, "the ball relaunches after timeBeforeLaunch");
    }
}
