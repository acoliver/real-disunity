using UnityEngine;
using UnityEngine.SceneManagement;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private sealed record PendingLoad(int Index, LoadSceneMode Mode, AsyncOperation Operation);

    private static readonly List<PendingLoad> PendingLoads = new();
    private static bool _pumpPosted;

    /// <summary>Asks for build scene <paramref name="index"/> to load: in <see cref="LoadSceneMode.Single"/> mode it replaces
    /// every loaded scene, in <see cref="LoadSceneMode.Additive"/> mode it loads beside them and the active scene stays.
    /// The load takes effect in the delayed-tasks phase of the frame that is running or next runs, the engine's choice,
    /// so a script's call returns at once and its own frame finishes with the scenes it had. The returned operation is
    /// done when the load has happened.</summary>
    internal static AsyncOperation QueueSceneLoad(int index, LoadSceneMode mode)
    {
        var entry = BuildScenes[index];
        if (mode is not (LoadSceneMode.Single or LoadSceneMode.Additive))
            throw new ArgumentOutOfRangeException(nameof(mode), mode, $"scene {entry.Name}: {(int)mode} is not a LoadSceneMode");
        if (entry.ReadPlan is null)
            throw new InvalidOperationException($"scene {entry.Name} cannot load: {entry.PlanError ?? "it has no plan"}");
        var operation = new AsyncOperation();
        PendingLoads.Add(new PendingLoad(index, mode, operation));
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
                SwitchScene(load.Index, load.Mode);
                load.Operation.Complete();
            }
        }
        finally { if (PendingLoads.Count > 0) PostPump(); }
    }

    /// <summary>Loads build scene <paramref name="index"/>. A <see cref="LoadSceneMode.Single"/> load first tells scripts
    /// each loaded scene is going and destroys its roots except those marked DontDestroyOnLoad (through the ordinary
    /// destroy path, so OnDisable and OnDestroy run). The new plan then loads, which wakes its scripts and raises the
    /// load events.</summary>
    private static void SwitchScene(int index, LoadSceneMode mode)
    {
        var entry = BuildScenes[index];
        var options = _sceneOptions ?? throw new InvalidOperationException("the world has no scene load options");
        if (mode == LoadSceneMode.Additive)
        {
            LoadPlan(entry.ReadPlan!(), options, index, mode);
            return;
        }
        var previous = ActiveScene;
        UnloadingScenes.AddRange(LoadedScenes);
        try
        {
            UnloadAllScenes();
            LoadPlan(entry.ReadPlan!(), options, index, mode, previous);
        }
        finally { UnloadingScenes.Clear(); }
    }

    /// <summary>Tells scripts each loaded scene is going and destroys its roots. The scenes stay nameable through
    /// <see cref="UnloadingScenes"/> until the caller's load is done.</summary>
    private static void UnloadAllScenes()
    {
        var leaving = LoadedScenes.ToArray();
        foreach (var scene in leaving)
        {
            SceneManager.RaiseUnloaded(new Scene { Handle = scene.Handle });
            foreach (var root in RootsOf(scene)) root.Destroy();
        }
        foreach (var scene in leaving) scene.Loaded?.Dispose();
        LoadedScenes.Clear();
        CurrentScene = null;
    }
}
