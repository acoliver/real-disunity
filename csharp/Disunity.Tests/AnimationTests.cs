using System.Reflection;
using Disunity.Hosting;
using UnityEngine;

/// <summary>The animation types: Animators cannot be attached, hashes match Unity, and clips, overrides and masks hold data.</summary>
internal static class AnimationTests
{
    public static void Run()
    {
        AddComponentIsRefused();
        StringToHashMatchesUnity();
        AnimatorHoldsDefaultsAndIgnoresCommands();
        AnimatorRefusesPlayback();
        ClipHoldsSettingsAndOrderedEvents();
        LegacyClipsHoldCurves();
        StateInfoMatchesNames();
        TransitionAndClipInfo();
        OverrideControllerHoldsData();
        AvatarMaskHoldsData();
        StateMachineBehavioursDoNothingByDefault();
        TypeShapesAndEnums();
        Console.WriteLine("PASS: Animators cannot be attached, state hashes match Unity, and clips, overrides, masks and state structs behave as data");
    }

    private static List<string> Warnings(Action action)
    {
        var seen = new List<string>();
        void Collect(string condition, string stackTrace, LogType type)
        {
            if (type == LogType.Warning) seen.Add(condition);
        }
        Application.logMessageReceivedThreaded += Collect;
        try { action(); }
        finally { Application.logMessageReceivedThreaded -= Collect; }
        return seen;
    }

