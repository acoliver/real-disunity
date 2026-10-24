using System.Text.Json;

namespace UnityEngine;

public class Transform : Component
{
    public Vector3 position { get => ReadVector("position"); set => SetVector("position", value, "world"); }
    public Vector3 localPosition { get => ReadVector("local_position"); set => SetVector("position", value, "local"); }
    public Vector3 localScale { get => ReadVector("local_scale"); set => SetVector("scale", value, "local"); }
    public Quaternion rotation { get => ReadRotation("rotation"); set => SetRotation(value, "world"); }
    public Quaternion localRotation { get => ReadRotation("local_rotation"); set => SetRotation(value, "local"); }
    public Transform? parent
    {
        get { var p = Snapshot().GetProperty("parent"); return p.ValueKind == JsonValueKind.Null ? null : NativeIdentity.GameObject(p.GetUInt64()).transform; }
        set => SetParent(value);
    }
    public int childCount => Snapshot().GetProperty("children").GetArrayLength();
    public Transform GetChild(int index) => NativeIdentity.GameObject(Snapshot().GetProperty("children")[index].GetUInt64()).transform;
    public void SetParent(Transform? value, bool worldPositionStays = true)
    {
        NativeWorld.Request("transform.parent", ("object", OwnerHandle), ("parent", value?.OwnerHandle), ("world_position_stays", worldPositionStays)).Dispose();
        NativeIdentity.Runtime?.Reconcile();
    }
    private JsonElement Snapshot() { using var result = NativeWorld.Request("transform.get", ("object", OwnerHandle)); return NativeWorld.Data(result).Clone(); }
    private Vector3 ReadVector(string field) { var v = Snapshot().GetProperty(field); return new(v[0].GetSingle(), v[1].GetSingle(), v[2].GetSingle()); }
    private Quaternion ReadRotation(string field) { var v = Snapshot().GetProperty(field); return new(v[0].GetSingle(), v[1].GetSingle(), v[2].GetSingle(), v[3].GetSingle()); }
    private void SetVector(string field, Vector3 value, string space) => NativeWorld.Request("transform.set", ("object", OwnerHandle), (field, new[] { value.x, value.y, value.z }), ("space", space)).Dispose();
    private void SetRotation(Quaternion value, string space) => NativeWorld.Request("transform.set", ("object", OwnerHandle), ("rotation", new[] { value.x, value.y, value.z, value.w }), ("space", space)).Dispose();
}
