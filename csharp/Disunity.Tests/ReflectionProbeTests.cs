using Disunity.Hosting;
using UnityEngine;

/// <summary>The reflection probe type exists for its defaults; a probe cannot join a world.</summary>
internal static class ReflectionProbeTests
{
    public static void Run()
    {
        Expect.That(ReflectionProbe.defaultTexture == null, "there is no default reflection texture");
        Expect.Equal(new Vector4(1f, 1f, 0f, 0f), ReflectionProbe.defaultTextureHDRDecodeValues, "the decode values of a stored linear cubemap");
        Expect.That(typeof(ReflectionProbe).BaseType == typeof(Behaviour), "a reflection probe is a Behaviour");
        DisunityHost.LoadWorld();
        try
        {
            var error = Expect.Catch<NotSupportedException>(() => DisunityHost.CreateGameObject().AddComponent<ReflectionProbe>(), "AddComponent");
            Expect.That(error.Message.Contains("ReflectionProbe"), error.Message);
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: ReflectionProbe reports its defaults and cannot be added to a GameObject");
    }
}
