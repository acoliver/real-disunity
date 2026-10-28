namespace UnityEngine;

public record struct Vector2Int(int x, int y)
{
    public static Vector2Int zero => new(0, 0);
    public override readonly string ToString() => $"({x}, {y})";
    public static Vector2Int up => new(0, 1);
    public static Vector2Int down => new(0, -1);
    public static Vector2Int left => new(-1, 0);
    public static Vector2Int right => new(1, 0);
    public static Vector2Int operator +(Vector2Int a, Vector2Int b) => new(a.x + b.x, a.y + b.y);
    public static Vector2Int operator /(Vector2Int a, int b) => new(a.x / b, a.y / b);
}

public record struct Vector3(float X, float Y, float Z = 0)
{
    public float x { readonly get => X; set => X = value; }
    public float y { readonly get => Y; set => Y = value; }
    public float z { readonly get => Z; set => Z = value; }
    public static Vector3 zero => new(0, 0, 0);
    /// <summary>Unity's format, two decimals per component.</summary>
    public override readonly string ToString() => string.Format(System.Globalization.CultureInfo.InvariantCulture, "({0:F2}, {1:F2}, {2:F2})", X, Y, Z);
    public static Vector3 one => new(1, 1, 1);
    public static Vector3 operator *(Vector3 a, float b) => new(a.X * b, a.Y * b, a.Z * b);
    public static implicit operator Vector3(Vector2 a) => new(a.x, a.y, 0);
    public static implicit operator Vector2(Vector3 a) => new(a.X, a.Y);
}

public readonly record struct Quaternion(float x, float y, float z, float w)
{
    public static Quaternion identity => new(0, 0, 0, 1);
    /// <summary>Unity's format, five decimals per component.</summary>
    public override string ToString() => string.Format(System.Globalization.CultureInfo.InvariantCulture, "({0:F5}, {1:F5}, {2:F5}, {3:F5})", x, y, z, w);
}

public static class Mathf
{
    public static float Min(float a, float b) => MathF.Min(a, b);
    public static float Clamp(float value, float min, float max) => Math.Clamp(value, min, max);
}

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