    private static void AddComponentIsRefused()
    {
        DisunityHost.LoadWorld();
        try
        {
            var go = DisunityHost.CreateGameObject();
            var error = Refusal.Of(() => go.AddComponent<Animator>());
            Expect.That(error.Message == "Animator components are not supported yet", $"message: {error.Message}");
            Expect.That(go.GetComponent<Animator>() == null, "the Animator stays off the GameObject");
            var body = go.AddComponent<Rigidbody2D>();
            Expect.That(go.GetComponent<Rigidbody2D>() == body, "other components still attach after a refusal");
            Expect.Throws<NotSupportedException>(() => go.AddComponent<Rigidbody>(), "3D physics components are still refused");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void StringToHashMatchesUnity()
    {
        // Idle and Attack are values Unity users have published for Animator.StringToHash.
        Expect.Equal(2081823275, Animator.StringToHash("Idle"), "Idle");
        Expect.Equal(1080829965, Animator.StringToHash("Attack"), "Attack");
        // The rest are IEEE CRC-32 (zlib) of the UTF-8 bytes, as a signed int.
        Expect.Equal(1432961145, Animator.StringToHash("Base Layer.Idle"), "Base Layer.Idle");
        Expect.Equal(-823668238, Animator.StringToHash("Speed"), "a hash above int.MaxValue wraps negative");
        Expect.Equal(0, Animator.StringToHash(""), "the empty string");
        Expect.Equal(unchecked((int)0x98AD42B5u), Animator.StringToHash("caf\u00e9"), "non-ASCII text hashes its UTF-8 bytes");
        Expect.Throws<ArgumentNullException>(() => Animator.StringToHash(null!), "null name");
    }

    private static void AnimatorHoldsDefaultsAndIgnoresCommands()
    {
        var animator = new Animator();
        Expect.That(animator.enabled && animator.speed == 1f && animator.logWarnings && animator.fireEvents, "enabled, speed 1, warnings and events on");
        Expect.That(animator.cullingMode == AnimatorCullingMode.AlwaysAnimate && animator.updateMode == AnimatorUpdateMode.Normal, "culling and update modes");
        Expect.That(!animator.hasBoundPlayables && !animator.hasRootMotion && !animator.isHuman && animator.avatar == null, "no playables, root motion or avatar");
        Expect.That(animator.layerCount == 0 && animator.parameterCount == 0 && animator.parameters.Length == 0, "no layers or parameters");
        Expect.That(animator.GetLayerIndex("Base Layer") == -1 && animator.GetLayerName(0) == "" && !animator.IsInTransition(0), "no layers by name or index");
        Expect.That(animator.GetCurrentAnimatorStateInfo(0).fullPathHash == 0 && animator.GetCurrentAnimatorClipInfo(0).Length == 0, "no current state or clips");
        Expect.That(animator.GetBehaviour<StateMachineBehaviour>() == null && animator.GetBehaviours<StateMachineBehaviour>().Length == 0, "no behaviours");
        Expect.Throws<IndexOutOfRangeException>(() => animator.GetParameter(0), "GetParameter on an animator without parameters");
        animator.speed = 2.5f;
        animator.cullingMode = AnimatorCullingMode.CullCompletely;
        animator.keepAnimatorStateOnDisable = true;
        Expect.That(animator.speed == 2.5f && animator.cullingMode == AnimatorCullingMode.CullCompletely && animator.keepAnimatorStateOnDisable, "settings round trip");
        var controller = new RuntimeAnimatorController();
        animator.runtimeAnimatorController = controller;
        Expect.That(animator.runtimeAnimatorController == controller, "the controller is stored");

        var warnings = Warnings(() =>
        {
            Expect.Equal(0f, animator.GetFloat("Speed"), "GetFloat");
            animator.SetBool("Run", true);
            Expect.That(!animator.GetBool("Run"), "SetBool changes nothing: the parameter does not exist");
            animator.Play("Idle");
        });
        Expect.That(warnings.Count == 4 && warnings[0].StartsWith("Animator.GetFloat(Speed) was ignored"), $"commands warn: {string.Join("|", warnings)}");
        animator.logWarnings = false;
        Expect.That(Warnings(() => animator.SetTrigger("Jump")).Count == 0, "logWarnings off silences the warning");
    }

    private static void AnimatorRefusesPlayback()
    {
        var animator = new Animator();
        foreach (var (name, call) in new (string, Action)[]
        {
            ("Update", () => animator.Update(0.1f)),
            ("Rebind", animator.Rebind),
            ("StartRecording", () => animator.StartRecording(10)),
            ("GetIKPosition", () => animator.GetIKPosition(AvatarIKGoal.LeftFoot)),
            ("SetLookAtWeight", () => animator.SetLookAtWeight(1f)),
            ("MatchTarget", () => animator.MatchTarget(default, default, AvatarTarget.Root, default, 0f)),
        })
        {
            var error = Refusal.Of(call);
            Expect.That(error.Message.StartsWith($"Animator.{name} is not supported yet"), $"{name}: {error.Message}");
        }
    }

    private static void ClipHoldsSettingsAndOrderedEvents()
    {
        var clip = new AnimationClip();
        Expect.That(clip.frameRate == 60f && clip.length == 0f && clip.empty && !clip.legacy, "a new clip: 60 fps, empty, not legacy");
        Expect.That(!clip.hasRootCurves && !clip.hasMotionCurves && !clip.hasMotionFloatCurves && !clip.hasGenericRootTransform && !clip.humanMotion, "no curves of any kind");
        Expect.That(clip.events.Length == 0 && clip.wrapMode == WrapMode.Default && !clip.isLooping, "no events, default wrap mode");
        clip.frameRate = 30f;
        clip.legacy = true;
        clip.wrapMode = WrapMode.Loop;
        Expect.That(clip.frameRate == 30f && clip.legacy && ((Motion)clip).legacy && clip.isLooping && !clip.isAnimatorMotion, "settings round trip");

        clip.AddEvent(new AnimationEvent { time = 2f, functionName = "Late" });
        clip.AddEvent(new AnimationEvent { time = 0.5f, functionName = "Early" });
        clip.AddEvent(new AnimationEvent { time = 2f, functionName = "LateToo", intParameter = 7 });
        Expect.Sequence(["Early", "Late", "LateToo"], clip.events.Select(e => e.functionName), "events sort by time, equal times keep insertion order");
        var first = clip.events[0];
        first.functionName = "Changed";
        Expect.That(clip.events[0].functionName == "Early", "events hands out copies");
        var added = new AnimationEvent { time = 1f, functionName = "Mid" };
        clip.AddEvent(added);
        added.functionName = "Mutated";
        Expect.That(clip.events[1].functionName == "Mid", "AddEvent stores a copy");
        clip.events = [new AnimationEvent { time = 3f, functionName = "B" }, new AnimationEvent { time = 1f, functionName = "A" }];
        Expect.Sequence(["A", "B"], clip.events.Select(e => e.functionName), "assigning events sorts them");
        var evt = new AnimationEvent();
        Expect.That(evt.messageOptions == SendMessageOptions.RequireReceiver && evt.data == "" && !evt.isFiredByAnimator && !evt.isFiredByLegacy, "event defaults");
        Expect.Throws<ArgumentNullException>(() => clip.AddEvent(null!), "null event");
        var sample = Refusal.Of(() => clip.SampleAnimation(null!, 0f));
        Expect.That(sample.Message.StartsWith("AnimationClip.SampleAnimation is not supported yet"), sample.Message);
        var info = new AnimatorClipInfo();
        Expect.That(info.clip == null && info.weight == 0f, "a default clip info has no clip");
    }

    private static List<string> Errors(Action action)
    {
        var seen = new List<string>();
        void Collect(string condition, string stackTrace, LogType type)
        {
            if (type == LogType.Error) seen.Add(condition);
        }
        Application.logMessageReceivedThreaded += Collect;
        try { action(); }
        finally { Application.logMessageReceivedThreaded -= Collect; }
        return seen;
    }

    /// <summary>SetCurve stores copies on legacy clips only; length spans the keys; continuity flips
    /// rotation keys into the previous key's hemisphere.</summary>
    private static void LegacyClipsHoldCurves()
    {
        var clip = new AnimationClip();
        var curve = AnimationCurve.Linear(0.5f, 0f, 2f, 1f);
        Expect.That(Errors(() => clip.SetCurve("", typeof(Transform), "localPosition.x", curve)).Count == 1 && clip.empty,
            "a non-legacy clip refuses SetCurve at runtime with an error");
        clip.legacy = true;
        clip.SetCurve("", typeof(Transform), "localPosition.x", curve);
        clip.SetCurve("Child", typeof(Transform), "localPosition.y", AnimationCurve.Linear(0f, 0f, 1f, 1f));
        Expect.That(!clip.empty && clip.length == 2f, $"length spans the keys of every curve: {clip.length}");
        curve.AddKey(5f, 1f);
        Expect.That(clip.length == 2f, "the clip keeps a copy of the curve");
        clip.SetCurve("Child", typeof(Transform), "localPosition.y", null!);
        Expect.That(clip.length == 1.5f, $"a null curve removes it: {clip.length}");
        var copy = (AnimationClip)UnityEngine.Object.Instantiate(clip);
        clip.ClearCurves();
        Expect.That(clip.empty && clip.length == 0f && !copy.empty && copy.length == 1.5f, "ClearCurves empties the clip and a clone keeps its curves");

        var rotation = new AnimationClip { legacy = true };
        float[][] values = [[0f, 0f, 0f, 1f], [0f, 0f, 0.6f, -0.8f], [0f, 0f, 0.8f, 0.6f]];
        for (var c = 0; c < 4; c++)
            rotation.SetCurve("", typeof(Transform), "localRotation." + "xyzw"[c], new AnimationCurve(
                new Keyframe(0f, values[0][c]), new Keyframe(1f, values[1][c]), new Keyframe(2f, values[2][c])));
        rotation.EnsureQuaternionContinuity();
        var find = typeof(AnimationClip).GetMethod("FindCurve", BindingFlags.Instance | BindingFlags.NonPublic)!;
        float[] Values(char axis) => ((AnimationCurve)find.Invoke(rotation, ["", typeof(Transform), "localRotation." + axis])!).keys.Select(k => k.value).ToArray();
        Expect.Sequence([1f, 0.8f, 0.6f], Values('w'), "a key in the opposite hemisphere is negated");
        Expect.Sequence([0f, -0.6f, 0.8f], Values('z'), "every component of a negated key flips, later keys follow the flipped one");
        var mismatched = new AnimationClip { legacy = true };
        for (var c = 0; c < 4; c++)
            mismatched.SetCurve("", typeof(Transform), "localRotation." + "xyzw"[c], AnimationCurve.Linear(0f, 0f, c + 1, 1f));
        Expect.Throws<NotSupportedException>(mismatched.EnsureQuaternionContinuity, "rotation curves with different key times");
        Expect.Throws<ArgumentNullException>(() => rotation.SetCurve("", null!, "x", curve), "a null type");
    }

    private static AnimatorStateInfo StateInfo(string path, string shortName, string parent)
    {
        object boxed = default(AnimatorStateInfo);
        void Set(string field, object value) => typeof(AnimatorStateInfo).GetField(field, BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(boxed, value);
        Set("m_FullPath", Animator.StringToHash(path));
        Set("m_Name", Animator.StringToHash(shortName));
        Set("m_Path", Animator.StringToHash(parent));
        Set("m_Tag", Animator.StringToHash("Enemy"));
        Set("m_NormalizedTime", 1.5f);
        Set("m_Loop", 1);
        return (AnimatorStateInfo)boxed;
    }

    private static void StateInfoMatchesNames()
    {
        var state = StateInfo("Base Layer.Idle", "Idle", "Base Layer");
        Expect.That(state.IsName("Idle") && state.IsName("Base Layer.Idle") && state.IsName("Base Layer"), "IsName accepts the short name, full path and parent path");
        Expect.That(!state.IsName("Walk") && !state.IsName("idle"), "IsName rejects other names, case sensitively");
        Expect.That(state.fullPathHash == Animator.StringToHash("Base Layer.Idle") && state.shortNameHash == Animator.StringToHash("Idle"), "hash properties");
        Expect.That(state.nameHash == Animator.StringToHash("Base Layer"), "nameHash is the parent path hash");
        Expect.That(state.IsTag("Enemy") && !state.IsTag("Friend") && state.loop && state.normalizedTime == 1.5f, "tag, loop and normalized time");
        Expect.That(!default(AnimatorStateInfo).loop && !default(AnimatorStateInfo).IsName("Idle"), "a default state matches no name");
    }

    private static void TransitionAndClipInfo()
    {
        object boxed = default(AnimatorTransitionInfo);
        void Set(string field, object value) => typeof(AnimatorTransitionInfo).GetField(field, BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(boxed, value);
        Set("m_FullPath", Animator.StringToHash("Base Layer.Idle -> Walk"));
        Set("m_Name", Animator.StringToHash("Idle -> Walk"));
        Set("m_UserName", Animator.StringToHash("Start"));
        Set("m_HasFixedDuration", true);
        Set("m_AnyState", true);
        var transition = (AnimatorTransitionInfo)boxed;
        Expect.That(transition.IsName("Idle -> Walk") && transition.IsName("Base Layer.Idle -> Walk") && !transition.IsName("Start"), "transition names");
        Expect.That(transition.IsUserName("Start") && !transition.IsUserName("Idle -> Walk"), "user names");
        Expect.That(transition.durationUnit == DurationUnit.Fixed && transition.anyState, "fixed duration, any state");
        Expect.That(default(AnimatorTransitionInfo).durationUnit == DurationUnit.Normalized, "a default transition is normalized");

        var clip = new AnimationClip { name = "Walk" };
        var constructor = typeof(AnimatorClipInfo).GetConstructor(BindingFlags.Instance | BindingFlags.NonPublic, [typeof(AnimationClip), typeof(float)])!;
        var info = (AnimatorClipInfo)constructor.Invoke([clip, 0.25f]);
        Expect.That(info.clip == clip && info.weight == 0.25f, "a clip info finds its clip by instance ID");
    }

    private sealed class TwoClips(AnimationClip a, AnimationClip b) : RuntimeAnimatorController
    {
        public override AnimationClip[] animationClips => [a, b];
    }

    private static void OverrideControllerHoldsData()
    {
        var idle = new AnimationClip { name = "Idle" };
        var walk = new AnimationClip { name = "Walk" };
        var fast = new AnimationClip { name = "FastWalk" };
        var empty = new AnimatorOverrideController();
        Expect.That(empty.runtimeAnimatorController == null && empty.overridesCount == 0 && empty.animationClips.Length == 0, "an override controller without a base has no clips");
        var overrides = new AnimatorOverrideController(new TwoClips(idle, walk));
        Expect.That(overrides.overridesCount == 2, "one override slot per base clip");
        Expect.That(overrides["Walk"] == walk && overrides[idle] == idle, "an unoverridden clip reads as itself");
        overrides["Walk"] = fast;
        Expect.That(overrides["Walk"] == fast && overrides[walk] == fast, "an override is found by name and by original clip");
        Expect.That(overrides.animationClips.SequenceEqual([idle, fast]), "animationClips lists the effective clips");
        var pairs = new List<KeyValuePair<AnimationClip, AnimationClip>>();
        overrides.GetOverrides(pairs);
        Expect.That(pairs.Count == 2 && pairs[0].Key == idle && pairs[0].Value == null && pairs[1].Key == walk && pairs[1].Value == fast, "GetOverrides pairs originals with overrides or null");
        overrides.ApplyOverrides([new(idle, fast), new(walk, null!)]);
        Expect.That(overrides[idle] == fast && overrides[walk] == walk, "ApplyOverrides sets and clears overrides");
        Expect.That(Warnings(() => overrides["Missing"] = fast).Count == 1 && overrides["Missing"] == null, "an unknown clip name warns and reads null");
        Expect.Throws<ArgumentNullException>(() => overrides.GetOverrides(null!), "null list");
        overrides.runtimeAnimatorController = null;
        Expect.That(overrides.overridesCount == 0, "dropping the base controller drops the slots");
    }

    private static void AvatarMaskHoldsData()
    {
        var mask = new AvatarMask();
        Expect.That(mask.humanoidBodyPartCount == 13 && mask.transformCount == 0, "13 body parts, no transforms");
        Expect.That(Enum.GetValues<AvatarMaskBodyPart>().Where(p => p != AvatarMaskBodyPart.LastBodyPart).All(mask.GetHumanoidBodyPartActive), "every body part starts active");
        mask.SetHumanoidBodyPartActive(AvatarMaskBodyPart.Head, false);
        Expect.That(!mask.GetHumanoidBodyPartActive(AvatarMaskBodyPart.Head) && mask.GetHumanoidBodyPartActive(AvatarMaskBodyPart.Body), "one body part toggles");
        Expect.Throws<ArgumentOutOfRangeException>(() => mask.GetHumanoidBodyPartActive(AvatarMaskBodyPart.LastBodyPart), "LastBodyPart is not a part");
        mask.transformCount = 2;
        mask.SetTransformPath(0, "Hips/Spine");
        mask.SetTransformActive(1, false);
        Expect.That(mask.GetTransformPath(0) == "Hips/Spine" && mask.GetTransformActive(0) && mask.GetTransformPath(1) == "" && !mask.GetTransformActive(1), "transform paths round trip");
        mask.transformCount = 1;
        Expect.Throws<ArgumentOutOfRangeException>(() => mask.GetTransformPath(1), "a removed transform slot");
        var error = Refusal.Of(() => mask.AddTransformPath(null!));
        Expect.That(error.Message.StartsWith("AvatarMask.AddTransformPath is not supported yet"), error.Message);
        var description = new HumanDescription { upperArmTwist = 0.5f, hasTranslationDoF = true };
        var limit = new HumanLimit { useDefaultValues = true, axisLength = 2f };
        Expect.That(description.upperArmTwist == 0.5f && description.hasTranslationDoF && limit.useDefaultValues && limit.axisLength == 2f, "human description and limit round trip");
    }

    private sealed class Quiet : StateMachineBehaviour;

    private sealed class Counting : StateMachineBehaviour
    {
        public int Entered;
        public override void OnStateEnter(Animator animator, AnimatorStateInfo stateInfo, int layerIndex) => Entered += layerIndex + 1;
    }

    private static void StateMachineBehavioursDoNothingByDefault()
    {
        DisunityHost.LoadWorld();
        try
        {
            var quiet = ScriptableObject.CreateInstance<Quiet>();
            var animator = new Animator();
            var info = default(AnimatorStateInfo);
            Expect.That(Warnings(() =>
            {
                quiet.OnStateEnter(animator, info, 0);
                quiet.OnStateUpdate(animator, info, 0);
                quiet.OnStateExit(animator, info, 0);
                quiet.OnStateMove(animator, info, 0);
                quiet.OnStateIK(animator, info, 0);
                quiet.OnStateMachineEnter(animator, 1);
                quiet.OnStateMachineExit(animator, 1);
            }).Count == 0 && quiet != null, "the default messages return without effect");
            var counting = ScriptableObject.CreateInstance<Counting>();
            counting.OnStateEnter(animator, info, 2);
            Expect.That(counting.Entered == 3, "a subclass overrides a message");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void TypeShapesAndEnums()
    {
        Expect.That(typeof(Animator).BaseType == typeof(Behaviour) && !typeof(Animator).IsSealed, "Animator is a Behaviour");
        Expect.That(typeof(AnimationClip).BaseType == typeof(Motion) && typeof(AnimationClip).IsSealed, "AnimationClip is a sealed Motion");
        Expect.That(typeof(AnimatorOverrideController).BaseType == typeof(RuntimeAnimatorController), "AnimatorOverrideController derives from RuntimeAnimatorController");
        Expect.That(typeof(RuntimeAnimatorController).BaseType == typeof(UnityEngine.Object) && typeof(Motion).BaseType == typeof(UnityEngine.Object), "controllers and motions are Objects");
        Expect.That(typeof(StateMachineBehaviour).IsAbstract && typeof(StateMachineBehaviour).BaseType == typeof(ScriptableObject), "StateMachineBehaviour is an abstract ScriptableObject");
        Expect.That(typeof(AnimatorStateInfo).IsValueType && typeof(AnimatorClipInfo).IsValueType && typeof(HumanDescription).IsValueType, "state infos and descriptions are structs");
        Expect.Sequence(["m_Name", "m_Path", "m_FullPath", "m_NormalizedTime", "m_Length", "m_Speed", "m_SpeedMultiplier", "m_Tag", "m_Loop"],
            typeof(AnimatorStateInfo).GetFields(BindingFlags.Instance | BindingFlags.NonPublic).Select(f => f.Name), "AnimatorStateInfo field layout");
        Expect.Sequence(["m_FullPath", "m_UserName", "m_Name", "m_HasFixedDuration", "m_Duration", "m_NormalizedTime", "m_AnyState", "m_TransitionType"],
            typeof(AnimatorTransitionInfo).GetFields(BindingFlags.Instance | BindingFlags.NonPublic).Select(f => f.Name), "AnimatorTransitionInfo field layout");
        Expect.Sequence(["m_ClipInstanceID", "m_Weight"], typeof(AnimatorClipInfo).GetFields(BindingFlags.Instance | BindingFlags.NonPublic).Select(f => f.Name), "AnimatorClipInfo field layout");
        Expect.Equal(9, (int)AnimatorControllerParameterType.Trigger, "Trigger");
        Expect.Equal(3, (int)AnimatorControllerParameterType.Int, "Int");
        Expect.Equal(54, (int)HumanBodyBones.UpperChest, "UpperChest");
        Expect.Equal(55, (int)HumanBodyBones.LastBone, "LastBone");
        Expect.Equal(2, (int)AnimatorRecorderMode.Record, "Record");
        var parameter = new AnimatorControllerParameter { type = AnimatorControllerParameterType.Float, defaultFloat = 1f };
        Expect.That(parameter.Equals(new AnimatorControllerParameter { type = AnimatorControllerParameterType.Float, defaultFloat = 1f })
            && !parameter.Equals(new AnimatorControllerParameter()) && parameter.nameHash == 0, "parameters compare by value");
    }
}

/// <summary>Runs an action that must throw and returns the exception.</summary>
internal static class Refusal
{
    public static NotSupportedException Of(Action action)
    {
        try { action(); }
        catch (NotSupportedException error) { return error; }
        throw new InvalidOperationException("expected NotSupportedException, nothing was thrown");
    }
}
