using CompatibilityBridge;
using UnityEngine;
using UnityEngine.Events;
using UObject = UnityEngine.Object;

internal static class CloneBehavior
{
    public static void Run()
    {
        var root = new GameObject("template-root"); root.SetActive(false);
        var child = new GameObject("template-child"); child.transform.SetParent(root.transform, false);
        var witness = new GameObject("clone-witness");
        var persistent = new { m_PersistentCalls = new { m_Calls = new[] { new { m_Target = TestGraph.Ref("child-script"), m_MethodName = "Mark", m_Mode = 1, m_CallState = 2, m_Arguments = new { } } } } };
        using var payload = TestGraph.Payload([
            new("root-script", root, "CloneProbe", new { link = TestGraph.Ref("child"), partner = TestGraph.Ref("child-script"), refs = new[] { TestGraph.Ref("child") }, witness = TestGraph.Ref("witness"), signal = persistent }),
            new("child-script", child, "CloneProbe", new { link = TestGraph.Ref("root"), partner = TestGraph.Ref("root-script"), refs = new[] { TestGraph.Ref("root") }, witness = TestGraph.Ref("witness") })
        ], new() { ["root"] = root, ["child"] = child, ["witness"] = witness }, [root.NativeHandle]);
        using var session = new ScriptSession(payload.RootElement); session.Initialize();
        BinderBehavior.Check(child.transform.localPosition.y == 0, "inactive template cannot dispatch Awake/OnEnable");
        var clone = UObject.Instantiate(root);
        var cloneChild = clone.transform.GetChild(0).gameObject;
        BinderBehavior.Check(clone.activeInHierarchy, "template clone restores authored active state");
        BinderBehavior.Check(cloneChild.transform.localPosition.y == 1 && child.transform.localPosition.y == 0, "cloned serialized event targets clone sibling, leaving original untouched");
        var scripts = clone.GetComponents<MonoBehaviour>();
        BinderBehavior.Check(scripts.Length == 1 && cloneChild.GetComponents<MonoBehaviour>().Length == 1, "native graph clone returns exactly one cached script per native component");
        UObject.Destroy(clone);
        BinderBehavior.Check(witness.transform.position.z == 2, "recursive clone teardown invokes OnDestroy once for each active script");
        BinderBehavior.Check(root.IsAlive && child.IsAlive, "cloned graph destruction preserves inactive original templates");
        Console.WriteLine("PASS clone graph: internal object/component/list/event remap, external sharing, authored activation, inactive templates and recursive OnDestroy");
    }
}
public class CloneProbe : MonoBehaviour
{
    public GameObject link = null!;
    public CloneProbe partner = null!;
    public List<GameObject> refs = new();
    public GameObject witness = null!;
    public UnityEvent signal = new();
    private void Awake()
    {
        if (!ReferenceEquals(partner, link.GetComponent<CloneProbe>()) || !ReferenceEquals(refs[0], link)) throw new Exception("clone serialized internal reference was not remapped before Awake");
    }
    private void OnEnable() => signal.Invoke();
    public void Mark() { var p = transform.localPosition; p.y++; transform.localPosition = p; }
    private void OnDestroy() { var p = witness.transform.position; p.z++; witness.transform.position = p; }
}
