#ifndef ELEMENT
#define ELEMENT float
#endif
Buffer<ELEMENT> Input : register(t0);
RWStructuredBuffer<ELEMENT> Output : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 thread : SV_DispatchThreadID) {
    Output[thread.x] = Input[thread.x + 1];
}
