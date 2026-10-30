using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Tilemaps;

/// <summary>Camera, Canvas, Collider2D and Tilemap are behaviours, so a game compiled to reach <c>enabled</c>
/// through <see cref="Behaviour"/> reads and writes the component's own native state.</summary>
internal static class BehaviourTypeTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var owner = new GameObject("Behaviours");
            Behaviour[] behaviours =
            [
                owner.AddComponent<Camera>(),
                new GameObject("Canvas").AddComponent<Canvas>(),
                new GameObject("Box").AddComponent<BoxCollider2D>(),
                new GameObject("Grid").AddComponent<Tilemap>(),
            ];
            foreach (var behaviour in behaviours) Toggles(behaviour);
            Expect.That(owner.GetComponents<Behaviour>().Single() == behaviours[0], "a camera is found as a Behaviour");
            Expect.That(owner.GetComponents<MonoBehaviour>().Length == 0, "a camera is not a MonoBehaviour");
            Console.WriteLine("PASS: Camera, Canvas, Collider2D and Tilemap are behaviours whose enabled state is native");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void Toggles(Behaviour behaviour)
    {
        var name = behaviour.GetType().Name;
        Expect.That(behaviour.enabled && behaviour.isActiveAndEnabled, $"{name} starts enabled");
        behaviour.enabled = false;
        Expect.That(!behaviour.enabled && !behaviour.isActiveAndEnabled, $"{name} disables through Behaviour.enabled");
        behaviour.enabled = true;
        Expect.That(behaviour.enabled, $"{name} enables through Behaviour.enabled");
    }
}
