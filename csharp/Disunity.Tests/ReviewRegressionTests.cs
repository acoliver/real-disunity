using Disunity.Hosting;
using UnityEngine;

internal static class ReviewRegressionTests
{
    private sealed class Pose : MonoBehaviour
    {
        public static Vector3 Seen;
        private void Awake() => Seen = transform.position;
    }
    public static void Run()
    {
        WorldPosition();
        DescendantLifetime();
        CloneConstruction();
        CloneNestedFailures();
        ReviewRetentionTests.Run();
        AssetFailure();
        NestedFailure();
        CancellationFailure();
        LongFailure();
        AssemblyReferences();
        AssetCleanupFailure();
    }
    private static void WorldPosition()
    {
        DisunityHost.LoadWorld();
        var parent = DisunityHost.CreateGameObject();
        parent.transform.position = new(10, 3, 0);
        parent.transform.rotation = new Quaternion(0, 0, MathF.Sqrt(0.5f), MathF.Sqrt(0.5f));
        parent.transform.localScale = new(2, 3, 1);
        var child = DisunityHost.CreateGameObject();
        child.transform.SetParent(parent.transform, false);
        child.transform.localPosition = new(2, 0, 0);
        Require(Distance(child.transform.position, new Vector3(10, 7, 0)) < 0.0001f, "B1 world getter under translated rotated scaled parent");
        child.transform.position = new(4, 5, 0);
        Require(Distance(child.transform.position, new Vector3(4, 5, 0)) < 0.0001f, "B1 world setter");
        child.AddComponent<Pose>();
        var clone = UnityEngine.Object.Instantiate(child, new Vector3(8, 9, 0), Quaternion.identity, parent.transform);
        Require(Distance(Pose.Seen, new Vector3(8, 9, 0)) < 0.0001f, "B1 parented clone Awake world pose");
        var body = clone.AddComponent<Rigidbody2D>();
        body.bodyType = RigidbodyType2D.Kinematic;
        var before = body.position;
        body.MovePosition(before);
        DisunityHost.Step(0.02f);
        Require((body.position - before).magnitude < 0.0001f, "B1 parented body position movement round trip");
        parent.transform.localScale = new(0, 1, 1);
        try { child.transform.position = Vector3.zero; throw new Exception("B1 singular parent accepted"); }
        catch (DisunityException error) when (error.Reason == DisunityErrorReason.SingularParent) { }
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: B1 world position, parented clone and body round trip");
    }
    private static void DescendantLifetime()
    {
        DisunityHost.LoadWorld();
        var parent = DisunityHost.CreateGameObject();
        var child = DisunityHost.CreateGameObject();
        var inactive = DisunityHost.CreateGameObject();
        child.transform.SetParent(parent.transform, false);
        inactive.transform.SetParent(child.transform, false);
        inactive.SetActive(false);
        UnityEngine.Object.Destroy(parent);
        Require(child && inactive && child != null && inactive.transform != null, "B2 pending subtree wrappers stay visible");
        Require(UnityEngine.Object.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length == 2, "B2 active pending subtree stays searchable");
        DisunityHost.Step(0);
        Require(!child && !inactive && child == null && inactive.transform == null, "B2 subtree wrappers retire at frame barrier");
        try { _ = child!.transform.position; throw new Exception("B2 child read accepted"); }
        catch (ObjectDisposedException) { }
        Require(UnityEngine.Object.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length == 0, "B2 lookup after subtree invalidation");
        DisunityHost.UnloadWorld();
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: B2 scriptless inactive descendant lifetime and unload");
    }
    private sealed class Nested : MonoBehaviour { }
    private sealed class Reentrant : MonoBehaviour
    {
        public static Action? Construct;
        public static int Awakes;
        public static int Disables;
        public static int Destroys;
        private void OnDisable() => Disables++;
        private void OnDestroy() => Destroys++;
        public Reentrant() => Construct?.Invoke();
        public int Value = 42;
        public FieldData Field = new();
        private void Awake() { Require(Value == 42, "B3 fields bound before Awake"); Awakes++; }
    }
    private sealed class FieldData
    {
        public static Action? Construct;
        public FieldData() => Construct?.Invoke();
        public int Value = 7;
    }
    private static void CloneConstruction()
    {
        DisunityHost.LoadWorld();
        var template = DisunityHost.AddComponent(new Reentrant());
        template.gameObject.AddComponent<Reentrant>();
        var before = Reentrant.Awakes;
        Action reenter = () => {
            Require(UnityEngine.Object.FindObjectsByType<Reentrant>(FindObjectsSortMode.None).Length == 2, "B3 unpublished clone hidden from lookup");
            DisunityHost.AddComponent(new Nested());
            Require(Reentrant.Awakes == before, "B3 no clone callback during binding");
        };
        Reentrant.Construct = reenter;
        FieldData.Construct = reenter;
        var clone = UnityEngine.Object.Instantiate(template);
        Reentrant.Construct = null;
        FieldData.Construct = null;
        Require(Reentrant.Awakes == before + 2 && clone.Field.Value == 7, "B3 activation follows complete binding");
        var count = UnityEngine.Object.FindObjectsByType<Reentrant>(FindObjectsSortMode.None).Length;
        Reentrant.Construct = () => { DisunityHost.AddComponent(new Nested()); throw new InvalidOperationException("B3 constructor fault"); };
        try { UnityEngine.Object.Instantiate(template); throw new Exception("B3 construction failure lost"); }
        catch (System.Reflection.TargetInvocationException error) when (error.InnerException?.Message == "B3 constructor fault") { }
        finally { Reentrant.Construct = null; }
        Require(UnityEngine.Object.FindObjectsByType<Reentrant>(FindObjectsSortMode.None).Length == count, "B3 rollback removes unpublished clone");
        DisunityHost.Step(0);
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: B3 constructor and field reentry, publication and rollback");
    }
    private static void CloneNestedFailures()
    {
        var failures = new List<Exception>();
        foreach (var constructorKind in new[] { 0, 1, 2 })
        {
            try { CloneNestedFailure(constructorKind); }
            catch (Exception error) { failures.Add(error); }
            finally
            {
                Reentrant.Construct = null;
                FieldData.Construct = null;
                DisunityHost.UnloadWorld();
            }
        }
        if (failures.Count != 0) throw new AggregateException(failures);
        Console.WriteLine("PASS: B3 parented constructor and field faults preserve first failure and retire reservations");
    }
    private static void CloneNestedFailure(int constructorKind)
    {
        DisunityHost.LoadWorld();
        var template = DisunityHost.AddComponent(new Reentrant());
        template.gameObject.AddComponent<Reentrant>();
        template.gameObject.AddComponent<Rigidbody2D>();
        var parent = DisunityHost.CreateGameObject();
        var tracked = (List<GameObject>)typeof(DisunityHost).GetField("Tracked", System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.NonPublic)!.GetValue(null)!;
        GameObject? reserved = null;
        var awakes = Reentrant.Awakes;
        var disables = Reentrant.Disables;
        var destroys = Reentrant.Destroys;
        var nestedDestroyed = 0;
        Action fail = () => {
            reserved = tracked.Single(value => !ReferenceEquals(value, parent) && !ReferenceEquals(value, template.gameObject));
            DisunityHost.AddComponent(new FailureProbe {
                Enter = () => throw EngineFault.Make("B3 original nested Awake failure"),
                Cleanup = () => nestedDestroyed++
            });
        };
        var fieldConstructions = 0;
        if (constructorKind == 0) Reentrant.Construct = fail;
        else FieldData.Construct = () => {
            // The serialized copy follows the two script wrapper field initializers.
            if (constructorKind == 1 || ++fieldConstructions > 2) fail();
        };
        try { UnityEngine.Object.Instantiate(template, parent.transform); throw new Exception("B3 nested failure lost"); }
        catch (Exception error) when (error.ToString().Contains("B3 original nested Awake failure"))
        {
            Require(!error.ToString().Contains("script has no managed binding"), "B3 never dispatch unbound clone cleanup");
        }
        Require(reserved is not null && !tracked.Contains(reserved), "B3 rollback removes reserved wrapper");
        foreach (var registry in new[] { "Scripts", "NativeComponents" })
        {
            var roots = (System.Collections.IDictionary)typeof(DisunityHost).GetField(registry, System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.NonPublic)!.GetValue(null)!;
            Require(!roots.Values.Cast<Component>().Any(value => ReferenceEquals(value.gameObject, reserved)), "B3 rollback releases all reserved script and native component wrappers");
        }
        Require(Reentrant.Awakes == awakes && Reentrant.Disables == disables + 2 && Reentrant.Destroys == destroys + 2 && nestedDestroyed == 1, "B3 only entered scripts receive cleanup and teardown completes");
        Require(UnityEngine.Object.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length == 0, "B3 failure removes published world objects");
        try { DisunityHost.Step(0); throw new Exception("B3 faulted world advanced"); }
        catch (DisunityException error) when (error.Reason == DisunityErrorReason.Faulted) { }
    }
    private sealed class FaultAsset : ScriptableObject
    {
        public static readonly List<string> Calls = new();
        public static bool Fail;
        private void Awake() { Calls.Add("awake"); if (Fail) throw EngineFault.Make("B4 asset fault"); }
        private void OnEnable() => Calls.Add("enable");
        private void OnDisable() => Calls.Add("disable");
        private void OnDestroy() => Calls.Add("destroy");
    }
    private static void AssetFailure()
    {
        DisunityHost.LoadWorld();
        FaultAsset.Fail = false;
        ScriptableObject.CreateInstance<FaultAsset>();
        FaultAsset.Calls.Clear();
        FaultAsset.Fail = true;
        try { ScriptableObject.CreateInstance<FaultAsset>(); throw new Exception("B4 failure lost"); }
        catch (Exception error) when (error.ToString().Contains("B4 asset fault")) { }
        finally { FaultAsset.Fail = false; }
        Require(!FaultAsset.Calls.Contains("enable"), "B4 failed Awake cannot run OnEnable");
        Require(FaultAsset.Calls.Count(value => value == "destroy") == 2 && FaultAsset.Calls.Count(value => value == "disable") == 1, "B4 live assets join fault teardown once");
        DisunityHost.UnloadWorld();
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: B4 asset failure and bounded shared teardown");
    }
    private sealed class FailureProbe : MonoBehaviour
    {
        public Action? Enter;
        public Action? Tick;
        public Action? Cleanup;
        private void Awake() => Enter?.Invoke();
        private void Update() => Tick?.Invoke();
        private void OnDestroy() => Cleanup?.Invoke();
    }
    private static void NestedFailure()
    {
        foreach (var depth in new[] { 1, 2 })
        foreach (var catchChild in new[] { false, true })
        {
            DisunityHost.LoadWorld();
            var cleaned = 0;
            Action enter = () => throw EngineFault.Make("B5 original nested failure");
            if (depth == 2) enter = () => DisunityHost.AddComponent(new FailureProbe { Enter = () => throw EngineFault.Make("B5 original nested failure"), Cleanup = () => cleaned++ });
            DisunityHost.AddComponent(new FailureProbe { Tick = () => {
                try { DisunityHost.AddComponent(new FailureProbe { Enter = enter, Cleanup = () => cleaned++ }); }
                catch when (catchChild) { }
            }, Cleanup = () => cleaned++ });
            try { DisunityHost.Step(0); throw new Exception("B5 failure lost"); }
            catch (DisunityException error) when (error.Message.Contains("B5 original nested failure") && !error.Message.Contains("wrong callback")) { }
            Require(cleaned == depth + 1, "B5 bounded teardown across all suspended scopes");
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: B5 nested first-failure preservation including caught child faults");
    }
    private sealed class ThrowingIterator : System.Collections.IEnumerator, IDisposable
    {
        public static int Disposed;
        public object? Current => null;
        public bool MoveNext() => true;
        public void Reset() => throw new NotSupportedException();
        public void Dispose() { Disposed++; throw new Exception("B6 disposer failure"); }
    }
    private static void CancellationFailure()
    {
        foreach (var failUpdate in new[] { true, false })
        {
            DisunityHost.LoadWorld();
            ThrowingIterator.Disposed = 0;
            var cleaned = 0;
            var probe = DisunityHost.AddComponent(new FailureProbe { Cleanup = () => cleaned++ });
            probe.StartCoroutine(new ThrowingIterator());
            probe.StartCoroutine(new ThrowingIterator());
            probe.Tick = () => {
                if (failUpdate) throw EngineFault.Make("B6 original Update failure");
                probe.gameObject.SetActive(false);
            };
            if (failUpdate)
            {
                try { DisunityHost.Step(0); throw new Exception("B6 failure lost"); }
                catch (DisunityException error) when (error.Message.Contains("B6 original Update failure")) { }
            }
            else DisunityHost.Step(0);
            Require(DisunityHost.ScriptExceptionCount == 2, "B6 each disposer failure is logged, not thrown");
            Require(cleaned == (failUpdate ? 1 : 0) && ThrowingIterator.Disposed == 2, "B6 disposal cannot abandon native teardown");
            DisunityHost.UnloadWorld();
            Require(cleaned == 1, "B6 unload destroys the script once");
        }
        Console.WriteLine("PASS: B6 cancellation disposal preserves first failure and completes cleanup");
    }
    private static void LongFailure()
    {
        foreach (var length in new[] { 65500, 65536, 70000 })
        {
            DisunityHost.LoadWorld();
            var cleaned = false;
            DisunityHost.AddComponent(new FailureProbe { Tick = () => throw EngineFault.Make("B7 large λ" + new string('界', length)), Cleanup = () => cleaned = true });
            try { DisunityHost.Step(0); throw new Exception("B7 failure lost"); }
            catch (DisunityException error) when (error.Message.Contains("B7 large λ")) { }
            Require(cleaned, "B7 long UTF-8 failure reaches native cleanup");
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: B7 oversized multibyte aggregate failures complete teardown");
    }
    private static void AssemblyReferences()
    {
        var path = Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "../../../../UnityConsumer/bin/Debug/net8.0/UnityConsumer.dll"));
        var consumer = System.Reflection.Assembly.LoadFile(path);
        var custom = consumer.GetReferencedAssemblies().Where(a => !a.Name!.StartsWith("System.") && a.Name != "netstandard").ToArray();
        Require(custom.Length == 1 && custom[0].Name == "Disunity.Engine", "B9 built consumer references one Unity API assembly");
        var engine = typeof(DisunityHost).Assembly;
        Require(engine.GetReferencedAssemblies().All(a => a.Name!.StartsWith("System.") || a.Name == "netstandard"), "B9 built engine has no request transport assembly");
        Require(engine.GetTypes().Count(t => t.FullName == "UnityEngine.GameObject") == 1, "B9 one built GameObject definition");
        Console.WriteLine("PASS: B9 built consumer and engine assembly references");
    }
    private sealed class CleanupAsset : ScriptableObject
    {
        public static int DestroyedCount;
        public static bool FailEnable;
        private void OnEnable() { if (FailEnable) throw EngineFault.Make("B4 enable failure"); }
        private void OnDisable() => throw EngineFault.Make("B4 cleanup failure");
        private void OnDestroy() => DestroyedCount++;
    }
    private static void AssetCleanupFailure()
    {
        foreach (var failEnable in new[] { false, true })
        {
            DisunityHost.LoadWorld();
            CleanupAsset.DestroyedCount = 0;
            CleanupAsset.FailEnable = failEnable;
            try
            {
                var asset = ScriptableObject.CreateInstance<CleanupAsset>();
                UnityEngine.Object.Destroy(asset);
                DisunityHost.Step(0);
                throw new Exception("B4 cleanup failure lost");
            }
            catch (DisunityException error) when (error.Reason == DisunityErrorReason.CallbackFailed && error.Message.Contains(failEnable ? "B4 enable failure" : "B4 cleanup failure")) { }
            Require(CleanupAsset.DestroyedCount == 1, "B4 cleanup failure retains OnDestroy exactly once");
            DisunityHost.UnloadWorld();
            Require(CleanupAsset.DestroyedCount == 1, "B4 repeated unload does not repeat cleanup");
        }
        Console.WriteLine("PASS: B4 enable and cleanup faults preserve required teardown");
    }
    private static float Distance(Vector3 a, Vector3 b) => MathF.Abs(a.x - b.x) + MathF.Abs(a.y - b.y) + MathF.Abs(a.z - b.z);
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
