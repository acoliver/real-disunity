using System.Reflection;
using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>UnityEngine.Object's name, hideFlags, instance IDs, ToString, destruction and searches.</summary>
internal static class ObjectModelTests
{
    private sealed class Probe : MonoBehaviour { }

    private sealed class Finder : MonoBehaviour { }

    private sealed class Spawned : MonoBehaviour { }

    private sealed class Asset : ScriptableObject { }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Names();
            HideFlagsAndIds();
            ToStringFormat();
            AssetDestruction();
            AssetCloning();
            AsyncCloning();
            AsyncCloningTiming();
            AsyncCloningFailure();
            InstantiateParameterPlacement();
            DontDestroy();
            Searches();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: Object name, hideFlags, instance IDs, ToString, asset destruction, DontDestroyOnLoad and the Find overloads");
    }

    private static void Names()
    {
        var go = DisunityHost.CreateGameObject();
        go.name = "Hero";
        var component = go.AddComponent<Probe>();
        Ensure(go.name == "Hero" && component.name == "Hero", "a component's name is its GameObject's");
        component.name = "Renamed";
        Ensure(go.name == "Renamed" && component.name == "Renamed", "renaming a component renames the GameObject");
        Ensure(go.transform.name == "Renamed", "the transform is a component too");

        var texture = new Texture2D(2, 2);
        Ensure(texture.name == "", "an asset starts unnamed");
        texture.name = "Ground";
        var material = new Material(DisunityHost.RegisterShader("Test/Names"));
        material.name = "Mat";
        var text = new TextAsset("body") { name = "notes" };
        var asset = ScriptableObject.CreateInstance<Asset>();
        asset.name = "data";
        Ensure(texture.name == "Ground" && material.name == "Mat" && text.name == "notes" && asset.name == "data", "assets keep their own names");
        Ensure(DisunityHost.RegisterShader("Test/Names").name == "Test/Names", "a shader's name is the registered name");
        Throws<ArgumentNullException>(() => texture.name = null!, "a null name is rejected");
        Throws<ArgumentNullException>(() => go.name = null!, "a null GameObject name is rejected");
        var loose = new Probe();
        Ensure(loose.name == "" && loose.ToString().StartsWith(" ("), "a component not attached to a GameObject has an empty name");
        Throws<InvalidOperationException>(() => loose.name = "x", "naming a component before it is attached", "not attached");
    }

    private static void HideFlagsAndIds()
    {
        var go = DisunityHost.CreateGameObject();
        var texture = new Texture2D(2, 2);
        Ensure(go.hideFlags == HideFlags.None && texture.hideFlags == HideFlags.None, "hideFlags start empty");
        texture.hideFlags = HideFlags.HideAndDontSave;
        go.hideFlags = HideFlags.NotEditable | HideFlags.HideInHierarchy;
        Ensure(texture.hideFlags == HideFlags.HideAndDontSave && go.hideFlags == (HideFlags)9, "hideFlags are stored");

        var objects = new UnityEngine.Object[]
        {
            go, go.transform, go.AddComponent<Probe>(), texture, new Texture2D(1, 1), new Material(Material.SpriteLit),
            new RenderTexture(4, 4, 0), DisunityHost.RegisterShader("Test/Ids"), new TextAsset("x"), ScriptableObject.CreateInstance<Asset>(),
        };
        var ids = objects.Select(value => value.GetInstanceID()).ToArray();
        Ensure(ids.All(id => id != 0) && ids.Distinct().Count() == ids.Length, "instance IDs are non-zero and unique");
        Ensure(ids.SequenceEqual(objects.Select(value => value.GetInstanceID())), "an instance ID does not change");
        Ensure(ids.All(id => id < 0 && id % 2 == 0), "run-time objects have negative even IDs");
        Ensure(objects[3].GetEntityId() == (EntityId)ids[3] && objects[3].GetEntityId().IsValid() && !EntityId.None.IsValid(), "the entity ID wraps the instance ID");
        Ensure((int)objects[3].GetEntityId() == ids[3], "entity ID converts to int");
        var other = new Texture2D(1, 1);
        Ensure(other.GetInstanceID() != ids[3] && !other.Equals(objects[3]) && other.Equals(other), "Equals follows identity");
    }

    private static void ToStringFormat()
    {
        var go = DisunityHost.CreateGameObject();
        go.name = "Hero";
        Ensure(go.ToString() == "Hero (UnityEngine.GameObject)", "GameObject.ToString is name and type");
        var texture = new Texture2D(2, 2) { name = "Ground" };
        Ensure(texture.ToString() == "Ground (UnityEngine.Texture2D)", "Texture2D.ToString is name and type");
        Ensure(new Material(DisunityHost.RegisterShader("Test/ToString")).ToString() == " (UnityEngine.Material)", "an unnamed object has an empty name");
        Ensure(new TextAsset("body").ToString() == "body", "TextAsset.ToString is its text");
        UnityEngine.Object.Destroy(texture);
        Ensure(texture.ToString() == "null", "a destroyed object prints as null");
    }

    private static void AssetDestruction()
    {
        var shader = DisunityHost.RegisterShader("Test/Destroy");
        Func<UnityEngine.Object>[] makers =
        [
            () => new Texture2D(2, 2), () => new Material(shader), () => new RenderTexture(8, 8, 0), () => new TextAsset("t"),
            () => new Material(shader), () => new Texture2D(4, 4),
        ];
        for (var i = 0; i < makers.Length; i++)
        {
            var value = makers[i]();
            Ensure(value != null && value, $"asset {i} is alive");
            if (i < 3) UnityEngine.Object.Destroy(value);
            else if (i == 3) UnityEngine.Object.DestroyImmediate(value, true);
            else UnityEngine.Object.DestroyImmediate(value);
            Ensure(value == null && !value && !(value != null), $"asset {i} compares equal to null once destroyed");
            UnityEngine.Object.Destroy(value);
            UnityEngine.Object.DestroyImmediate(value);
        }
        UnityEngine.Object.DestroyImmediate(null);
        UnityEngine.Object.Destroy(null);

        var rendering = new RenderTexture(8, 8, 0);
        RenderTexture.active = rendering;
        UnityEngine.Object.Destroy(rendering);
        Ensure(!rendering.IsCreated() && RenderTexture.active == null, "destroying a render texture releases it");

        var white = Texture2D.whiteTexture;
        UnityEngine.Object.Destroy(white);
        Ensure(white == null && Texture2D.whiteTexture && !ReferenceEquals(Texture2D.whiteTexture, white), "a destroyed built-in texture is made again");
        Ensure(Shader.Find("Test/Destroy") == shader, "a live shader stays registered");
        UnityEngine.Object.Destroy(shader);
        Ensure(Shader.Find("Test/Destroy") == null && DisunityHost.RegisterShader("Test/Destroy") != shader, "a destroyed shader leaves the registry");

        var go = DisunityHost.CreateGameObject();
        NotSupported(() => UnityEngine.Object.DestroyImmediate(go), "DestroyImmediate of a GameObject", "DestroyImmediate");
        NotSupported(() => UnityEngine.Object.DestroyImmediate(go.transform, true), "DestroyImmediate of a component", "DestroyImmediate");
        NotSupported(() => UnityEngine.Object.DestroyImmediate(ScriptableObject.CreateInstance<Asset>()), "DestroyImmediate of a script asset", "DestroyImmediate");
        Ensure(go && go.activeSelf, "a refused DestroyImmediate leaves the object alive");
        NotSupported(() => UnityEngine.Object.Destroy(new Texture2D(1, 1), 1f), "delayed destruction of an asset", "delayed destruction");
    }

    private static void AsyncCloning()
    {
        var texture = new Texture2D(2, 2) { name = "Src" };
        var viaParameters = UnityEngine.Object.Instantiate(texture, new InstantiateParameters());
        Ensure(viaParameters != texture && viaParameters.name == "Src(Clone)", "Instantiate with parameters clones an asset");
        var operation = UnityEngine.Object.InstantiateAsync(texture, 3);
        operation.WaitForCompletion();
        var clones = operation.Result!;
        Ensure(operation.isDone && clones.Length == 3 && clones.All(clone => clone != texture), "InstantiateAsync makes the requested clones");
        var none = UnityEngine.Object.InstantiateAsync(texture, 2, default(InstantiateParameters), new System.Threading.CancellationToken(true));
        none.WaitForCompletion();
        Ensure(none.Result!.Length == 0, "a cancelled InstantiateAsync makes no clones");
        Throws<ArgumentOutOfRangeException>(() => UnityEngine.Object.InstantiateAsync(texture, -1), "a negative count");
        Throws<NotSupportedException>(() => UnityEngine.Object.Instantiate(texture, default(UnityEngine.SceneManagement.Scene)), "instantiate into a scene", "Scene");
    }

    private static void AsyncCloningTiming()
    {
        var original = DisunityHost.CreateGameObject();
        original.AddComponent<Spawned>();
        var before = UnityEngine.Object.FindObjectsByType<Spawned>(FindObjectsSortMode.None).Length;

        var fireAndForget = UnityEngine.Object.InstantiateAsync(original, 3);
        Ensure(!fireAndForget.isDone && fireAndForget.Result is null, "Result is null before the operation is done");
        Ensure(UnityEngine.Object.FindObjectsByType<Spawned>(FindObjectsSortMode.None).Length == before, "no clone exists before the first frame");
        var seen = -1;
        fireAndForget.completed += _ => seen = UnityEngine.Object.FindObjectsByType<Spawned>(FindObjectsSortMode.None).Length;
        DisunityHost.Step(0.02f);
        Ensure(fireAndForget.isDone, "the operation completes in the next frame");
        Ensure(seen == before + 3, $"a completed handler sees the clones (saw {seen - before} new)");
        Ensure(UnityEngine.Object.FindObjectsByType<Spawned>(FindObjectsSortMode.None).Length == before + 3 && fireAndForget.Result!.Length == 3,
            "the clones appear without Result being read first");

        var waited = UnityEngine.Object.InstantiateAsync(original, 2);
        waited.WaitForCompletion();
        Ensure(waited.isDone && waited.Result!.Length == 2 && UnityEngine.Object.FindObjectsByType<Spawned>(FindObjectsSortMode.None).Length == before + 5,
            "WaitForCompletion makes the clones at once");
        DisunityHost.Step(0.02f);
        Ensure(UnityEngine.Object.FindObjectsByType<Spawned>(FindObjectsSortMode.None).Length == before + 5, "the clones are made once");
    }

    private static void AsyncCloningFailure()
    {
        using var log = new LogRecorder();
        var failing = UnityEngine.Object.InstantiateAsync(ScriptableObject.CreateInstance<Asset>(), 1);
        var completed = 0;
        failing.completed += _ => completed++;
        DisunityHost.Step(0.02f);
        Ensure(failing.isDone && completed == 1 && failing.Result is null, "a failed creation still completes the operation, with no result");
        Ensure(log.Messages.Count == 1 && log.Messages[0].StartsWith("Exception:") && log.Messages[0].Contains("Instantiate"), "the failure is logged");
    }

    private static void InstantiateParameterPlacement()
    {
        var parent = DisunityHost.CreateGameObject();
        parent.transform.position = new Vector3(10, 0, 0);
        var original = DisunityHost.CreateGameObject();
        var local = UnityEngine.Object.Instantiate(original, new Vector3(1, 2, 0), Quaternion.identity, new InstantiateParameters { parent = parent.transform });
        Ensure(local.transform.parent == parent.transform && local.transform.localPosition == new Vector3(1, 2, 0) && local.transform.position == new Vector3(11, 2, 0),
            "a position under a parent is local without worldSpace");
        var world = UnityEngine.Object.Instantiate(original, new Vector3(1, 2, 0), Quaternion.identity, new InstantiateParameters { parent = parent.transform, worldSpace = true });
        Ensure(world.transform.parent == parent.transform && world.transform.position == new Vector3(1, 2, 0), "a position under a parent is world with worldSpace");
        var unparented = UnityEngine.Object.Instantiate(original, new Vector3(3, 0, 0), Quaternion.identity, default(InstantiateParameters));
        Ensure(unparented.transform.parent == null && unparented.transform.position == new Vector3(3, 0, 0), "a position without a parent is world space");

        var scene = (object)default(UnityEngine.SceneManagement.Scene);
        typeof(UnityEngine.SceneManagement.Scene).GetProperty("Handle", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)!.SetValue(scene, 5);
        var chosen = new InstantiateParameters { scene = (UnityEngine.SceneManagement.Scene)scene };
        NotSupported(() => UnityEngine.Object.Instantiate(original, chosen), "a chosen scene", "InstantiateParameters.scene");
        NotSupported(() => UnityEngine.Object.Instantiate(original, Vector3.zero, Quaternion.identity, chosen), "a chosen scene with a position", "InstantiateParameters.scene");
        NotSupported(() => UnityEngine.Object.InstantiateAsync(original, 1, chosen, default), "a chosen scene for InstantiateAsync", "InstantiateParameters.scene");
        NotSupported(() => UnityEngine.Object.Instantiate(original, new InstantiateParameters { parent = parent.transform, worldSpace = true }), "world space under a parent", "worldPositionStays");
    }

    private static void AssetCloning()
    {
        var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false) { name = "Ground", filterMode = FilterMode.Point };
        texture.SetPixel(1, 1, Color.red);
        texture.Apply();
        var copy = UnityEngine.Object.Instantiate(texture);
        Ensure(copy != texture && copy.name == "Ground(Clone)" && copy.filterMode == FilterMode.Point, "an asset clone is a new named object");
        Ensure(copy.GetPixel(1, 1) == Color.red, "a texture clone holds the pixels");
        copy.SetPixel(1, 1, Color.blue);
        Ensure(texture.GetPixel(1, 1) == Color.red, "a texture clone does not share pixels");

        var material = new Material(DisunityHost.RegisterShader("Test/Clone")) { name = "Mat" };
        material.SetFloat("_Cutoff", 0.5f);
        var materialCopy = (Material)UnityEngine.Object.Instantiate((UnityEngine.Object)material);
        Ensure(materialCopy.name == "Mat(Clone)" && materialCopy.GetFloat("_Cutoff") == 0.5f && materialCopy.shader == material.shader, "a material clone keeps shader and values");
        var textCopy = UnityEngine.Object.Instantiate(new TextAsset("hello") { name = "t" });
        Ensure(textCopy.text == "hello" && textCopy.name == "t(Clone)", "a text asset clone keeps its text");
        NotSupported(() => UnityEngine.Object.Instantiate(ScriptableObject.CreateInstance<Asset>()), "cloning a script asset", "Instantiate");
    }

    private static void DontDestroy()
    {
        var root = DisunityHost.CreateGameObject();
        var child = DisunityHost.CreateGameObject();
        child.transform.parent = root.transform;
        using (var log = new LogRecorder())
        {
            UnityEngine.Object.DontDestroyOnLoad(root);
            UnityEngine.Object.DontDestroyOnLoad(root.transform);
            UnityEngine.Object.DontDestroyOnLoad(new Texture2D(1, 1));
            Ensure(log.Messages.Count == 0, "root objects and assets do not warn");
            UnityEngine.Object.DontDestroyOnLoad(child);
            Ensure(log.Messages.Count == 1 && log.Messages[0].StartsWith("Warning:") && log.Messages[0].Contains("DontDestroyOnLoad"), "a child object warns");
        }
        Throws<ArgumentNullException>(() => UnityEngine.Object.DontDestroyOnLoad(null!), "DontDestroyOnLoad(null)");
        Ensure(root && child && root.activeSelf, "DontDestroyOnLoad leaves objects alone in a single scene");
    }

    private static void Searches()
    {
        var found = Enumerable.Range(0, 3).Select(_ => DisunityHost.AddComponent(new Finder())).ToArray();
        var hidden = DisunityHost.AddComponent(new Finder());
        hidden.gameObject.SetActive(false);
        Ensure(UnityEngine.Object.FindObjectsByType<Finder>(FindObjectsSortMode.None).Length == 3, "FindObjectsByType skips inactive objects");
        Ensure(UnityEngine.Object.FindObjectsByType<Finder>(FindObjectsInactive.Exclude, FindObjectsSortMode.None).Length == 3, "Exclude finds the active ones");
        Ensure(UnityEngine.Object.FindObjectsOfType<Finder>().Length == 3 && UnityEngine.Object.FindObjectsOfType<Finder>(false).Length == 3, "FindObjectsOfType");
        Ensure(UnityEngine.Object.FindObjectOfType<Finder>() && UnityEngine.Object.FindFirstObjectByType<Finder>() && UnityEngine.Object.FindAnyObjectByType<Finder>(), "single finds");
        Ensure(UnityEngine.Object.FindFirstObjectByType<Finder>(FindObjectsInactive.Exclude) && UnityEngine.Object.FindAnyObjectByType<Finder>(FindObjectsInactive.Exclude), "single finds with Exclude");
        Ensure(UnityEngine.Object.FindObjectOfType<Finder>(false), "FindObjectOfType(false)");

        var untyped = UnityEngine.Object.FindObjectsByType(typeof(Finder), FindObjectsSortMode.None);
        Ensure(untyped.Length == 3 && untyped.All(value => value is Finder), "the Type overload finds the same objects");
        Ensure(UnityEngine.Object.FindObjectsOfType(typeof(Finder)).Length == 3 && UnityEngine.Object.FindObjectOfType(typeof(Finder)) is Finder, "FindObjectsOfType(Type)");
        Ensure(UnityEngine.Object.FindFirstObjectByType(typeof(Finder)) is Finder && UnityEngine.Object.FindAnyObjectByType(typeof(Finder)) is Finder, "single Type finds");
        Ensure(UnityEngine.Object.FindObjectsByType(typeof(Finder), FindObjectsInactive.Exclude, FindObjectsSortMode.None).Length == 3, "Type overload with Exclude");
        Ensure(UnityEngine.Object.FindObjectsByType<Transform>(FindObjectsSortMode.None).Length >= 3, "components of any type are found");

        var sorted = UnityEngine.Object.FindObjectsByType<Finder>(FindObjectsSortMode.InstanceID).Select(value => value.GetInstanceID()).ToArray();
        Ensure(sorted.Length == 3 && sorted.SequenceEqual(sorted.OrderBy(id => id)), "InstanceID sorting is ascending");
        Ensure(found.All(value => UnityEngine.Object.FindObjectsByType<Finder>(FindObjectsSortMode.None).Contains(value)), "every active object is found");

        NotSupported(() => UnityEngine.Object.FindObjectsByType<Finder>(FindObjectsInactive.Include, FindObjectsSortMode.None), "FindObjectsByType(Include)", "FindObjectsByType");
        NotSupported(() => UnityEngine.Object.FindObjectsOfType<Finder>(true), "FindObjectsOfType(true)", "FindObjectsOfType");
        NotSupported(() => UnityEngine.Object.FindObjectOfType<Finder>(true), "FindObjectOfType(true)", "FindObjectOfType");
        NotSupported(() => UnityEngine.Object.FindFirstObjectByType<Finder>(FindObjectsInactive.Include), "FindFirstObjectByType(Include)", "FindFirstObjectByType");
        NotSupported(() => UnityEngine.Object.FindAnyObjectByType<Finder>(FindObjectsInactive.Include), "FindAnyObjectByType(Include)", "FindAnyObjectByType");
        NotSupported(() => UnityEngine.Object.FindObjectsByType(typeof(Finder), FindObjectsInactive.Include, FindObjectsSortMode.None), "Type overload with Include", "FindObjectsByType");
        NotSupported(() => UnityEngine.Object.FindObjectsOfType(typeof(Finder), true), "FindObjectsOfType(Type, true)", "FindObjectsOfType");
        NotSupported(() => UnityEngine.Object.FindObjectOfType(typeof(Finder), true), "FindObjectOfType(Type, true)", "FindObjectOfType");
        NotSupported(() => UnityEngine.Object.FindFirstObjectByType(typeof(Finder), FindObjectsInactive.Include), "FindFirstObjectByType(Type, Include)", "FindFirstObjectByType");
        NotSupported(() => UnityEngine.Object.FindAnyObjectByType(typeof(Finder), FindObjectsInactive.Include), "FindAnyObjectByType(Type, Include)", "FindAnyObjectByType");
        NotSupported(() => UnityEngine.Object.FindObjectsByType<Material>(FindObjectsSortMode.None), "finding Materials", "FindObjectsByType");
        NotSupported(() => UnityEngine.Object.FindObjectsOfType<Texture2D>(), "finding Texture2Ds", "FindObjectsOfType");
        NotSupported(() => UnityEngine.Object.FindFirstObjectByType<Asset>(), "finding script assets", "FindFirstObjectByType");
        NotSupported(() => UnityEngine.Object.FindAnyObjectByType<Shader>(), "finding Shaders", "FindAnyObjectByType");
        NotSupported(() => UnityEngine.Object.FindObjectOfType<ScriptableObject>(), "finding ScriptableObjects", "FindObjectOfType");
        NotSupported(() => UnityEngine.Object.FindObjectsByType(typeof(Material), FindObjectsSortMode.None), "the Type overload for Materials", "FindObjectsByType");
        NotSupported(() => UnityEngine.Object.FindObjectOfType(typeof(Texture2D)), "the Type overload for one Texture2D", "FindObjectOfType");
        Ensure(UnityEngine.Object.FindObjectsByType<UnityEngine.Object>(FindObjectsSortMode.None).Length >= 3 && UnityEngine.Object.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length >= 3,
            "Object and GameObject find scene objects");
        Throws<ArgumentNullException>(() => UnityEngine.Object.FindObjectsByType(null!, FindObjectsSortMode.None), "a null type");
        foreach (var name in new[] { "FindObjectsOfTypeAll", "FindObjectsOfTypeIncludingAssets" })
        {
            var method = typeof(UnityEngine.Object).GetMethod(name, [typeof(Type)])!;
            var failure = Assert(() => method.Invoke(null, [typeof(Finder)]));
            Ensure(failure.InnerException is NotSupportedException && failure.InnerException.Message.Contains(name), $"{name} says what it cannot do");
        }
        var legacy = typeof(UnityEngine.Object).GetMethod("FindSceneObjectsOfType", [typeof(Type)])!;
        Ensure(((UnityEngine.Object[])legacy.Invoke(null, [typeof(Finder)])!).Length == 3, "FindSceneObjectsOfType searches the scene");
    }

    private static TargetInvocationException Assert(Action action)
    {
        try { action(); }
        catch (TargetInvocationException error) { return error; }
        throw new InvalidOperationException("expected the call to throw");
    }
}
