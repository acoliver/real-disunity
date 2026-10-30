namespace UnityEngine.Rendering;

/// <summary>The render queue values of the materials a draw may use, from <see cref="lowerBound"/> to
/// <see cref="upperBound"/> inclusive. The queues are 0 to 5000; opaque ones end at 2500 and transparent
/// ones start above it. It holds values; no draw uses it yet.</summary>
public struct RenderQueueRange : IEquatable<RenderQueueRange>
{
    public int lowerBound;
    public int upperBound;

    public static RenderQueueRange all => new() { lowerBound = 0, upperBound = 5000 };

    public static RenderQueueRange opaque => new() { lowerBound = 0, upperBound = 2500 };

    public static RenderQueueRange transparent => new() { lowerBound = 2501, upperBound = 5000 };

    public bool Equals(RenderQueueRange other) => lowerBound == other.lowerBound && upperBound == other.upperBound;

    public override bool Equals(object? obj) => obj is RenderQueueRange other && Equals(other);

    public override int GetHashCode() => HashCode.Combine(lowerBound, upperBound);

    public static bool operator ==(RenderQueueRange left, RenderQueueRange right) => left.Equals(right);

    public static bool operator !=(RenderQueueRange left, RenderQueueRange right) => !left.Equals(right);
}
