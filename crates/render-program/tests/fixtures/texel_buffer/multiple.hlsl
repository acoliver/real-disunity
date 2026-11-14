// Shared scalar image type at two slots, plus a two-component image in another slot.
Buffer<float> First : register(t3, space2);
Buffer<float> Second : register(t7, space2);
Buffer<float2> Pair : register(t5, space2);
RWStructuredBuffer<float4> Output : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 thread : SV_DispatchThreadID) {
    Output[thread.x] = float4(First[thread.x] + Second[thread.x + 1], Pair[thread.x], 1);
}
