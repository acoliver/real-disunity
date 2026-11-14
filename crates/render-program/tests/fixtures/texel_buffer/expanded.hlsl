// The regeneration script exposes the raw vec4 returned by OpImageFetch so the
// test can observe Vulkan's missing-component values, which HLSL otherwise discards.
#ifndef ELEMENT
#define ELEMENT float
#define VECTOR float4
#define COMPONENTS 1
#endif
Buffer<ELEMENT> Input : register(t0);
RWStructuredBuffer<VECTOR> Output : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 thread : SV_DispatchThreadID) {
#if COMPONENTS == 1
    Output[thread.x] = VECTOR(Input[thread.x + 1], 0, 0, 1);
#else
    Output[thread.x] = VECTOR(Input[thread.x + 1], 0, 1);
#endif
}
