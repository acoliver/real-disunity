namespace UnityEngine;

/// <summary>Unity's random numbers. Unseeded, the sequence differs every run, as in Unity.
/// After <see cref="InitState"/> a seed always gives the same sequence, though not the values
/// Unity's generator gives for that seed.</summary>
public static class Random
{
    private static System.Random _source = new();
    public static void InitState(int seed) => _source = new System.Random(seed);
    public static float value => _source.NextSingle();
    public static int Range(int minInclusive, int maxExclusive) => _source.Next(minInclusive, maxExclusive);
    public static float Range(float minInclusive, float maxInclusive) => minInclusive + value * (maxInclusive - minInclusive);
}

public static class Debug
{
    public static void Log(object? message) => Console.WriteLine(message);
    public static void LogError(object? message) => Console.Error.WriteLine(message);
}
