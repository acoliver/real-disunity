using Disunity.Hosting;
using UnityEngine.SceneManagement;

/// <summary>SceneHandle, the opaque value Scene.handle returns.</summary>
internal static class SceneHandleTests
{
    public static void Run()
    {
        ValueSemantics();
        Console.WriteLine("PASS: SceneHandle compares by its raw value, orders by it and round-trips through its raw data");
        HandlesOfScenes();
        Console.WriteLine("PASS: Scene.handle is a SceneHandle that is equal for one scene and differs between two scenes");
    }

    private static void ValueSemantics()
    {
        var one = SceneHandle.FromRawData(7);
        var same = SceneHandle.FromRawData(7);
        var other = SceneHandle.FromRawData(9);
        Expect.That(one == same && one.Equals(same) && one.Equals((object)same) && !(one != same), "equal raw data is equal");
        Expect.That(one != other && !one.Equals(other) && !one.Equals("7") && !one.Equals((object?)null), "different raw data differs");
        Expect.Equal(one.GetHashCode(), same.GetHashCode(), "equal handles hash alike");
        Expect.Equal(7UL, one.GetRawData(), "the raw data reads back");
        Expect.Equal("7", one.ToString(), "the text is the raw value");
        Expect.That(one.CompareTo(other) < 0 && other.CompareTo(one) > 0 && one.CompareTo(same) == 0, "handles order by raw value");
        Expect.Equal(0UL, default(SceneHandle).GetRawData(), "the default handle is raw 0");
        int number = one;
        Expect.That(number == 7 && (int)other == 9 && (int)default(SceneHandle) == 0, "a handle converts implicitly to the scene id it wraps");
    }

    private static string Plan(string name) =>
        "{\"scene\":\"" + name + "\",\"assets\":[],\"sprites\":[],\"textures\":[],\"fonts\":[],\"axes\":[],\"differences\":[],\"objects\":[]}";

    private static void HandlesOfScenes()
    {
        Expect.Equal(default(SceneHandle), default(Scene).handle, "the invalid scene has the default handle");
        var options = new SceneLoadOptions(typeof(SceneHandleTests).Assembly, _ => []);
        BuildScene[] build =
        [
            new("First", "Assets/Scenes/First.unity", () => Plan("level0")),
            new("Second", "Assets/Scenes/Second.unity", () => Plan("level1")),
        ];
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.SetBuildScenes(build, options);
            using var loaded = DisunityHost.LoadScene(Plan("level0"), options);
            SceneManager.LoadScene("Second", LoadSceneMode.Additive);
            DisunityHost.Step(0.01f);
            var (first, second) = (SceneManager.GetSceneAt(0), SceneManager.GetSceneAt(1));
            Expect.That(first.handle != default && second.handle != default, "a loaded scene has a non-default handle");
            Expect.That(first.handle != second.handle, "two scenes have different handles");
            Expect.That((int)first.handle != 0 && (int)second.handle != 0 && (int)first.handle != (int)second.handle, "the scene ids are different non-zero numbers");
            Expect.That(first.handle == SceneManager.GetSceneAt(0).handle, "a scene found again has the same handle");
            Expect.That(SceneManager.SetActiveScene(second), "the second scene can be made active");
            Expect.That(SceneManager.GetActiveScene().handle == second.handle, "the active scene's handle is the second scene's");
        }
        finally
        {
            DisunityHost.UnloadWorld();
            SceneManager.ClearHandlers();
        }
    }
}
