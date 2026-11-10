using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>
/// The pipeline asks the context whether any script wants OnRenderObject, and then invokes those scripts.
/// </summary>
internal static class RenderObjectCallbackTests
{
    private sealed class Drawn : MonoBehaviour
    {
        public int calls;
        private void OnRenderObject() => calls++;
    }

    private sealed class Plain : MonoBehaviour
    {
        public int updates;
        private void Update() => updates++;
    }

    private class DrawnBase : MonoBehaviour
    {
        public int calls;
        protected void OnRenderObject() => calls++;
    }

    private sealed class DrawnDerived : DrawnBase { }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        var context = new ScriptableRenderContext();
        DisunityHost.CreateGameObject().AddComponent<Plain>();
        Expect.That(!context.HasInvokeOnRenderObjectCallbacks(), "a script without OnRenderObject asks for no callback");
        var drawn = DisunityHost.CreateGameObject().AddComponent<Drawn>();
        Expect.That(context.HasInvokeOnRenderObjectCallbacks(), "an enabled script with OnRenderObject asks for the callback");
        context.InvokeOnRenderObjectCallback();
        Expect.That(drawn.calls == 1, $"the callback runs the script's OnRenderObject once: {drawn.calls}");
        drawn.enabled = false;
        Expect.That(!context.HasInvokeOnRenderObjectCallbacks(), "a disabled script asks for no callback");
        context.InvokeOnRenderObjectCallback();
        Expect.That(drawn.calls == 1, "a disabled script is not called");
        drawn.enabled = true;
        drawn.gameObject.SetActive(false);
        Expect.That(!context.HasInvokeOnRenderObjectCallbacks(), "a script on an inactive object asks for no callback");
        drawn.gameObject.SetActive(true);
        var derived = DisunityHost.CreateGameObject().AddComponent<DrawnDerived>();
        context.InvokeOnRenderObjectCallback();
        Expect.That(drawn.calls == 2 && derived.calls == 1, $"a base class's OnRenderObject counts: {drawn.calls} {derived.calls}");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: the context reports and invokes the OnRenderObject callbacks of active, enabled scripts");
    }
}
