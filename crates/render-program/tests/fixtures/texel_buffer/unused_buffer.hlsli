// Original fixture shared header. The pixel stage does not use this resource.
#ifdef STORAGE_BUFFER
RWBuffer<float4> unused_values : register(u0);
#elif !defined(NO_BUFFER)
Buffer<float4> unused_values : register(t0);
#endif
