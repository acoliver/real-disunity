using Disunity;

namespace PortableFixture;

/// <summary>External-assembly lifecycle fixture with one command from each tick phase.</summary>
public sealed class TriangleBehaviour(int index) : MonoBehaviour
{
    public int Starts { get; private set; }
    public int Updates { get; private set; }
    public int LateUpdates { get; private set; }

    protected override void Start()
    {
        Starts++;
        gameObject.Transform.position = new Vector3(index * 2.0f, 1.25f, -index * 3.0f);
    }

    protected override void Update()
    {
        Updates++;
        gameObject.Transform.rotationY = index * 0.25f;
    }

    protected override void LateUpdate()
    {
        LateUpdates++;
        gameObject.Transform.materialSlot = index;
    }
}
