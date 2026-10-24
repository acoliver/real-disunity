using CompatibilityBridge;
using UnityEngine;

internal static class ContextualMetadataBehavior
{
    internal static void Run()
    {
        BinderBehavior.Check(ContextualMetadata.FindCallback(typeof(Callable), "OnDestroy") is not null,
            "closed parameterless void callback is available");
        BinderBehavior.Check(ContextualMetadata.FindCallback(typeof(Callable), "OnDisable") is null,
            "absent callback remains absent");
        Reject(typeof(Generic));
        Reject(typeof(ReturnsValue));
        Console.WriteLine("PASS contextual metadata: callable and absent mappings, generic and non-void rejection");
    }
    private static void Reject(Type type)
    {
        try { ContextualMetadata.FindCallback(type, "OnDestroy"); }
        catch (InvalidDataException error) when (error.Message.Contains("closed parameterless void instance method")) { return; }
        throw new Exception("unsupported contextual callback metadata accepted: " + type.FullName);
    }
    private sealed class Callable : MonoBehaviour
    {
        private void OnDestroy() { }
    }
    private sealed class Generic : MonoBehaviour
    {
        private void OnDestroy<T>() { }
    }
    private sealed class ReturnsValue : MonoBehaviour
    {
        private int OnDestroy() => 1;
    }
}
