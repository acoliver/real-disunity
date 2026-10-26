namespace UnityEngine;

public struct LayerMask
{
    public int value { get; set; }
    public static implicit operator int(LayerMask mask) => mask.value;
    public static implicit operator LayerMask(int value) => new() { value = value };
}
