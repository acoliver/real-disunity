using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.LowLevel;
using UnityEngine.Playables;

/// <summary>The playable, timeline and player loop types that packages mention, so every assembly of a player loads its types.</summary>
internal static class PlayablesTests
{
    private static readonly System.Reflection.Assembly Engine = typeof(DisunityHost).Assembly;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try { RunInWorld(); }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static GameObject Named(string name)
    {
        var go = DisunityHost.CreateGameObject();
        go.name = name;
        return go;
    }

    private static void RunInWorld()
    {
        FacadesForward();
        Console.WriteLine("PASS: the module facades forward the playable and player loop type names a player's assemblies reference");
        PlayableAssetIsAScriptAsset();
        Console.WriteLine("PASS: a PlayableAsset is a script asset with the base duration and no outputs");
        PlayableBehaviourHooksDoNothing();
        Console.WriteLine("PASS: PlayableBehaviour hooks do nothing and Clone copies the behaviour");
        NullHandlesAreNullAndInvalid();
        Console.WriteLine("PASS: default playables, outputs and handles are null and not valid");
        GraphAndPlayablesRefuseWork();
        Console.WriteLine("PASS: creating a graph or a playable names the member as unsupported");
        DirectorRefusesPlayback();
        Console.WriteLine("PASS: PlayableDirector members name themselves as unsupported");
        PropertyNamesCompareByText();
        Console.WriteLine("PASS: PropertyName ids follow the text and the default name is null");
        ExposedReferencesResolveThroughTheTable();
        Console.WriteLine("PASS: an ExposedReference resolves through the table and falls back to its default");
        BindingsAndFramesHoldData();
        Console.WriteLine("PASS: PlayableBinding and FrameData hold their values");
        NotificationsReachReceivers();
        Console.WriteLine("PASS: a game notification and receiver implement the interfaces");
        PlayerLoopHoldsSystemsAndRefusesTheLoop();
        Console.WriteLine("PASS: PlayerLoopSystem holds data and the player loop names itself as unsupported");
    }

    private static void FacadesForward()
    {
        foreach (var name in new[]
        {
            "UnityEngine.Playables.IPlayable", "UnityEngine.Playables.IPlayableAsset", "UnityEngine.Playables.IPlayableBehaviour",
            "UnityEngine.Playables.IPlayableOutput", "UnityEngine.Playables.INotification", "UnityEngine.Playables.INotificationReceiver",
            "UnityEngine.Playables.PlayableAsset", "UnityEngine.Playables.PlayableBehaviour", "UnityEngine.Playables.PlayableBinding",
            "UnityEngine.Playables.PlayableHandle", "UnityEngine.Playables.Playable", "UnityEngine.Playables.PlayableOutputHandle",
            "UnityEngine.Playables.PlayableOutput", "UnityEngine.Playables.PlayableGraph", "UnityEngine.Playables.FrameData",
            "UnityEngine.Playables.ScriptPlayable`1", "UnityEngine.Playables.ScriptPlayableOutput",
            "UnityEngine.Playables.ScriptPlayableBinding", "UnityEngine.Playables.PlayableDirector",
            "UnityEngine.Playables.DirectorUpdateMode", "UnityEngine.Playables.PlayableBinding+DataStreamType",
            "UnityEngine.Playables.FrameData+EvaluationType", "UnityEngine.PropertyName", "UnityEngine.IExposedPropertyTable",
            "UnityEngine.ExposedReference`1", "UnityEngine.Animations.AnimationPlayableOutput", "UnityEngine.Animations.AnimationMotionXToDeltaPlayable",
            "UnityEngine.LowLevel.PlayerLoopSystem", "UnityEngine.LowLevel.PlayerLoop",
        })
            Expect.That(ModuleFacades.Forwards(Engine, name), $"the module facades forward {name}");
    }

    private sealed class Clip : PlayableAsset
    {
        public static int Enabled;
        public double Length = 4;
        private void OnEnable() => Enabled++;
        public override double duration => Length;
        public override Playable CreatePlayable(PlayableGraph graph, GameObject owner) => Playable.Null;
    }

    private sealed class PlainClip : PlayableAsset
    {
        public override Playable CreatePlayable(PlayableGraph graph, GameObject owner) => Playable.Null;
    }

