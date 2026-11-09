using System.Collections.Concurrent;
using System.Reflection;
using System.Reflection.Emit;

namespace Disunity.Hosting;

/// <summary>
/// Keeps the delegates a game hands to native code alive for the life of the process. A game passes
/// <c>new Callback(StaticMethod)</c> to a native function that remembers the function pointer; its own runtime
/// keeps that pointer callable after the delegate object is unreachable, while this host's runtime ends the
/// process when native code calls a pointer whose delegate has been collected. <see cref="PlayerCallbackRewriter"/>
/// replaces such delegate constructions in a player's assemblies with calls to <see cref="Keep{TDelegate}"/>.
/// </summary>
public static class NativeCallbacks
{
    private static readonly ConcurrentDictionary<(Type Delegate, IntPtr Method), Delegate> Kept = new();
    private static readonly ConcurrentDictionary<Type, Func<object?, IntPtr, Delegate>> Constructors = new();

    /// <summary>
    /// The delegate of type <typeparamref name="TDelegate"/> that calls <paramref name="method"/>, a function pointer
    /// such as <c>ldftn</c> pushes. With a null <paramref name="target"/> (a static method) the same instance returns
    /// for each type and method pair, so it is never collected and its native thunk stays valid. A delegate with a
    /// target is new on each call, as the delegate's constructor would make it: the target would stay alive too.
    /// </summary>
    public static TDelegate Keep<TDelegate>(object? target, IntPtr method) where TDelegate : Delegate
    {
        var create = Constructors.GetOrAdd(typeof(TDelegate), Constructor);
        if (target is not null) return (TDelegate)create(target, method);
        return (TDelegate)Kept.GetOrAdd((typeof(TDelegate), method), key => create(null, key.Method));
    }

    /// <summary>
    /// Wraps the delegate type's <c>(object, native int)</c> constructor, the one <c>newobj</c> calls after
    /// <c>ldftn</c>, in a small dynamic method that takes the target and function pointer as arguments.
    /// </summary>
    private static Func<object?, IntPtr, Delegate> Constructor(Type delegateType)
    {
        var constructor = delegateType.GetConstructor(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
            [typeof(object), typeof(IntPtr)])
            ?? throw new ArgumentException($"{delegateType} has no (object, native int) constructor", nameof(delegateType));
        var create = new DynamicMethod("Create", typeof(Delegate), [typeof(object), typeof(IntPtr)], typeof(NativeCallbacks).Module, skipVisibility: true);
        var il = create.GetILGenerator();
        il.Emit(OpCodes.Ldarg_0);
        il.Emit(OpCodes.Ldarg_1);
        il.Emit(OpCodes.Newobj, constructor);
        il.Emit(OpCodes.Ret);
        return (Func<object?, IntPtr, Delegate>)create.CreateDelegate(typeof(Func<object?, IntPtr, Delegate>));
    }
}
