// Owned synthetic palette: three rows per bone, addressed in float4 elements.
Buffer<float4> Bones : register(t0);
float4 main(uint vertex : SV_VertexID) : SV_Position {
    uint bone = vertex % 2;
    uint offset = 1;
    uint first = bone * 3 + offset;
    return Bones[first] + Bones[first + 1] * 2 + Bones[first + 2] * 4;
}
