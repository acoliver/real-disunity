using System.Numerics;
using UnityEngine;

namespace CompatibilityBridge;

internal static class NativeWait
{
    internal static object Describe(object? value, Func<Coroutine, ulong> routine) => value switch
    {
        null => new { kind = "next" },
        WaitForSeconds scaled => new { kind = "scaled", seconds = Ratio(scaled.Seconds) },
        WaitForSecondsRealtime unscaled => new { kind = "unscaled", seconds = Ratio(unscaled.Seconds) },
        WaitForFixedUpdate => new { kind = "fixed" },
        WaitForEndOfFrame => new { kind = "end_frame" },
        Coroutine join => new { kind = "join", routine = routine(join) },
        _ => throw new NotSupportedException("native_readiness_single_v1 does not support nested iterators, predicates or custom yields: " + value.GetType().FullName)
    };
    private static object Ratio(float seconds)
    {
        var bits = BitConverter.SingleToUInt32Bits(seconds);
        var exponent = (int)((bits >> 23) & 255);
        var significand = bits & 0x7fffff;
        if (exponent != 0) significand |= 0x800000;
        var power = exponent == 0 ? -149 : exponent - 150;
        var num = new BigInteger(significand);
        var den = BigInteger.One;
        if (power < 0) den <<= -power;
        else num <<= power;
        var divisor = BigInteger.GreatestCommonDivisor(num, den);
        num /= divisor;
        den /= divisor;
        if (num > 9007199254740991UL || den > 1000000000000UL)
            throw new NotSupportedException("native_readiness_single_v1 wait float exceeds exact native ratio bounds");
        return new { num = (ulong)num, den = (ulong)den };
    }
}
