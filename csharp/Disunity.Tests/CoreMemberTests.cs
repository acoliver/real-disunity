using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;
using Object = UnityEngine.Object;
using Random = UnityEngine.Random;

/// <summary>Time, Input, Random, LayerMask, Resources, MonoBehaviour.useGUILayout, WaitForSecondsRealtime and Object.MarkDirty.</summary>
internal static class CoreMemberTests
{
    private sealed class Counter : MonoBehaviour
    {
        public int Fixed;
        private void FixedUpdate() => Fixed++;
    }

    private sealed class ButtonReader : MonoBehaviour
    {
        public readonly List<(bool Down, bool Held, bool Up)> Fire = new();
        private void Update() => Fire.Add((Input.GetButtonDown("Fire"), Input.GetButton("Fire"), Input.GetButtonUp("Fire")));
    }

    public static void Run()
    {
        TimeMembers();
        InputMembers();
        RandomMembers();
        LayerMasks();
        ResourceMembers();
        SmallMembers();
        Console.WriteLine("PASS: Time scale, clamping and smoothing, Input buttons, mouse and touch, Random state and shapes, LayerMask, Resources lookups and the small stored members");
    }

    private static void Near(float expected, float actual, string what) => Ensure(MathF.Abs(expected - actual) < 1e-4f, $"{what}: expected {expected}, got {actual}");

    private static void TimeMembers()
    {
        DisunityHost.LoadWorld();
        try
        {
            var counter = DisunityHost.AddComponent(new Counter());
            Near(1f, Time.timeScale, "default timeScale");
            Near(1f / 3f, Time.maximumDeltaTime, "default maximumDeltaTime");
            DisunityHost.Step(0.1f);
            Near(0.1f, Time.deltaTime, "deltaTime");
            Near(0.1f, Time.unscaledDeltaTime, "unscaledDeltaTime");
            Near(0.1f, Time.unscaledTime, "unscaledTime");
            Near(0.1f, Time.smoothDeltaTime, "the first frame sets smoothDeltaTime");
            Ensure(Time.frameCount == 1 && Time.renderedFrameCount == 1, "renderedFrameCount follows frameCount");
            Ensure(counter.Fixed == 5, "five fixed steps in 0.1 s");

            Time.timeScale = 0.5f;
            Near(0.5f, Time.timeScale, "timeScale reads back");
            DisunityHost.Step(0.1f);
            Near(0.05f, Time.deltaTime, "timeScale halves deltaTime");
            Near(0.1f, Time.unscaledDeltaTime, "and not unscaledDeltaTime");
            Near(0.15f, Time.time, "time advances by scaled deltas");
            Near(0.2f, Time.unscaledTime, "unscaledTime advances by real deltas");
            Near(0.09f, Time.smoothDeltaTime, "smoothDeltaTime moves a fifth of the way to the new delta");
            Ensure(counter.Fixed == 7, "fixed steps run on scaled time: 0.05 s is two more steps");

            Time.timeScale = 0;
            DisunityHost.Step(0.1f);
            Ensure(Time.deltaTime == 0 && counter.Fixed == 7, "a zero scale stops time and the fixed steps");
            Near(0.3f, Time.unscaledTime, "while unscaled time runs on");
        }
        finally { DisunityHost.UnloadWorld(); }
        TimeLimits();
    }

