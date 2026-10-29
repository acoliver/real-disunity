using UnityEngine;
using UnityEngine.SceneManagement;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private sealed record PendingLoad(int Index, AsyncOperation Operation);

    private static readonly List<PendingLoad> PendingLoads = new();
    private static bool _pumpPosted;

    /// <summary>Asks for build scene <paramref name="index"/> to replace the loaded scene. The load takes effect in the
    /// delayed-tasks phase of the frame that is running or next runs, the engine's choice, so a script's call returns
    /// at once and its own frame finishes with the old scene. The returned operation is done when the load has happened.</summary>
    internal static AsyncOperation QueueSceneLoad(int index, LoadSceneMode mode)
    {
        var entry = BuildScenes[index];
        if (mode != LoadSceneMode.Single)
            throw new NotSupportedException($"SceneManager.LoadScene(\"{entry.Name}\", {mode}) is not supported yet: the engine loads one scene at a time");
        if (entry.ReadPlan is null)
            throw new InvalidOperationException($"scene {entry.Name} cannot load: {entry.PlanError ?? "it has no plan"}");
        var operation = new AsyncOperation();
        PendingLoads.Add(new PendingLoad(index, operation));
        PostPump();
        return operation;
    }

    private static void PostPump()
    {
        if (_pumpPosted || !PostToMainThread(PumpSceneLoads)) return;
        _pumpPosted = true;
    }

    /// <summary>Loads each requested scene whose operation allows activation, in request order. A held operation
    /// reports 0.9 and waits for the next frame.</summary>
    private static void PumpSceneLoads()
    {
        _pumpPosted = false;
        var due = PendingLoads.ToArray();
        PendingLoads.Clear();
        try
        {
            foreach (var load in due)
            {
                if (!load.Operation.allowSceneActivation)
                {
                    load.Operation.Report(0.9f);
                    PendingLoads.Add(load);
                    continue;
                }
                SwitchScene(load.Index);
                load.Operation.Complete();
            }
        }
        finally { if (PendingLoads.Count > 0) PostPump(); }
    }

    /// <summary>Replaces the loaded scene with build scene <paramref name="index"/>: tells scripts the old scene is
    /// going, destroys its roots except those marked DontDestroyOnLoad (through the ordinary destroy path, so
    /// OnDisable and OnDestroy run), then loads the new plan, which wakes its scripts and raises the load events.</summary>
    private static void SwitchScene(int index)
    {
        var entry = BuildScenes[index];
        var options = _sceneOptions ?? throw new InvalidOperationException("the world has no scene load options");
        var old = CurrentScene;
        _unloadingScene = old;
        try
        {
            if (old is not null)
            {
                SceneManager.RaiseUnloaded(ActiveScene);
                foreach (var root in SceneRoots()) root.Destroy();
                old.Loaded?.Dispose();
            }
            LoadPlan(entry.ReadPlan!(), options, index);
        }
        finally { _unloadingScene = null; }
    }
}