    private static void PlayableAssetIsAScriptAsset()
    {
        var plain = ScriptableObject.CreateInstance<PlainClip>();
        Expect.That(double.IsPositiveInfinity(plain.duration) && double.IsPositiveInfinity(PlayableBinding.DefaultDuration), "a clip lasts until stopped unless it says otherwise");
        Expect.That(!plain.outputs.Any() && !PlayableBinding.None.Any(), "and has no outputs");
        Expect.That(plain is IPlayableAsset, "a PlayableAsset is an IPlayableAsset");
        var clip = ScriptableObject.CreateInstance<Clip>();
        Expect.That(Clip.Enabled == 1, "creating a clip enables it as it does any script asset");
        Expect.Near(4f, (float)((IPlayableAsset)clip).duration, "an override of the duration is what the interface reads");
        Expect.That(clip.CreatePlayable(default, Named("owner")).Equals(Playable.Null), "CreatePlayable is the derived class's");
    }

    private sealed class Counting : PlayableBehaviour
    {
        public int Lives = 3;
        public int Pauses;
        public override void OnBehaviourPause(Playable playable, FrameData info) => Pauses++;
    }

    private static void PlayableBehaviourHooksDoNothing()
    {
        var behaviour = new Counting();
        IPlayableBehaviour hooks = behaviour;
        hooks.OnGraphStart(Playable.Null);
        hooks.OnGraphStop(Playable.Null);
        hooks.OnPlayableCreate(Playable.Null);
        hooks.OnPlayableDestroy(Playable.Null);
        hooks.OnBehaviourPlay(Playable.Null, default);
        hooks.PrepareFrame(Playable.Null, default);
        hooks.ProcessFrame(Playable.Null, default, null);
        hooks.OnBehaviourPause(Playable.Null, default);
        Expect.That(behaviour.Pauses == 1, "an override of a hook is what the interface calls");
        behaviour.Lives = 5;
        var copy = (Counting)((ICloneable)behaviour).Clone();
        Expect.That(!ReferenceEquals(copy, behaviour) && copy.Lives == 5 && copy.Pauses == 1, "Clone is a separate behaviour with the same state");
        copy.Lives = 9;
        Expect.That(behaviour.Lives == 5, "changing the copy leaves the original");
        Expect.That(new PlayableBehaviour().Clone() is PlayableBehaviour, "the base class clones too");
    }

    private static void NullHandlesAreNullAndInvalid()
    {
        Expect.That(!Playable.Null.IsValid(), "the null playable is not valid");
        Expect.That(Playable.Null.GetHandle().IsNull() && PlayableHandle.Null.IsNull() && !PlayableHandle.Null.IsValid(), "the null handle");
        Expect.That(Playable.Null.Equals(default(Playable)) && PlayableHandle.Null == default && Playable.Null == default(Playable), "nulls are equal");
        Expect.That(PlayableOutput.Null.IsOutputNull() && !PlayableOutput.Null.IsOutputValid(), "the null output");
        Expect.That(PlayableOutputHandle.Null.IsNull() && !PlayableOutputHandle.Null.IsValid(), "the null output handle");
        Expect.That(!default(PlayableGraph).IsValid(), "a default graph is not valid");
        Expect.That(ScriptPlayable<Counting>.Null.IsNull() && !ScriptPlayable<Counting>.Null.IsValid(), "the null script playable");
        Expect.That(ScriptPlayableOutput.Null.IsOutputNull() && AnimationPlayableOutput.Null.IsOutputNull(), "the null script and animation outputs");
        Expect.That(AnimationMotionXToDeltaPlayable.Null.Equals(default(AnimationMotionXToDeltaPlayable)) && AnimationMotionXToDeltaPlayable.Null.GetHandle().IsNull(),
            "the null motion playable");
        Playable motion = AnimationMotionXToDeltaPlayable.Null;
        Expect.That(motion.Equals(Playable.Null), "a null motion playable converts to the null playable");
        Playable converted = ScriptPlayable<Counting>.Null;
        Expect.That(converted.Equals(Playable.Null), "a null script playable converts to the null playable");
    }

    private static void RequireNamed(Action call, string member)
    {
        var error = Expect.Catch<NotSupportedException>(call, member);
        Expect.That(error.Message.Contains(member), $"{member} is named: {error.Message}");
    }

