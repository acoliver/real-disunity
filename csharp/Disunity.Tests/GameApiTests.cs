using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Events;
using UnityEngine.InputSystem;

internal static class GameApiTests
{
    private sealed class Child : MonoBehaviour
    {
        public static readonly List<bool> Edges = new();
        private void Awake() => Edges.Add(Input.GetKeyDown(KeyCode.Space));
        private void OnEnable() => Edges.Add(Input.GetKeyDown(KeyCode.Space));
    }
    private sealed class Parent : MonoBehaviour
    {
        public readonly List<bool> Edges = new();
        private void FixedUpdate()
        {
            Edges.Add(Input.GetKeyDown(KeyCode.Space));
            DisunityHost.AddComponent(new Child());
            Edges.Add(Input.GetKeyDown(KeyCode.Space));
        }
    }
    public static void Run()
    {
        DisunityHost.LoadWorld();
        Child.Edges.Clear();
        var parent = DisunityHost.AddComponent(new Parent());
        DisunityHost.PushKey(0, KeyCode.Space, true);
        DisunityHost.Step(0.041f);
        Require(parent.Edges.SequenceEqual(new[] { true, true, false, false }), "parent fixed phase is restored");
        Require(Child.Edges.SequenceEqual(new[] { true, true, false, false }), "nested lifecycle inherits fixed projection");
        Require(MathF.Abs(Time.fixedDeltaTime - 0.02f) < 0.000001f, "fixed delta comes from native clock");
        DisunityHost.UnloadWorld();
        TestValues();
        TestNativeProperties();
        Console.WriteLine("PASS: public math, events, native transform and nested input phase APIs");
    }
    private static void TestValues()
    {
        var cell = new Vector2Int(4, 6) / 2 + Vector2Int.left;
        Require(cell == new Vector2Int(1, 3), "integer vector arithmetic");
        Vector3 position = new Vector2(3, 4);
        Require(position.x == 3 && ((Vector2)position).sqrMagnitude == 25, "vector conversion and magnitude");
        Require(new Vector3(2, 3) * 2 == new Vector3(4, 6, 0), "two-dimensional vector construction");
        var signal = new UnityEvent<int>();
        var received = 0;
        UnityAction<int> listener = value => received = value;
        signal.AddListener(listener);
        signal.Invoke(7);
        signal.RemoveListener(listener);
        signal.Invoke(9);
        Require(received == 7, "event listener removal");
        Require(Mathf.Clamp(3, -1, 1) == 1 && Mathf.Min(2f, 3f) == 2, "math helpers");
        for (var i = 0; i < 100; i++) Require(UnityEngine.Random.Range(1, 3) is >= 1 and < 3, "integer random bounds");
    }
    private sealed class Marker : MonoBehaviour
    {
        public Quaternion Facing = Quaternion.identity;
        public Vector2Int Cell = new(3, 7);
        public static Vector3 LastAwakePosition;
        private void Awake() => LastAwakePosition = transform.position;
    }
    private static void TestNativeProperties()
    {
        DisunityHost.LoadWorld();
        var value = DisunityHost.CreateGameObject();
        value.name = "probe";
        value.tag = "Actor";
        Require(value.name == "probe" && value.CompareTag("Actor"), "native object metadata");
        Require(ReferenceEquals(GameObject.FindGameObjectWithTag("Actor"), value), "native tag lookup identity");
        Require(value.layer == 0, "objects start on the Default layer");
        value.layer = 8;
        Require(value.layer == 8, "native layer round trip");
        try { value.layer = 32; throw new InvalidOperationException("layer 32 was accepted"); }
        catch (ArgumentException) { }
        var marker = value.AddComponent<Marker>();
        Require(ReferenceEquals(UnityEngine.Object.FindFirstObjectByType<Marker>(), marker), "native eligibility for type lookup");
        value.transform.localScale = new(2, 3, 4);
        Require(value.transform.localScale == new Vector3(2, 3, 4), "native scale round trip");
        var clone = UnityEngine.Object.Instantiate(value, new Vector3(4, 5, 6), Quaternion.identity);
        Require(clone.transform.position == new Vector3(4, 5, 6) && Marker.LastAwakePosition == new Vector3(4, 5, 6), "clone world pose before activation");
        Require(clone.transform.rotation == Quaternion.identity, "clone world rotation");
        Require(clone.layer == 8 && clone.name == "probe(Clone)", "clones keep the layer and take Unity's clone name");
        Require(clone.GetComponent<Marker>()!.Facing == Quaternion.identity && clone.GetComponent<Marker>()!.Cell == new Vector2Int(3, 7), "clone copies quaternion and integer vector values");
        DisunityHost.PushKey(0, KeyCode.A, true);
        DisunityHost.Step(0.01f);
        Require(Keyboard.current.aKey.isPressed, "keyboard reads native input");
        var body = value.AddComponent<Rigidbody2D>();
        body.bodyType = RigidbodyType2D.Kinematic;
        body.MovePosition(new(1, 2));
        Require(body.position == Vector2.zero, "MovePosition waits for native fixed physics");
        DisunityHost.Step(0.02f);
        Require((body.position - new Vector2(1, 2)).magnitude < 0.0001f, "native MovePosition reaches target");
        DisunityHost.UnloadWorld();
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
