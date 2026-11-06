using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>ScriptableRenderContext members that need no record in the world.</summary>
internal static class ContextEmitTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
            CommandWireReader.Take();
            var context = default(ScriptableRenderContext);
            ScriptableRenderContext.EmitGeometryForCamera(camera);
            context.Submit();
            Expect.Equal(0, CommandWireReader.Take().Length, "emitting canvas geometry records nothing");
            Expect.Throws<ArgumentNullException>(() => ScriptableRenderContext.EmitGeometryForCamera(null!), "a null camera");
            Console.WriteLine("PASS: EmitGeometryForCamera records nothing and refuses a null camera");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }
}
