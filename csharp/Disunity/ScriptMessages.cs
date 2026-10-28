using System.Collections;
using System.Reflection;
using System.Runtime.ExceptionServices;
using UnityEngine;

namespace Disunity.Hosting;

internal static class ScriptMessages
{
    private static readonly Dictionary<(Type, string, Type?), MethodInfo?> Methods = new();
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
                method = type.GetMethod(name, BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly,
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