    private static void GraphAndPlayablesRefuseWork()
    {
        RequireNamed(() => PlayableGraph.Create(), "PlayableGraph.Create");
        RequireNamed(() => PlayableGraph.Create("graph"), "PlayableGraph.Create");
        var graph = default(PlayableGraph);
        RequireNamed(() => graph.Play(), "PlayableGraph.Play");
        RequireNamed(() => graph.Stop(), "PlayableGraph.Stop");
        RequireNamed(() => graph.Evaluate(0.1f), "PlayableGraph.Evaluate");
        RequireNamed(() => graph.Destroy(), "PlayableGraph.Destroy");
        RequireNamed(() => _ = graph.IsPlaying(), "PlayableGraph.IsPlaying");
        RequireNamed(() => _ = graph.GetRootPlayableCount(), "PlayableGraph.GetRootPlayableCount");
        RequireNamed(() => _ = graph.GetOutputCount(), "PlayableGraph.GetOutputCount");
        RequireNamed(() => _ = ScriptPlayable<Counting>.Create(graph, 1), "ScriptPlayable.Create");
        RequireNamed(() => _ = ScriptPlayableOutput.Create(graph, "out"), "ScriptPlayableOutput.Create");
        RequireNamed(() => _ = AnimationPlayableOutput.Create(graph, "anim", null!), "AnimationPlayableOutput.Create");
        RequireNamed(() => _ = Playable.Create(graph, 2), "Playable.Create");
        RequireNamed(() => _ = AnimationMotionXToDeltaPlayable.Create(graph), "AnimationMotionXToDeltaPlayable.Create");
        RequireNamed(() => _ = Playable.Null.GetPlayableType(), "Playable.GetPlayableType");
        RequireNamed(() => _ = ScriptPlayable<Counting>.Null.GetBehaviour(), "ScriptPlayable.GetBehaviour");
        RequireNamed(() => _ = ScriptPlayableBinding.Create("track", null, typeof(GameObject)), "ScriptPlayableBinding.Create");
        RequireNamed(() => _ = AnimationPlayableOutput.Null.GetTarget(), "AnimationPlayableOutput.GetTarget");
    }

    private static void DirectorRefusesPlayback()
    {
        Expect.That(typeof(PlayableDirector).BaseType == typeof(Behaviour) && typeof(IExposedPropertyTable).IsAssignableFrom(typeof(PlayableDirector)),
            "a director is a behaviour that holds exposed references");
        var director = new PlayableDirector();
        Expect.That(director.enabled, "a director starts enabled");
        director.enabled = false;
        Expect.That(!director.enabled, "and keeps its enabled flag");
        RequireNamed(() => director.Play(), "PlayableDirector.Play");
        RequireNamed(() => director.Stop(), "PlayableDirector.Stop");
        RequireNamed(() => director.Pause(), "PlayableDirector.Pause");
        RequireNamed(() => director.Resume(), "PlayableDirector.Resume");
        RequireNamed(() => director.Evaluate(), "PlayableDirector.Evaluate");
        RequireNamed(() => _ = director.state, "PlayableDirector.state");
        RequireNamed(() => _ = director.time, "PlayableDirector.time");
        RequireNamed(() => _ = director.playableAsset, "PlayableDirector.playableAsset");
        RequireNamed(() => director.playableAsset = null, "PlayableDirector.playableAsset");
        RequireNamed(() => _ = director.extrapolationMode, "PlayableDirector.extrapolationMode");
        RequireNamed(() => director.SetGenericBinding(null!, null), "PlayableDirector.SetGenericBinding");
        RequireNamed(() => _ = director.GetGenericBinding(null!), "PlayableDirector.GetGenericBinding");
        RequireNamed(() => director.SetReferenceValue("id", null!), "PlayableDirector.SetReferenceValue");
    }

    private static void PropertyNamesCompareByText()
    {
        Expect.That(PropertyName.None.IsNull() && default(PropertyName).IsNull() && new PropertyName("").IsNull(), "no name and the empty name are null");
        var intro = new PropertyName("intro");
        Expect.That(!intro.IsNull() && intro == new PropertyName("intro") && intro.GetHashCode() == new PropertyName("intro").GetHashCode(), "one text, one name");
        Expect.That(intro != new PropertyName("outro") && !intro.Equals(new PropertyName("outro")), "different texts differ");
        PropertyName implied = "intro";
        Expect.That(implied == intro, "a string converts to the name of that text");
        Expect.That(new PropertyName(intro) == intro, "a copy is equal");
    }

    private sealed class Table : IExposedPropertyTable
    {
        public readonly Dictionary<PropertyName, UnityEngine.Object> Values = new();
        public void SetReferenceValue(PropertyName id, UnityEngine.Object value) => Values[id] = value;
        public UnityEngine.Object GetReferenceValue(PropertyName id, out bool idValid)
        {
            idValid = Values.TryGetValue(id, out var value);
            return value!;
        }
        public void ClearReferenceValue(PropertyName id) => Values.Remove(id);
    }

