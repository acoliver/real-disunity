using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>GameObject constructors, component searches (and Component's forwards), Find, SendMessage and scene membership.</summary>
internal static class GameObjectMemberTests
{
    private interface IMarker { }

    private class Base : MonoBehaviour, IMarker { }

    private sealed class Derived : Base { }

    private sealed class Other : MonoBehaviour { }

    private sealed class Receiver : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        private void Ping() => Calls.Add("ping");
        public void Value(int number) => Calls.Add("int " + number);
        public void Text(string text) => Calls.Add("string " + (text ?? "null"));
        public void Throwing() => throw new InvalidOperationException("receiver failed");
        public System.Collections.IEnumerator Routine() { Calls.Add("routine"); yield break; }
    }

    private sealed class Second : MonoBehaviour
    {
        public int Pings;
        public void Throwing() => Pings++;
    }

    private const string Plan = """
        {"scene":"level3","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"A","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[]},
        {"name":"B","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[]},
        {"name":"A1","tag":"Untagged","layer":0,"active":true,"parent":0,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[]},
        {"name":"Prefab","tag":"Untagged","layer":0,"active":true,"parent":null,"template":true,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[]}]}
        """;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Constructors();
            Queries();
            ComponentForwards();
            FindByNameAndTag();
            Messages();
            Primitives();
        }
        finally { DisunityHost.UnloadWorld(); }
        SceneMembership();
        Console.WriteLine("PASS: GameObject constructors, component searches, Find, FindWithTag, SendMessage, CreatePrimitive and scene membership");
    }

    private static void Constructors()
    {
        var plain = new GameObject();
        Ensure(plain.name == "New Game Object" && plain.activeSelf && plain.transform != null, "the default constructor makes an active object");
        Ensure(ReferenceEquals(plain.gameObject, plain), "GameObject.gameObject is itself");
        var named = new GameObject("Hero");
        Ensure(named.name == "Hero", "the name constructor");
        var full = new GameObject("Full", typeof(Base), typeof(Other));
        Ensure(full.GetComponent<Base>() != null && full.GetComponent<Other>() != null, "the component constructor attaches each type");
        Ensure(GameObject.Find("Hero") == named, "constructed objects are findable");
        Throws<ArgumentException>(() => full.AddComponent(typeof(string)), "a non-component type");
        Ensure(full.AddComponent(typeof(Derived)) is Derived, "AddComponent(Type) returns the new component");
        Ensure(!named.isStatic, "isStatic starts false");
        named.isStatic = true;
        Ensure(named.isStatic, "isStatic is stored");
    }

    private static GameObject Tree(out GameObject child, out GameObject hidden, out GameObject grandchild)
    {
        var root = new GameObject("TreeRoot", typeof(Base));
        child = new GameObject("TreeChild", typeof(Derived));
        hidden = new GameObject("TreeHidden", typeof(Other));
        grandchild = new GameObject("TreeGrand", typeof(Other));
        child.transform.SetParent(root.transform, false);
        hidden.transform.SetParent(root.transform, false);
        grandchild.transform.SetParent(child.transform, false);
        hidden.SetActive(false);
        return root;
    }

    private static void Queries()
    {
        var root = Tree(out var child, out var hidden, out var grandchild);
        Ensure(root.GetComponent(typeof(Base)) is Base && root.GetComponent(typeof(Other)) is null, "GetComponent(Type)");
        Ensure(root.TryGetComponent<Base>(out var found) && found != null && !root.TryGetComponent<Other>(out _), "TryGetComponent<T>");
        Ensure(root.TryGetComponent(typeof(IMarker), out var marker) && marker is Base && !root.TryGetComponent(typeof(Other), out _), "TryGetComponent(Type)");
        Ensure(root.GetComponents<Component>().Length == 2 && root.GetComponents(typeof(Transform)).Length == 1, "GetComponents counts the transform");
        var list = new List<Component> { root.transform };
        root.GetComponents(typeof(Base), list);
        Ensure(list.Count == 1 && list[0] is Base, "GetComponents(Type, List) replaces the list");
        var markers = new List<IMarker>();
        root.GetComponentsInChildren(markers);
        Ensure(markers.Count == 2, "GetComponentsInChildren(List) finds the active ones");
        Ensure(root.GetComponentsInChildren<Other>().Length == 1, "an inactive child's components are left out");
        Ensure(root.GetComponentsInChildren<Other>(true).Length == 2, "includeInactive brings them back");
        Ensure(root.GetComponentsInChildren(typeof(Other), true).Length == 2, "the Type overload takes includeInactive");
        Ensure(root.GetComponentsInChildren(typeof(Other)).Length == 1, "and defaults to active only");
        root.GetComponentsInChildren(true, markers);
        Ensure(markers.Count == 2, "GetComponentsInChildren(bool, List)");
        Ensure(root.GetComponentInChildren<Derived>() is { } d && ReferenceEquals(d.gameObject, child), "GetComponentInChildren finds the descendant");
        Ensure(root.GetComponentInChildren<Other>() is { } o && ReferenceEquals(o.gameObject, grandchild), "depth first skips the inactive child");
        Ensure(root.GetComponentInChildren<Other>(true) is { } first && ReferenceEquals(first.gameObject, grandchild), "children come in sibling order");
        Ensure(root.GetComponentInChildren(typeof(Other), true) != null, "GetComponentInChildren(Type, bool)");
        Ensure(grandchild.GetComponentInParent<Base>() is Derived, "GetComponentInParent starts at the nearest ancestor");
        Ensure(grandchild.GetComponentInParent(typeof(Base)) is Derived, "GetComponentInParent(Type)");
        Ensure(grandchild.GetComponentsInParent<Base>().Length == 2 && grandchild.GetComponentsInParent<Base>()[0] is Derived, "GetComponentsInParent goes up in order");
        Ensure(hidden.GetComponentInParent<Base>() != null, "an inactive object still sees its active parents");
        Ensure(hidden.GetComponentInParent<Base>(true) != null && hidden.GetComponentsInParent<Base>(true).Length == 1, "includeInactive reaches from an inactive object");
        Ensure(hidden.GetComponentsInParent(typeof(Base), true).Length == 1 && grandchild.GetComponentsInParent(typeof(Base)).Length == 2, "the Type overloads");
        var up = new List<Base>();
        grandchild.GetComponentsInParent(false, up);
        Ensure(up.Count == 2, "GetComponentsInParent(bool, List)");
        var all = new List<Component>();
        root.GetComponents(all);
        Ensure(all.Count == 2, "GetComponents<T>(List)");
        child.SetActive(false);
        Ensure(grandchild.GetComponentsInParent<Base>().Length == 1, "an inactive ancestor's components are left out");
        Ensure(grandchild.GetComponentsInParent<Base>(true).Length == 2, "includeInactive brings them back");
        Ensure(root.GetComponentsInChildren<Derived>().Length == 0 && root.GetComponentsInChildren<Derived>(true).Length == 1, "and an inactive descendant's too");
    }

    private static void ComponentForwards()
    {
        var root = Tree(out var child, out _, out var grandchild);
        var script = root.GetComponent<Base>()!;
        Ensure(script.GetComponent(typeof(Base)) == script && script.TryGetComponent<Transform>(out var transform) && transform == root.transform, "Component.GetComponent and TryGetComponent");
        Ensure(script.TryGetComponent(typeof(Other), out _) == false, "Component.TryGetComponent(Type) misses");
        Ensure(script.GetComponents<Component>().Length == 2 && script.GetComponents(typeof(Base)).Length == 1, "Component.GetComponents");
        Ensure(script.GetComponentInChildren<Derived>() != null && script.GetComponentInChildren(typeof(Derived)) != null && script.GetComponentInChildren<Other>(true) != null, "Component.GetComponentInChildren");
        Ensure(script.GetComponentsInChildren<Other>().Length == 1 && script.GetComponentsInChildren<Other>(true).Length == 2 && script.GetComponentsInChildren(typeof(Other), true).Length == 2, "Component.GetComponentsInChildren");
        var below = child.GetComponent<Derived>()!;
        Ensure(below.GetComponentInParent<Base>() == below && below.GetComponentsInParent<Base>().Length == 2 && below.GetComponentInParent(typeof(Base), false) != null, "Component.GetComponentInParent on its own object");
        Ensure(below.GetComponentsInParent(typeof(Base)).Length == 2 && below.GetComponentsInParent<Base>(true).Length == 2, "Component.GetComponentsInParent");
        var results = new List<Other>();
        script.GetComponentsInChildren(true, results);
        Ensure(results.Count == 2, "Component list overload");
        Ensure(grandchild.transform.GetComponentInParent<Derived>() != null, "a Transform searches too");
    }

    private static void FindByNameAndTag()
    {
        var shown = new GameObject("FindShown");
        var hidden = new GameObject("FindHidden");
        hidden.SetActive(false);
        shown.tag = "Finder";
        Ensure(GameObject.Find("FindShown") == shown, "Find an active object");
        Ensure(GameObject.Find("FindHidden") is null, "Find skips inactive objects");
        Ensure(GameObject.Find("FindMissing") is null, "Find misses");
        var parent = new GameObject("FindParent");
        var kid = new GameObject("FindKid");
        kid.transform.SetParent(parent.transform, false);
        Ensure(GameObject.Find("FindParent/FindKid") == kid && GameObject.Find("FindParent") == parent, "a path starts at a root");
        Ensure(GameObject.Find("FindKid/FindParent") is null, "and must name a root first");
        kid.SetActive(false);
        Ensure(GameObject.Find("FindParent/FindKid") is null, "an inactive object at the end of a path is not found");
        Ensure(GameObject.FindWithTag("Finder") == shown && GameObject.FindGameObjectWithTag("Finder") == shown, "FindWithTag");
        Ensure(GameObject.FindWithTag("NoSuchTag") is null, "FindWithTag misses");
        var second = new GameObject("FindSecond") { tag = "Finder" };
        Ensure(GameObject.FindGameObjectsWithTag("Finder").Length == 2 && second.CompareTag("Finder"), "FindGameObjectsWithTag");
        Throws<ArgumentNullException>(() => GameObject.Find(null!), "Find(null)");
    }

    private static void Messages()
    {
        var go = new GameObject("Messages", typeof(Receiver), typeof(Second));
        var receiver = go.GetComponent<Receiver>()!;
        go.SendMessage("Ping");
        go.SendMessage("Value", 7);
        go.SendMessage("Text", "hi");
        go.SendMessage("Text");
        go.SendMessage("Ping", 5);
        Ensure(receiver.Calls.SequenceEqual(["ping", "int 7", "string hi", "string null", "ping"]), "SendMessage reaches private and public methods, with and without a value");
        go.SendMessage("Routine");
        DisunityHost.Step(0.01f);
        Ensure(receiver.Calls.Count == 6 && receiver.Calls[5] == "routine", "a receiver that returns an iterator runs as a coroutine");
        using (var log = new LogRecorder())
        {
            go.SendMessage("Missing");
            Ensure(log.Messages.Count == 1 && log.Messages[0].StartsWith("Error:") && log.Messages[0].Contains("SendMessage Missing has no receiver!"), "RequireReceiver logs an error");
            go.SendMessage("Missing", SendMessageOptions.DontRequireReceiver);
            go.SendMessage("Value", "wrong type", SendMessageOptions.DontRequireReceiver);
            Ensure(log.Messages.Count == 1, "DontRequireReceiver stays quiet, and an argument of the wrong type finds no receiver");
            go.SendMessage("Throwing");
            Ensure(go.GetComponent<Second>()!.Pings == 1, "a receiver that throws does not stop the others");
            Ensure(log.Messages.Count == 2 && log.Messages[1].Contains("receiver failed"), "and its exception is logged");
        }
        go.SetActive(false);
        using (var log = new LogRecorder())
        {
            go.SendMessage("Ping", SendMessageOptions.RequireReceiver);
            Ensure(receiver.Calls.Count == 6 && log.Messages.Count == 1, "an inactive object receives nothing");
        }
    }

    private static void Primitives() =>
        Throws<NotSupportedException>(() => GameObject.CreatePrimitive(PrimitiveType.Cube), "CreatePrimitive", "CreatePrimitive", "not supported yet");

    private static void SceneMembership()
    {
        DisunityHost.LoadWorld();
        try
        {
            var before = new GameObject("BeforeScene");
            Ensure(!before.scene.IsValid() && before.scene.name == "" && before.scene.buildIndex == -1 && before.sceneCullingMask == 0, "without a loaded scene the scene is invalid");
            using var loaded = DisunityHost.LoadScene(Plan, new SceneLoadOptions(typeof(GameObjectMemberTests).Assembly, _ => []));
            var a = GameObject.Find("A")!;
            var scene = a.scene;
            Ensure(scene.IsValid() && scene.isLoaded && scene.name == "level3" && scene.buildIndex == 3 && scene.handle != 0, "the scene is the loaded one");
            Ensure(a.sceneCullingMask == 1UL && GameObject.Find("A1")!.scene == scene, "objects of the scene share it");
            var runtime = new GameObject("Runtime");
            Ensure(runtime.scene == scene, "an object made at run time joins the loaded scene");
            Ensure(loaded.Objects.Single(value => value.name == "Prefab").scene.IsValid() == false, "a prefab template belongs to no scene");
            var roots = scene.GetRootGameObjects();
            Ensure(roots.Select(value => value.name).SequenceEqual(["BeforeScene", "A", "B", "Runtime"]) && scene.rootCount == 4, "the roots, in sibling order and without templates");
            var list = new List<GameObject> { a };
            scene.GetRootGameObjects(list);
            Ensure(list.Count == 4, "GetRootGameObjects(List) replaces the list");
            Ensure(default(UnityEngine.SceneManagement.Scene).GetRootGameObjects().Length == 0, "an invalid scene has no roots");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
