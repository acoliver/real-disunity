#include "unused_buffer.hlsli"

float4 main(float4 position : SV_Position) : SV_Target {
    return float4(position.xy, 0.0, 1.0);
}
