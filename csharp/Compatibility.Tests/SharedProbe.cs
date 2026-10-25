using System.Collections;
using UnityEngine;

public sealed class SharedProbe : MonoBehaviour
{
    public int awakeCount, updates, resumed;
    public string initialName = "", rustName = "";
    public ulong reverse;
    public bool held, nativeHeld;
    public float delta, nativeDelta;

    private void Awake()
    {
        awakeCount++;
        initialName = gameObject.name;
        gameObject.name = "clr-mutated-rust-object-7291";
        var created = new GameObject("clr-created-object-4837");
        reverse = created.NativeHandle;
    }
    private IEnumerator Start() { yield return null; resumed++; }
    private void Update()
    {
        updates++;
        rustName = gameObject.name;
        held = Input.GetKey(KeyCode.W);
        delta = Time.deltaTime;
        using var input = NativeWorld.Request("input.get", ("phase", "dynamic"));
        nativeHeld = NativeWorld.Data(input).GetProperty("keys").EnumerateArray().Any(key => key.GetString() == "W");
        using var time = NativeWorld.Request("time.get");
        nativeDelta = NativeWorld.Data(time).GetProperty("delta_time").GetSingle();
        transform.position = new Vector3(17, 23, 31);
    }
}
