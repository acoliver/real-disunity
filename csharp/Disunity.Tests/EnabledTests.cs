using Disunity.Hosting;
using UnityEngine;

internal static class EnabledTests
{
    private sealed class Probe : MonoBehaviour
    {
        public int Starts, Updates, Enables, Disables;
        private void Start() => Starts++;
        private void Update() => Updates++;
        private void OnEnable() => Enables++;
        private void OnDisable() => Disables++;
    }
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var probe = DisunityHost.AddComponent(new Probe());
        DisunityHost.Step(0.01f);
        probe.enabled = false;
        if (probe.enabled || probe.Disables != 1) throw new Exception("native disable must drain before returning");
        DisunityHost.Step(0.01f);
        if (probe.Updates != 1) throw new Exception("disabled script received Update");
        probe.enabled = true;
        DisunityHost.Step(0.01f);
        if (!probe.enabled || probe.Enables != 2 || probe.Starts != 1 || probe.Updates != 2) throw new Exception("native enabled eligibility");
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: native script enabled state and exactly-once Start");
    }
}
