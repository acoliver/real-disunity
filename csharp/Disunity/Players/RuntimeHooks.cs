using System.Reflection;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>A startup hook that cannot run, and why.</summary>
public sealed record UnresolvedHook(RuntimeHook Hook, string Reason);

/// <summary>
/// A player's startup hooks resolved to their methods. Each phase runs its hooks in listed
/// order; the phases run in <see cref="PhaseOrder"/>. A hook must name a static, non-generic
/// method with no parameters. Resolution never skips a hook: one that cannot be resolved is
/// kept in <see cref="Unresolved"/>, and running its phase fails with its name. A hook that
/// throws stops its phase; Unity instead logs the exception and continues.
/// </summary>
public sealed class RuntimeHooks
{
    /// <summary>The order in which Unity runs the startup phases.</summary>
    public static readonly IReadOnlyList<RuntimeInitializeLoadType> PhaseOrder = Array.AsReadOnly(new[]
    {
        RuntimeInitializeLoadType.SubsystemRegistration,
        RuntimeInitializeLoadType.AfterAssembliesLoaded,
        RuntimeInitializeLoadType.BeforeSplashScreen,
        RuntimeInitializeLoadType.BeforeSceneLoad,
        RuntimeInitializeLoadType.AfterSceneLoad,
    });

    private const BindingFlags Static = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;

    private readonly IReadOnlyList<(RuntimeHook Hook, MethodInfo Method)> _resolved;

    private RuntimeHooks(IReadOnlyList<(RuntimeHook, MethodInfo)> resolved, IReadOnlyList<UnresolvedHook> unresolved)
    {
        _resolved = resolved;
        Unresolved = unresolved;
    }

    /// <summary>Hooks whose assembly, type or method could not be found or loaded.</summary>
    public IReadOnlyList<UnresolvedHook> Unresolved { get; }

    /// <summary>
    /// Resolves each hook's method. <paramref name="loadAssembly"/> returns the assembly with a
    /// given name, such as from the player's load context, and throws when it cannot.
    /// </summary>
    public static RuntimeHooks Resolve(IEnumerable<RuntimeHook> hooks, Func<string, Assembly> loadAssembly)
    {
        var resolved = new List<(RuntimeHook, MethodInfo)>();
        var unresolved = new List<UnresolvedHook>();
        foreach (var hook in hooks)
        {
            var (method, reason) = Find(hook, loadAssembly);
            if (method is not null) resolved.Add((hook, method));
            else unresolved.Add(new UnresolvedHook(hook, reason!));
        }
        return new RuntimeHooks(resolved, unresolved);
    }

    /// <summary>
    /// Runs every hook of <paramref name="phase"/> in listed order. Fails before running any
    /// when one of the phase's hooks is unresolved, and stops at the first hook that throws,
    /// naming it.
    /// </summary>
    public void Run(RuntimeInitializeLoadType phase)
    {
        if (!Enum.IsDefined(phase)) throw new ArgumentOutOfRangeException(nameof(phase), phase, "not a startup phase");
        var missing = Unresolved.Where(entry => entry.Hook.Phase == phase).ToList();
        if (missing.Count > 0)
            throw new InvalidOperationException(
                $"{phase} startup hooks cannot run: " + string.Join("; ", missing.Select(m => $"{m.Hook}: {m.Reason}")));
        foreach (var (hook, method) in _resolved.Where(entry => entry.Hook.Phase == phase))
        {
            try { method.Invoke(null, null); }
            catch (TargetInvocationException error) when (error.InnerException is not null)
            {
                throw new InvalidOperationException($"startup hook {hook} failed", error.InnerException);
            }
        }
    }

    private static (MethodInfo? Method, string? Reason) Find(RuntimeHook hook, Func<string, Assembly> loadAssembly)
    {
        try
        {
            var type = loadAssembly(hook.Assembly).GetType(hook.Type, throwOnError: false);
            if (type is null) return (null, $"no type {hook.Type} in {hook.Assembly}");
            var method = type.GetMethod(hook.Method, Static, Type.EmptyTypes);
            if (method is null) return (null, $"no static method {hook.Method}() on {hook.Type}");
            return method.ContainsGenericParameters
                ? (null, $"{hook.Type}.{hook.Method} is generic")
                : (method, null);
        }
        // Everything that loading an assembly, building its facades or looking up a type or
        // method throws for a name or file it cannot use.
        catch (Exception error) when (error is IOException or BadImageFormatException or TypeLoadException
            or AmbiguousMatchException or ArgumentException or NotSupportedException or InvalidDataException)
        {
            return (null, error.Message);
        }
    }
}
