namespace AOT;

/// <summary>
/// Marks a static method that native code calls back into, naming the delegate type it is
/// called through. Packages read it when they build function pointers; the engine only needs
/// the attribute to exist.
/// </summary>
[AttributeUsage(AttributeTargets.Method)]
public sealed class MonoPInvokeCallbackAttribute(Type type) : Attribute
{
    public Type type { get; } = type;
}