    private static void TimeLimits()
    {
        DisunityHost.LoadWorld();
        try
        {
            Time.maximumDeltaTime = 0.2f;
            Near(0.2f, Time.maximumDeltaTime, "maximumDeltaTime reads back");
            DisunityHost.Step(5f);
            Near(0.2f, Time.deltaTime, "a long frame counts as maximumDeltaTime");
            Near(0.2f, Time.unscaledDeltaTime, "also for unscaledDeltaTime");
            Time.timeScale = 2;
            DisunityHost.Step(5f);
            Near(0.4f, Time.deltaTime, "the clamp applies before scaling");

            using (var log = new LogRecorder())
            {
                Time.timeScale = 500;
                Near(100f, Time.timeScale, "a scale above 100 is brought to 100");
                Time.timeScale = -3;
                Near(0f, Time.timeScale, "and a negative scale to 0");
                Ensure(log.Messages.Count == 2 && log.Messages.All(message => message.StartsWith("Warning:")), "each logs a warning");
            }
            Throws<ArgumentOutOfRangeException>(() => Time.timeScale = float.NaN, "a NaN scale");
            Time.maximumDeltaTime = 0;
            Near(0.0001f, Time.maximumDeltaTime, "maximumDeltaTime has a floor");
            Throws<ArgumentOutOfRangeException>(() => Time.maximumDeltaTime = float.PositiveInfinity, "an infinite maximumDeltaTime");

            var start = Time.realtimeSinceStartupAsDouble;
            var later = start;
            for (var spins = 0; later <= start && spins < 50_000_000; spins++) later = Time.realtimeSinceStartupAsDouble;
            Ensure(start > 0 && later > start, "realtimeSinceStartupAsDouble is positive and rises");
            // The clock moves between reads, so the float read is bracketed by two double reads.
            var before = Time.realtimeSinceStartupAsDouble;
            var single = Time.realtimeSinceStartup;
            var after = Time.realtimeSinceStartupAsDouble;
            Ensure((float)before <= single && single <= (float)after,
                $"realtimeSinceStartup is the same clock as a float: {before} <= {single} <= {after}");
        }
        finally { DisunityHost.UnloadWorld(); }
        DisunityHost.LoadWorld();
        try
        {
            Near(1f, Time.timeScale, "a new world starts at scale 1");
            Near(1f / 3f, Time.maximumDeltaTime, "and the default maximum");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void InputMembers()
    {
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.RegisterInputAxis(0, new("Fire", KeyCode.None, KeyCode.Space) { AltPositive = KeyCode.A, AltNegative = KeyCode.B });
            var counter = DisunityHost.AddComponent(new ButtonReader());
            Ensure(!Input.anyKey && !Input.anyKeyDown && !Input.GetButton("Fire"), "nothing is held at the start");
            DisunityHost.PushKey(0, KeyCode.A, true);
            DisunityHost.Step(0.01f);
            Ensure(counter.Fire.Last() == (true, true, false), "the alternative positive key presses the button");
            Ensure(Input.anyKey, "anyKey outside a callback");
            Ensure(Input.GetButton("Fire"), "GetButton outside a callback");
            DisunityHost.PushKey(0.011, KeyCode.A, false);
            DisunityHost.Step(0.01f);
            Ensure(counter.Fire.Last() == (false, false, true), "releasing it ends the button");
            DisunityHost.PushKey(0.021, KeyCode.B, true);
            DisunityHost.Step(0.01f);
            Ensure(counter.Fire.Last() == (true, true, false), "the alternative negative key counts too");
            Ensure(Input.GetButton("Fire") && Input.anyKey, "B holds the button and counts for anyKey");
            Throws<ArgumentException>(() => Input.GetButton("Nothing"), "an unregistered button");
            Throws<ArgumentException>(() => Input.GetButtonDown("Nothing"), "an unregistered button down");
            Throws<ArgumentException>(() => Input.GetButtonUp("Nothing"), "an unregistered button up");
            DisunityHost.PushFocus(0.031, false);
            DisunityHost.Step(0.01f);
            Ensure(!Input.anyKey && !Input.GetButton("Fire"), "losing focus releases every key");
        }
        finally { DisunityHost.UnloadWorld(); }

        Ensure(Input.mousePresent && Input.mousePosition == Vector3.zero && Input.mouseScrollDelta == Vector2.zero, "the mouse is present and idle");
        Ensure(!Input.GetMouseButton(0) && !Input.GetMouseButtonDown(1) && !Input.GetMouseButtonUp(2), "no mouse button is pressed");
        Throws<ArgumentOutOfRangeException>(() => Input.GetMouseButton(-1), "a negative mouse button");
        Ensure(!Input.touchSupported && Input.touchCount == 0 && Input.touches.Length == 0, "a desktop host has no touches");
        Throws<ArgumentOutOfRangeException>(() => Input.GetTouch(0), "GetTouch with no touches");
        Ensure(Input.compositionString == "", "no text is being composed");
        Ensure(Input.imeCompositionMode == IMECompositionMode.Auto, "IME mode defaults to Auto");
        Input.imeCompositionMode = IMECompositionMode.Off;
        Input.compositionCursorPos = new Vector2(3, 4);
        Ensure(Input.imeCompositionMode == IMECompositionMode.Off && Input.compositionCursorPos == new Vector2(3, 4), "IME settings are stored");
        Input.imeCompositionMode = IMECompositionMode.Auto;
    }

    private static void RandomMembers()
    {
        Random.InitState(42);
        var saved = Random.state;
        var first = Enumerable.Range(0, 5).Select(_ => Random.value).ToArray();
        Random.state = saved;
        Ensure(Enumerable.Range(0, 5).Select(_ => Random.value).SequenceEqual(first), "restoring the state repeats the sequence exactly");
        Random.InitState(42);
        Ensure(Enumerable.Range(0, 5).Select(_ => Random.value).SequenceEqual(first), "a seed repeats it too");
        Random.InitState(43);
        Ensure(!Enumerable.Range(0, 5).Select(_ => Random.value).SequenceEqual(first), "another seed differs");
        Throws<ArgumentException>(() => Random.state = default, "an all-zero state");

        Random.InitState(1);
        var circle = Enumerable.Range(0, 4000).Select(_ => Random.insideUnitCircle).ToArray();
        Ensure(circle.All(point => point.sqrMagnitude <= 1f), "insideUnitCircle stays inside");
        var circleInner = circle.Count(point => point.sqrMagnitude <= 0.25f) / 4000f;
        Ensure(MathF.Abs(circleInner - 0.25f) < 0.04f, $"a uniform disc has a quarter of its points within half the radius, got {circleInner}");
        var sphere = Enumerable.Range(0, 4000).Select(_ => Random.insideUnitSphere).ToArray();
        Ensure(sphere.All(point => point.sqrMagnitude <= 1f), "insideUnitSphere stays inside");
        var sphereInner = sphere.Count(point => point.sqrMagnitude <= 0.25f) / 4000f;
        Ensure(MathF.Abs(sphereInner - 0.125f) < 0.04f, $"a uniform ball has an eighth of its points within half the radius, got {sphereInner}");

        var fixedColor = Random.ColorHSV(0f, 0f, 1f, 1f, 1f, 1f);
        Near(1f, fixedColor.r, "hue 0, full saturation and value is red: r");
        Near(0f, fixedColor.g, "g");
        Near(0f, fixedColor.b, "b");
        Near(1f, fixedColor.a, "and opaque");
        for (var i = 0; i < 50; i++)
        {
            var color = Random.ColorHSV(0.5f, 0.5f, 0f, 0f, 0.25f, 0.75f);
            Ensure(color.r == color.g && color.g == color.b && color.r is >= 0.25f and <= 0.75f, "zero saturation is a grey in the value range");
        }
        Ensure(Random.ColorHSV(0f, 1f, 0f, 1f, 1f, 1f, 0.5f, 0.5f).a == 0.5f, "the alpha range");
        Random.InitState(5);
        var one = Random.ColorHSV();
        Random.InitState(5);
        Ensure(one == Random.ColorHSV(0f, 1f, 0f, 1f, 0f, 1f, 1f, 1f), "the short overloads default to the full ranges");
    }

    private static void LayerMasks()
    {
        Ensure(LayerMask.NameToLayer("Default") == 0 && LayerMask.NameToLayer("TransparentFX") == 1 && LayerMask.NameToLayer("Ignore Raycast") == 2, "built-in layers");
        Ensure(LayerMask.NameToLayer("Water") == 4 && LayerMask.NameToLayer("UI") == 5, "Water and UI");
        Ensure(LayerMask.NameToLayer("Nope") == -1 && LayerMask.NameToLayer("") == -1, "unknown names are -1");
        Ensure(LayerMask.GetMask("Default", "UI") == 0b100001 && LayerMask.GetMask() == 0 && LayerMask.GetMask("Nope", "Water") == 0b10000, "GetMask sets one bit per known layer");
        Ensure(LayerMask.LayerToName(5) == "UI" && LayerMask.LayerToName(3) == "" && LayerMask.LayerToName(40) == "", "LayerToName");
        DisunityHost.SetUserLayerName(9, "Enemies");
        try { Ensure(LayerMask.NameToLayer("Enemies") == 9 && LayerMask.GetMask("Enemies") == 1 << 9, "user layers come from the host"); }
        finally { DisunityHost.ClearUserLayerNames(); }
        Ensure(LayerMask.NameToLayer("Enemies") == -1, "and can be cleared");
        Throws<ArgumentOutOfRangeException>(() => DisunityHost.SetUserLayerName(3, "x"), "user layers start at 8");
    }

    private static void ResourceMembers()
    {
        DisunityHost.LoadWorld();
        try
        {
            var visible = new GameObject("Seen", typeof(Counter));
            var hidden = new GameObject("Unseen", typeof(Counter));
            hidden.SetActive(false);
            var texture = new Texture2D(2, 2);
            Ensure(Resources.InstanceIDToObject(visible.GetInstanceID()) == visible && Resources.EntityIdToObject(visible.GetEntityId()) == visible, "an ID finds its object");
            Ensure(Resources.EntityIdToObject(texture.GetEntityId()) == texture, "assets are found by ID too");
            Ensure(Resources.EntityIdIsValid(visible.GetEntityId()) && Resources.InstanceIDIsValid(texture.GetInstanceID()), "live objects are valid");
            Ensure(!Resources.EntityIdIsValid(EntityId.None) && Resources.EntityIdToObject(EntityId.None) is null && !Resources.InstanceIDIsValid(12345), "unknown IDs are not");
            var id = texture.GetEntityId();
            Object.DestroyImmediate(texture);
            Ensure(!Resources.EntityIdIsValid(id) && Resources.EntityIdToObject(id) is null, "a destroyed object is no longer valid");

            Ensure(Resources.FindObjectsOfTypeAll<Counter>().Length == 2, "FindObjectsOfTypeAll includes inactive objects");
            Ensure(Resources.FindObjectsOfTypeAll(typeof(GameObject)).Contains(hidden), "also for GameObjects");
            Ensure(Object.FindObjectsByType<Counter>(FindObjectsSortMode.None).Length == 1, "unlike the active-only search");
            Throws<NotSupportedException>(() => Resources.FindObjectsOfTypeAll<Texture2D>(), "FindObjectsOfTypeAll for an asset type", "Texture2D", "not supported yet");
            Throws<NotSupportedException>(() => Resources.FindObjectsOfTypeAll<Object>(), "FindObjectsOfTypeAll for Object", "Object", "not supported yet");
            Throws<NotSupportedException>(() => Resources.GetBuiltinResource<Material>("Sprites-Default.mat"), "GetBuiltinResource", "Sprites-Default.mat", "not supported yet");
            Throws<NotSupportedException>(() => Resources.GetBuiltinResource(typeof(Material), "Default-Material.mat"), "GetBuiltinResource(Type, path)", "Default-Material.mat", "not supported yet");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void SmallMembers()
    {
        DisunityHost.LoadWorld();
        try
        {
            var script = DisunityHost.AddComponent(new Counter());
            Ensure(script.useGUILayout, "useGUILayout defaults to true");
            script.useGUILayout = false;
            Ensure(!script.useGUILayout, "and is stored");
            script.MarkDirty();
            Ensure(script.name == script.gameObject.name, "MarkDirty changes nothing");
        }
        finally { DisunityHost.UnloadWorld(); }
        var wait = new WaitForSecondsRealtime(1.5f);
        Near(1.5f, wait.waitTime, "waitTime from the constructor");
        wait.waitTime = 4;
        Near(4f, wait.waitTime, "waitTime can be set");
        Throws<ArgumentOutOfRangeException>(() => wait.waitTime = -1, "a negative wait");
        Throws<ArgumentOutOfRangeException>(() => wait.waitTime = float.NaN, "a NaN wait");
        Near(4f, wait.waitTime, "a refused value changes nothing");
    }
}