    private static void ExposedReferencesResolveThroughTheTable()
    {
        var fallback = Named("fallback");
        var chosen = Named("chosen");
        var reference = new ExposedReference<GameObject> { exposedName = "target", defaultValue = fallback };
        var table = new Table();
        Expect.That(ReferenceEquals(reference.Resolve(table), fallback), "a name the table lacks resolves to the default");
        Expect.That(ReferenceEquals(reference.Resolve(null!), fallback), "no table resolves to the default");
        table.SetReferenceValue("target", chosen);
        Expect.That(ReferenceEquals(reference.Resolve(table), chosen), "a name the table holds resolves to its value");
        table.ClearReferenceValue("target");
        Expect.That(ReferenceEquals(reference.Resolve(table), fallback), "a cleared name resolves to the default again");
        var other = new ExposedReference<Transform> { exposedName = "target" };
        table.SetReferenceValue("target", chosen);
        Expect.That(other.Resolve(table) is null, "a value of another type resolves to null");
    }

    private static void BindingsAndFramesHoldData()
    {
        var owner = Named("track");
        var binding = new PlayableBinding
        {
            streamName = "Track", streamType = PlayableBinding.DataStreamType.Animation, sourceObject = owner, outputTargetType = typeof(Animator),
        };
        Expect.That(binding.streamName == "Track" && binding.streamType == PlayableBinding.DataStreamType.Animation
            && ReferenceEquals(binding.sourceObject, owner) && binding.outputTargetType == typeof(Animator), "a binding holds what it was given");
        Expect.That(default(PlayableBinding).streamType == PlayableBinding.DataStreamType.Animation, "the first stream type is the default");
        var frame = new FrameData
        {
            frameId = 7, deltaTime = 0.5f, weight = 0.25f, effectiveWeight = 0.125f, effectiveParentDelay = 1d, effectiveParentSpeed = 2d,
            effectiveSpeed = 3d, evaluationType = FrameData.EvaluationType.Playback, seekOccurred = true, timeLooped = false, timeHeld = true,
            effectivePlayState = PlayState.Delayed,
        };
        Expect.That(frame.frameId == 7 && frame.evaluationType == FrameData.EvaluationType.Playback && frame.effectivePlayState == PlayState.Delayed, "frame state");
        Expect.That(frame.seekOccurred && !frame.timeLooped && frame.timeHeld, "frame flags");
        Expect.Near(0.5f, frame.deltaTime, "delta time");
        Expect.Near(0.25f, frame.weight, "weight");
        Expect.Near(0.125f, frame.effectiveWeight, "effective weight");
        Expect.Near(3f, (float)frame.effectiveSpeed, "effective speed");
        Expect.Near(2f, (float)frame.effectiveParentSpeed, "effective parent speed");
        Expect.Near(1f, (float)frame.effectiveParentDelay, "effective parent delay");
        Expect.That(default(FrameData).evaluationType == FrameData.EvaluationType.Evaluate, "the first evaluation type is the default");
    }

    private sealed class Marker : INotification
    {
        public PropertyName id => "marker";
    }

    private sealed class Receiver : INotificationReceiver
    {
        public INotification? Last;
        public object? Context;
        public void OnNotify(Playable origin, INotification notification, object context)
        {
            Last = notification;
            Context = context;
        }
    }

    private static void NotificationsReachReceivers()
    {
        var receiver = new Receiver();
        INotificationReceiver asReceiver = receiver;
        asReceiver.OnNotify(Playable.Null, new Marker(), "ctx");
        Expect.That(receiver.Last is Marker { id: var id } && id == new PropertyName("marker") && "ctx".Equals(receiver.Context), "the receiver sees the notification");
    }

    private static void PlayerLoopHoldsSystemsAndRefusesTheLoop()
    {
        var calls = 0;
        var child = new PlayerLoopSystem { type = typeof(Marker), updateDelegate = () => calls++ };
        var root = new PlayerLoopSystem { type = typeof(Receiver), subSystemList = [child] };
        root.subSystemList[0].updateDelegate!();
        Expect.That(calls == 1 && root.subSystemList.Length == 1 && root.subSystemList[0].type == typeof(Marker), "systems hold their delegate and children");
        Expect.That(root.updateFunction == IntPtr.Zero && root.loopConditionFunction == IntPtr.Zero, "native function pointers default to zero");
        Expect.That(root.ToString() == nameof(Receiver), "a system prints its type name");
        RequireNamed(() => _ = PlayerLoop.GetCurrentPlayerLoop(), "PlayerLoop.GetCurrentPlayerLoop");
        RequireNamed(() => _ = PlayerLoop.GetDefaultPlayerLoop(), "PlayerLoop.GetDefaultPlayerLoop");
        RequireNamed(() => PlayerLoop.SetPlayerLoop(root), "PlayerLoop.SetPlayerLoop");
    }
}
