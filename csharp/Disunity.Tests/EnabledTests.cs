using System.Reflection;
using System.Reflection.Emit;
using Disunity.Hosting;
using UnityEngine;

internal static class EnabledTests
{
    private sealed class Probe : MonoBehaviour
    {
        public int Starts, Updates, Enables, Disables;
        private void Start() => Starts++;
        private void Update() => Updates++;
        private void OnEnable() => Enables++;
        private void OnDisable() => Disables++;
        public void Toggle(bool on) => base.enabled = on;
        public bool Read() => base.enabled;
    }
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        DisunityHost.Step(0.01f);
        probe.enabled = false;
        if (probe.enabled || probe.Disables != 1) throw new Exception("native disable must drain before returning");
        DisunityHost.Step(0.01f);
        if (probe.Updates != 1) throw new Exception("disabled script received Update");
        probe.enabled = true;
        DisunityHost.Step(0.01f);
        if (!probe.enabled || probe.Enables != 2 || probe.Starts != 1 || probe.Updates != 2) throw new Exception("native enabled eligibility");
        BaseAccess(probe);
        NativeBehaviourThroughBase();
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: native script enabled state and exactly-once Start");
    }

    /// <summary>A subclass reaching <c>base.enabled</c> compiles to a non-virtual call that must still hit the script's state.</summary>
    private static void BaseAccess(Probe probe)
    {
        var disables = probe.Disables;
        probe.Toggle(false);
        if (probe.enabled || probe.Read() || probe.Disables != disables + 1) throw new Exception("base.enabled = false must disable the script");
        probe.Toggle(true);
        if (!probe.enabled || !probe.Read()) throw new Exception("base.enabled = true must enable the script");
        NonVirtualCallsOnBehaviour(probe);
        Console.WriteLine("PASS: base.enabled reaches the script's native state");
    }

    /// <summary>
    /// A game built against Unity's metadata binds <c>base.enabled</c> to <c>Behaviour</c> itself, so its IL makes
    /// non-virtual calls to the Behaviour accessors. The runtime must accept those calls and they must reach the script.
    /// </summary>
    private static void NonVirtualCallsOnBehaviour(Probe probe)
    {
        var property = typeof(Behaviour).GetProperty(nameof(Behaviour.enabled))!;
        var set = Accessor("SetEnabled", typeof(void), property.SetMethod!, typeof(bool));
        var get = Accessor("GetEnabled", typeof(bool), property.GetMethod!);
        ((Action<Behaviour, bool>)set.CreateDelegate(typeof(Action<Behaviour, bool>)))(probe, false);
        if (probe.Read()) throw new Exception("a non-virtual Behaviour.set_enabled call must disable the script");
        ((Action<Behaviour, bool>)set.CreateDelegate(typeof(Action<Behaviour, bool>)))(probe, true);
        if (!((Func<Behaviour, bool>)get.CreateDelegate(typeof(Func<Behaviour, bool>)))(probe)) throw new Exception("a non-virtual Behaviour.get_enabled call must read the script");
    }

    private static DynamicMethod Accessor(string name, Type result, MethodInfo target, params Type[] extra)
    {
        var parameters = new[] { typeof(Behaviour) }.Concat(extra).ToArray();
        var method = new DynamicMethod(name, result, parameters, typeof(EnabledTests), skipVisibility: true);
        var il = method.GetILGenerator();
        for (var i = 0; i < parameters.Length; i++) il.Emit(OpCodes.Ldarg, i);
        il.Emit(OpCodes.Call, target);
        il.Emit(OpCodes.Ret);
        return method;
    }

    private static void NativeBehaviourThroughBase()
    {
        Behaviour listener = DisunityHost.CreateGameObject().AddComponent<AudioListener>();
        listener.enabled = false;
        if (listener.enabled) throw new Exception("a native behaviour disabled through Behaviour must read false");
        listener.enabled = true;
        if (!listener.enabled) throw new Exception("a native behaviour enabled through Behaviour must read true");
        Console.WriteLine("PASS: native behaviour enabled through a Behaviour reference");
    }
}
