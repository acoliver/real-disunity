using System.Text.Json;

namespace UnityEngine;

public static class Time
{
    private static double Read(string key)
    {
        using var response = NativeWorld.Request("time.get");
        var data = NativeWorld.Data(response);
        if (!data.TryGetProperty(key, out var value) || !value.TryGetDouble(out var number))
            throw new NativeWorldException("invalid_response", $"time.get did not return numeric field '{key}'");
        return number;
    }
    public static float deltaTime => (float)Read("delta_time");
    public static float fixedDeltaTime
    {
        get => (float)Read("fixed_delta_time");
        set => Set("fixed_delta_time", value);
    }
    public static float time => (float)Read("time");
    public static float unscaledTime => (float)Read("unscaled_time");
    public static float unscaledDeltaTime => (float)Read("unscaled_delta_time");
    public static float fixedUnscaledDeltaTime => (float)Read("fixed_unscaled_delta_time");
    private static void Set(string field, float value)
    {
        if (NativePhaseScope.Strict) throw new NotSupportedException("strict Time configuration requires authenticated host session.configure");
        NativeWorld.Request("time.set", (field, value)).Dispose();
    }
    public static float timeScale
    {
        get => (float)Read("time_scale");
        set => Set("time_scale", value);
    }
}

public enum KeyCode
{
    None, Space, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    UpArrow, DownArrow, LeftArrow, RightArrow, Return, Escape
}

public static class Input
{
    public static bool GetKey(KeyCode key) => ReadKeys("keys").Contains(key.ToString());
    public static bool GetKeyDown(KeyCode key) => ReadKeys("down").Contains(key.ToString());
    public static bool GetKeyUp(KeyCode key) => ReadKeys("up").Contains(key.ToString());
    public static float GetAxisRaw(string name) => GetAxis(name);
    public static float GetAxis(string name)
    {
        using var response = NativePhaseScope.Input();
        var axes = NativeWorld.Data(response).GetProperty("axes");
        if (!axes.TryGetProperty(name, out var axis)) throw new NotSupportedException($"Input.GetAxis('{name}')");
        return axis.GetSingle();
    }
    private static HashSet<string> ReadKeys(string field)
    {
        using var response = NativePhaseScope.Input();
        return NativeWorld.Data(response).GetProperty(field).EnumerateArray()
            .Select(item => item.GetString() ?? string.Empty).ToHashSet(StringComparer.Ordinal);
    }
}

public static class Random
{
    public static float value => Range(0f, 1f);
    public static int Range(int minInclusive, int maxExclusive)
    {
        using var response = NativeWorld.Request("random.range", ("min", minInclusive), ("max", maxExclusive), ("integer", true));
        return NativeWorld.Data(response).GetInt32();
    }
    public static float Range(float minInclusive, float maxInclusive)
    {
        using var response = NativeWorld.Request("random.range", ("min", minInclusive), ("max", maxInclusive), ("integer", false));
        return NativeWorld.Data(response).GetSingle();
    }
}
