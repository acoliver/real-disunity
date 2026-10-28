using UnityEngine.Rendering;

namespace UnityEngine
{
    /// <summary>Unity's identifier of an object; <see cref="Object.GetEntityId"/> wraps the instance ID.</summary>
    public readonly struct EntityId : IEquatable<EntityId>, IComparable<EntityId>
    {
        private readonly int m_Data;

        internal EntityId(int data) => m_Data = data;

        public static EntityId None => default;
        public bool IsValid() => m_Data != 0;
        public bool Equals(EntityId other) => m_Data == other.m_Data;
        public bool Equals(int other) => m_Data == other;
        public override bool Equals(object? obj) => obj is EntityId other && Equals(other);
        public int CompareTo(EntityId other) => m_Data.CompareTo(other.m_Data);
        public override int GetHashCode() => m_Data;
        public override string ToString() => m_Data.ToString(System.Globalization.CultureInfo.InvariantCulture);
        public string ToString(string format) => m_Data.ToString(format, System.Globalization.CultureInfo.InvariantCulture);

        public static bool operator ==(EntityId left, EntityId right) => left.m_Data == right.m_Data;
        public static bool operator !=(EntityId left, EntityId right) => left.m_Data != right.m_Data;
        public static bool operator <(EntityId left, EntityId right) => left.m_Data < right.m_Data;
        public static bool operator >(EntityId left, EntityId right) => left.m_Data > right.m_Data;
        public static bool operator <=(EntityId left, EntityId right) => left.m_Data <= right.m_Data;
        public static bool operator >=(EntityId left, EntityId right) => left.m_Data >= right.m_Data;
        public static implicit operator int(EntityId entityId) => entityId.m_Data;
        public static implicit operator EntityId(int intValue) => new(intValue);
        public static implicit operator EntityId(InstanceID entityId) => new(entityId);
        public static implicit operator InstanceID(EntityId entityId) => new(entityId.m_Data);
    }

    /// <summary>The older name of <see cref="EntityId"/>.</summary>
    public readonly struct InstanceID : IEquatable<InstanceID>, IComparable<InstanceID>
    {
        private readonly int m_Data;

        internal InstanceID(int data) => m_Data = data;

        public static InstanceID None => default;
        public bool IsValid() => m_Data != 0;
        public bool Equals(InstanceID other) => m_Data == other.m_Data;
        public bool Equals(int other) => m_Data == other;
        public override bool Equals(object? obj) => obj is InstanceID other && Equals(other);
        public int CompareTo(InstanceID other) => m_Data.CompareTo(other.m_Data);
        public override int GetHashCode() => m_Data;
        public override string ToString() => m_Data.ToString(System.Globalization.CultureInfo.InvariantCulture);
        public string ToString(string format) => m_Data.ToString(format, System.Globalization.CultureInfo.InvariantCulture);

        public static bool operator ==(InstanceID left, InstanceID right) => left.m_Data == right.m_Data;
        public static bool operator !=(InstanceID left, InstanceID right) => left.m_Data != right.m_Data;
        public static bool operator <(InstanceID left, InstanceID right) => left.m_Data < right.m_Data;
        public static bool operator >(InstanceID left, InstanceID right) => left.m_Data > right.m_Data;
        public static bool operator <=(InstanceID left, InstanceID right) => left.m_Data <= right.m_Data;
        public static bool operator >=(InstanceID left, InstanceID right) => left.m_Data >= right.m_Data;
        public static implicit operator int(InstanceID entityId) => entityId.m_Data;
        public static implicit operator InstanceID(int intValue) => new(intValue);
        public static implicit operator EntityId(InstanceID entityId) => new(entityId.m_Data);
        public static implicit operator InstanceID(EntityId entityId) => new((int)entityId);
    }

    /// <summary>Which mipmap limit group a texture belongs to. The engine stores it and applies no limit.</summary>
    public readonly struct MipmapLimitDescriptor
    {
        public MipmapLimitDescriptor(bool useMipmapLimit, string groupName)
        {
            this.useMipmapLimit = useMipmapLimit;
            this.groupName = groupName;
        }

        public bool useMipmapLimit { get; }
        public string groupName { get; }
    }

    /// <summary>The identity of a compute or graphics buffer. Its members come with the buffer types in G2.</summary>
    public struct GraphicsBufferHandle : IEquatable<GraphicsBufferHandle>, IComparable<GraphicsBufferHandle>
    {
        public uint value;

        public readonly int CompareTo(GraphicsBufferHandle other) => value.CompareTo(other.value);
        public readonly bool Equals(GraphicsBufferHandle other) => value == other.value;
        public override readonly bool Equals(object? obj) => obj is GraphicsBufferHandle other && Equals(other);
        public override readonly int GetHashCode() => unchecked((int)value);
        public static bool operator ==(GraphicsBufferHandle a, GraphicsBufferHandle b) => a.value == b.value;
        public static bool operator !=(GraphicsBufferHandle a, GraphicsBufferHandle b) => a.value != b.value;
    }

    /// <summary>A GPU buffer. Only its identity exists in G1; its members come in G2.</summary>
    public sealed class ComputeBuffer;

    /// <summary>A GPU buffer. Only its identity exists in G1; its members come in G2.</summary>
    public sealed class GraphicsBuffer;
}

namespace UnityEngine.Rendering
{
    /// <summary>A ray tracing scene. Only its identity exists in G1; its members come with ray tracing support.</summary>
    public sealed class RayTracingAccelerationStructure;

    /// <summary>A GPU texture handle. Only its identity exists in G1; its members come with GPU texture support.</summary>
    public sealed class GraphicsTexture;

    /// <summary>A shader tag name stored as an integer, as <see cref="Shader.PropertyToID"/> does for properties.</summary>
    public readonly struct ShaderTagId : IEquatable<ShaderTagId>
    {
        private static readonly NameTable Tags = new(1);
        public static readonly ShaderTagId none = new(-1);

        private readonly int m_Id;

        private ShaderTagId(int id) => m_Id = id;
        public ShaderTagId(string name) => m_Id = Tags.IdOf(name);

        public string name => Tags.NameOf(m_Id) ?? "";
        public bool Equals(ShaderTagId other) => m_Id == other.m_Id;
        public override bool Equals(object? obj) => obj is ShaderTagId other && Equals(other);
        public override int GetHashCode() => m_Id;
        public static bool operator ==(ShaderTagId tag1, ShaderTagId tag2) => tag1.m_Id == tag2.m_Id;
        public static bool operator !=(ShaderTagId tag1, ShaderTagId tag2) => tag1.m_Id != tag2.m_Id;
        public static explicit operator ShaderTagId(string name) => new(name);
        public static explicit operator string(ShaderTagId tagId) => tagId.name;
    }
}

namespace UnityEngine
{
    /// <summary>Assigns integers to names, the same integer to the same name for the life of the process.</summary>
    internal sealed class NameTable(int firstId)
    {
        private readonly Dictionary<string, int> _ids = new(StringComparer.Ordinal);
        private readonly List<string> _names = new();

        public int IdOf(string name)
        {
            ArgumentNullException.ThrowIfNull(name);
            lock (_ids)
            {
                if (_ids.TryGetValue(name, out var id)) return id;
                id = firstId + _names.Count;
                _ids.Add(name, id);
                _names.Add(name);
                return id;
            }
        }

        public string? NameOf(int id)
        {
            lock (_ids) return id >= firstId && id - firstId < _names.Count ? _names[id - firstId] : null;
        }
    }
}
