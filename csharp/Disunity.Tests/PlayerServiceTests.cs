using Disunity.Hosting;
using UnityEngine;
using UnityEngine.CrashReportHandler;
using UnityEngine.Profiling;

/// <summary>Crash report metadata, managed memory figures and the audio types a player's startup and tweens refer to.</summary>
internal static class PlayerServiceTests
{
    public static void Run()
    {
        CrashMetadata();
        ProfilerMemory();
        AudioTypesLoadAndRefuse();
        ModuleFacadesForwardTheTypes();
        Console.WriteLine("PASS: CrashReportHandler metadata, Profiler memory figures and the unsupported audio types");
    }

    private static void CrashMetadata()
    {
        CrashReportHandler.SetUserMetadata("build", "42");
        Expect.Equal("42", CrashReportHandler.GetUserMetadata("build"), "a stored value reads back");
        CrashReportHandler.SetUserMetadata("build", "43");
        Expect.Equal("43", CrashReportHandler.GetUserMetadata("build"), "a second set replaces the value");
        CrashReportHandler.SetUserMetadata("build", null);
        Expect.That(CrashReportHandler.GetUserMetadata("build") is null, "a null value removes the key");
        CrashReportHandler.SetUserMetadata("never-set", null);
        Expect.That(CrashReportHandler.GetUserMetadata("missing") is null, "an unset key reads null");
        Expect.Throws<ArgumentNullException>(() => CrashReportHandler.SetUserMetadata(null!, "x"), "a null key is rejected");
        Expect.Throws<ArgumentNullException>(() => CrashReportHandler.GetUserMetadata(null!), "a null key is rejected on read");
    }

    private static void ProfilerMemory()
    {
        GC.Collect();
        var heap = Profiler.GetMonoHeapSizeLong();
        var used = Profiler.GetMonoUsedSizeLong();
        Expect.That(heap > 0 && used > 0, "the managed heap has a size and is in use");
        Expect.That(used <= heap, $"used {used} fits in the heap {heap}");
        var block = new byte[16 * 1024 * 1024];
        Expect.That(Profiler.usedHeapSizeLong >= used + block.Length / 2, "allocating raises the used figure");
        GC.KeepAlive(block);
    }

    private static void AudioTypesLoadAndRefuse()
    {
        DisunityHost.LoadWorld();
        try
        {
            var go = DisunityHost.CreateGameObject();
            var error = Refusal.Of(() => go.AddComponent<AudioSource>());
            Expect.That(error.Message == "AudioSource components are not supported yet: the engine has no audio output", $"the refusal names AudioSource and the reason: {error.Message}");
            Expect.That(go.GetComponent<AudioSource>() == null, "the AudioSource stays off the GameObject");
            Expect.That(go.AddComponent<AudioListener>().enabled, "other audio components still attach");
        }
        finally { DisunityHost.UnloadWorld(); }
        var source = new AudioSource();
        Expect.That(source.enabled, "an AudioSource built with new is enabled");
        Refuses("AudioSource.Play", source.Play);
        Refuses("AudioSource.Pause", source.Pause);
        Refuses("AudioSource.UnPause", source.UnPause);
        Refuses("AudioSource.Stop", source.Stop);
        Refuses("AudioSource.isPlaying", () => _ = source.isPlaying);
        Refuses("AudioSource.volume", () => source.volume = 1f);
        Refuses("AudioSource.minDistance", () => source.minDistance = 1f);
        Refuses("AudioSource.clip", () => source.clip = null!);
        Refuses("AudioSource.PlayClipAtPoint", () => AudioSource.PlayClipAtPoint(null!, Vector3.zero, 1f));
        Refuses("AudioClip.Create", () => AudioClip.Create("clip", 1, 1, 44100, false));
        Refuses("AudioClip.Create", () => AudioClip.Create("clip", 1, 1, 44100, true, _ => { }, _ => { }));
    }

    private static void Refuses(string member, Action call)
    {
        var error = Refusal.Of(call);
        Expect.That(error.Message.StartsWith(member + ":", StringComparison.Ordinal), $"{member} refusal names the member: {error.Message}");
    }

    private static void ModuleFacadesForwardTheTypes()
    {
        var engine = typeof(DisunityHost).Assembly;
        foreach (var name in new[]
        {
            "UnityEngine.CrashReportHandler.CrashReportHandler", "UnityEngine.Profiling.Profiler", "UnityEngine.AudioSource",
            "UnityEngine.AudioClip", "UnityEngine.AudioClip+PCMReaderCallback", "UnityEngine.AudioClip+PCMSetPositionCallback",
        })
            Expect.That(ModuleFacades.Forwards(engine, name), $"a module facade forwards {name}");
    }
}
