using System.Text;

namespace Unity.Baselib.LowLevel;

/// <summary>
/// Baselib's error explanation, which Unity Transport logs when a socket call fails, for example
/// when another process holds the port a server binds. The text names the error code and, at the
/// fuller verbosity, the platform's error number.
/// </summary>
public static unsafe partial class Binding
{
    /// <summary>Writes the explanation as UTF-8 with a terminating zero, cut to fit
    /// <paramref name="bufferLen"/>, and returns the length the whole explanation needs with its
    /// terminating zero. A null buffer or a zero length writes nothing.</summary>
    public static uint Baselib_ErrorState_Explain(Baselib_ErrorState* errorState, byte* buffer, uint bufferLen,
        Baselib_ErrorState_ExplainVerbosity verbosity)
    {
        var text = Encoding.UTF8.GetBytes(Explanation(errorState, verbosity));
        if (buffer != null && bufferLen > 0)
        {
            var count = (int)Math.Min((uint)text.Length, bufferLen - 1);
            text.AsSpan(0, count).CopyTo(new Span<byte>(buffer, count));
            buffer[count] = 0;
        }
        return (uint)text.Length + 1;
    }

    private static string Explanation(Baselib_ErrorState* errorState, Baselib_ErrorState_ExplainVerbosity verbosity)
    {
        if (errorState == null) return "no error state";
        var state = *errorState;
        var code = Enum.IsDefined(state.code) ? state.code.ToString() : $"0x{(uint)state.code:X8}";
        var native = state.nativeErrorCodeType != Baselib_ErrorState_NativeErrorCodeType.None;
        return verbosity == Baselib_ErrorState_ExplainVerbosity.ErrorType || !native
            ? code
            : $"{code} (platform error {state.nativeErrorCode})";
    }
}
