#if defined(WRITE)
RWBuffer<float4> Input : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 thread : SV_DispatchThreadID) {
    Input[thread.x] = float4(1, 2, 3, 4);
}
#else
Buffer<float4> Input : register(t0);
RWStructuredBuffer<uint> Output : register(u0);
[numthreads(1, 1, 1)]
void main() {
    uint count;
    Input.GetDimensions(count);
    Output[0] = count;
}
#endif
