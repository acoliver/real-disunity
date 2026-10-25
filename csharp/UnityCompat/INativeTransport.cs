namespace UnityEngine;

internal unsafe interface INativeTransport
{
    DisunityResult Request(ulong world, byte* bytes, nuint length, ulong* response);
    DisunityResult Copy(ulong response, byte* bytes, nuint capacity, nuint* length);
    DisunityResult Free(ulong response);
}
