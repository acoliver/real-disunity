using UnityEngine;
using Disunity.Hosting;

namespace PortableFixture;

/// <summary>External-assembly lifecycle fixture with one command from each tick phase. Its failing
/// Update throws a native error, which fails the step; an exception of the game's own is logged.</summary>
public sealed class TriangleBehaviour(int index) : MonoBehaviour
{
    public static int Destroyed { get; private set; }
    public static bool FailUpdates { get; set; }
    public static bool LongFailure { get; set; }
    private FixtureAsset? _asset;
    private void Awake() => _asset = ScriptableObject.CreateInstance<FixtureAsset>();

    public int Starts { get; private set; }
    public int Updates { get; private set; }
    public int LateUpdates { get; private set; }

    private void Start()
    {
        Starts++;
        gameObject.Transform.position = new Vector3(index * 2.0f, 1.25f, -index * 3.0f);
    }

    private void Update()
    {
        if (FailUpdates) throw new DisunityException(1, DisunityErrorReason.CallbackFailed, "managed callback failure λ" + (LongFailure ? new string('界', 70000) : ""));
        if (Updates != 0) UnityEngine.Object.Destroy(_asset);
        Updates++;
        gameObject.Transform.rotationY = index * 0.25f;
    }

    private void LateUpdate()
    {
        LateUpdates++;
        gameObject.Transform.materialSlot = index;
    }
    private void OnDestroy()
    {
        _ = Time.Frame;
        var probe = DisunityHost.CreateGameObject();
        probe.Transform.position = new Vector3(8, 13, 21);
        Destroyed++;
    }
}

public sealed class FixtureAsset : ScriptableObject
{
    public static int Destroyed { get; private set; }
    private void OnDestroy() => Destroyed++;
}
