using System.Collections;
using System.Reflection;
using System.Runtime.ExceptionServices;
using UnityEngine;

namespace Disunity.Hosting;

internal static class ScriptMessages
{
    private static readonly Dictionary<(Type, string, Type?), MethodInfo?> Methods = new();
    private static readonly Dictionary<Type, uint> Defined = new();
    private static readonly Dictionary<(Type, string), bool> Declares = new();
    private const BindingFlags Declared = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

    /// <summary>The per-frame messages the type defines, as native bits: 1 FixedUpdate, 2 Update, 4 LateUpdate.
    /// A method found on the type or a base type below MonoBehaviour counts, as the lookup in
    /// <see cref="Invoke(UnityEngine.Object, string, object?)"/> finds it.</summary>
    internal static uint PerFrameMessages(Type type)
    {
        if (Defined.TryGetValue(type, out var bits)) return bits;
        string[] names = ["FixedUpdate", "Update", "LateUpdate"];
        for (var bit = 0; bit < names.Length; bit++)
        {
            for (var t = type; t != null && t != typeof(MonoBehaviour); t = t.BaseType)
                if (t.GetMethod(names[bit], Declared, null, Type.EmptyTypes, null) != null) { bits |= 1u << bit; break; }
        }
        Defined.Add(type, bits);
        return bits;
    }

    /// <summary>Whether the type, or a base type below MonoBehaviour, declares the parameterless message, as
    /// <see cref="Invoke(UnityEngine.Object, string, object?)"/> would find it.</summary>
    internal static bool Defines(Type type, string name)
    {
        if (Declares.TryGetValue((type, name), out var found)) return found;
        for (var t = type; t != null && t != typeof(MonoBehaviour) && !found; t = t.BaseType)
            found = t.GetMethod(name, Declared, null, Type.EmptyTypes, null) != null;
        Declares.Add((type, name), found);
        return found;
    }

    /// <summary>Whether a message lookup for the type has run, which shows a native callback reached it.</summary>
    internal static bool HasLooked(Type type, string name) => Methods.Keys.Any(key => key.Item1 == type && key.Item2 == name);

    internal static void Invoke(MonoBehaviour script, uint message)
    {
        string[] names = ["Awake", "OnEnable", "Start", "FixedUpdate", "Update", "LateUpdate", "OnDisable", "OnDestroy"];
        if (message >= names.Length) throw new NotSupportedException($"native callback {message}");
        var result = Invoke(script, names[message]);
        if (result is IEnumerator iterator) script.StartCoroutine(iterator);
    }

    /// <summary>Calls the message on the script. An exception the script throws is logged against
    /// it and the result is null, as Unity reports it and goes on. A native error fails the step.</summary>
    internal static object? Invoke(UnityEngine.Object script, string name, object? argument = null)
    {
        var parameter = argument == null ? null : argument is Collider2D ? typeof(Collider2D) : argument.GetType();
        var key = (script.GetType(), name, parameter);
        if (!Methods.TryGetValue(key, out var method))
        {
            for (var type = key.Item1; type != null && method == null; type = type.BaseType)
                method = type.GetMethod(name, Declared,
                    null, parameter == null ? Type.EmptyTypes : [parameter], null);
            if (method != null && (method.ContainsGenericParameters || method.IsAbstract ||
                (method.ReturnType != typeof(void) && !(name == "Start" && typeof(IEnumerator).IsAssignableFrom(method.ReturnType)))))
                throw new NotSupportedException($"unsupported script message {key}");
            Methods.Add(key, method);
        }
        // Only an exception thrown inside the script's method arrives wrapped; a binding or
        // lookup failure is the engine's own and propagates, as does a native error.
        try { return method?.Invoke(script, argument == null ? null : [argument]); }
        catch (TargetInvocationException error) when (error.InnerException is { } inner)
        {
            if (!DisunityHost.IsGameException(inner)) ExceptionDispatchInfo.Capture(inner).Throw();
            DisunityHost.ReportScriptException(inner, script);
            return null;
        }
    }
}
