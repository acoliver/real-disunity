using Disunity.Hosting;
using UnityEngine;

internal static class PhysicsMaterialTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var ball = DisunityHost.CreateGameObject();
        var body = ball.AddComponent<Rigidbody2D>();
        var collider = ball.AddComponent<CircleCollider2D>();
        Require(body.sharedMaterial == null && collider.sharedMaterial == null, "default material references");
        var material = new PhysicsMaterial2D("spring") { friction = 0.3f, bounciness = 1 };
        body.sharedMaterial = material;
        Require(ReferenceEquals(body.sharedMaterial, material), "shared body asset identity");
        collider.sharedMaterial = material;
        Require(ReferenceEquals(collider.sharedMaterial, material), "shared collider asset identity");
        material.friction = 0.6f;
        Require(collider.sharedMaterial!.friction == 0.6f, "shared edits are live");
        foreach (var bad in new[] { -1f, float.NaN, float.PositiveInfinity })
        {
            try { material.friction = bad; throw new Exception("invalid friction accepted"); }
            catch (DisunityException error) { Require(error.Reason == DisunityErrorReason.Physics, "typed friction error"); }
            try { material.bounciness = bad; throw new Exception("invalid bounce accepted"); }
            catch (DisunityException error) { Require(error.Reason == DisunityErrorReason.Physics, "typed bounce error"); }
        }
        Require(material.friction == 0.6f && material.bounciness == 1, "invalid writes are atomic");
        var clone = UnityEngine.Object.Instantiate(ball);
        Require(ReferenceEquals(clone.GetComponent<CircleCollider2D>()!.sharedMaterial, material), "clones share asset");
        collider.sharedMaterial = null;
        Require(collider.sharedMaterial == null && ReferenceEquals(body.sharedMaterial, material), "clearing collider retains body material");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: native-backed physics material identity, mutation, validation and clone sharing");
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
