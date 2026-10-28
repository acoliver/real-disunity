using System.Runtime.InteropServices;

namespace UnityEngine
{
    /// <summary>
    /// One surface of a <see cref="RenderTexture"/>. The values are opaque tokens, not GPU addresses, and
    /// distinguish the color buffer from the depth buffer of the same texture.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct RenderBuffer
    {
        internal int m_RenderTextureInstanceID;
        internal IntPtr m_BufferPtr;

        /// <summary>No GPU object exists, so this is always zero.</summary>
        public readonly IntPtr GetNativeRenderBufferPtr() => IntPtr.Zero;
    }
}

namespace UnityEngine.Rendering
{
    /// <summary>
    /// Names a render target: a built-in target, a shader property, a texture or a render buffer, with the
    /// mip level, cube face and depth slice to use. Two identifiers are equal when every field is equal.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    public struct RenderTargetIdentifier : IEquatable<RenderTargetIdentifier>
    {
        public const int AllDepthSlices = -1;

        public static readonly RenderTargetIdentifier Invalid = new(BuiltinRenderTextureType.None, -1, CubemapFace.Unknown, -1);

        internal BuiltinRenderTextureType m_Type;
        internal int m_NameID;
        internal int m_InstanceID;
        internal IntPtr m_BufferPointer;
        internal int m_MipLevel;
        internal CubemapFace m_CubeFace;
        internal int m_DepthSlice;

        public RenderTargetIdentifier(BuiltinRenderTextureType type) : this(type, 0, CubemapFace.Unknown, 0) { }

        public RenderTargetIdentifier(BuiltinRenderTextureType type, int mipLevel, CubemapFace cubeFace, int depthSlice)
        {
            m_Type = type;
            m_NameID = -1;
            m_InstanceID = 0;
            m_BufferPointer = IntPtr.Zero;
            m_MipLevel = mipLevel;
            m_CubeFace = cubeFace;
            m_DepthSlice = depthSlice;
        }

        public RenderTargetIdentifier(string name) : this(name, 0, CubemapFace.Unknown, 0) { }

        public RenderTargetIdentifier(string name, int mipLevel, CubemapFace cubeFace, int depthSlice) :
            this(Shader.PropertyToID(name), mipLevel, cubeFace, depthSlice) { }

        public RenderTargetIdentifier(int nameID) : this(nameID, 0, CubemapFace.Unknown, 0) { }

        public RenderTargetIdentifier(int nameID, int mipLevel, CubemapFace cubeFace, int depthSlice) :
            this(BuiltinRenderTextureType.PropertyName, mipLevel, cubeFace, depthSlice) => m_NameID = nameID;

        public RenderTargetIdentifier(RenderTargetIdentifier renderTargetIdentifier, int mipLevel, CubemapFace cubeFace, int depthSlice)
        {
            this = renderTargetIdentifier;
            m_MipLevel = mipLevel;
            m_CubeFace = cubeFace;
            m_DepthSlice = depthSlice;
        }

        public RenderTargetIdentifier(Texture tex) : this(tex, 0, CubemapFace.Unknown, 0) { }

        public RenderTargetIdentifier(Texture tex, int mipLevel, CubemapFace cubeFace, int depthSlice) :
            this(TypeOf(tex), mipLevel, cubeFace, depthSlice) => m_InstanceID = tex ? tex.GetInstanceID() : 0;

        public RenderTargetIdentifier(RenderBuffer buf, int mipLevel, CubemapFace cubeFace, int depthSlice) :
            this(BuiltinRenderTextureType.BufferPtr, mipLevel, cubeFace, depthSlice)
        {
            m_InstanceID = buf.m_RenderTextureInstanceID;
            m_BufferPointer = buf.m_BufferPtr;
        }

        private static BuiltinRenderTextureType TypeOf(Texture? tex) => tex switch
        {
            null => BuiltinRenderTextureType.None,
            RenderTexture => BuiltinRenderTextureType.RenderTexture,
            _ => BuiltinRenderTextureType.BindableTexture,
        };

        public static implicit operator RenderTargetIdentifier(BuiltinRenderTextureType type) => new(type);
        public static implicit operator RenderTargetIdentifier(string name) => new(name);
        public static implicit operator RenderTargetIdentifier(int nameID) => new(nameID);
        public static implicit operator RenderTargetIdentifier(Texture tex) => new(tex);
        public static implicit operator RenderTargetIdentifier(RenderBuffer buf) => new(buf, 0, CubemapFace.Unknown, 0);

        public override readonly string ToString() => $"Type {m_Type} NameID {m_NameID} InstanceID {m_InstanceID}";

        public override readonly int GetHashCode() =>
            HashCode.Combine(m_Type, m_NameID, m_InstanceID, m_BufferPointer, m_MipLevel, m_CubeFace, m_DepthSlice);

        public readonly bool Equals(RenderTargetIdentifier rhs) => m_Type == rhs.m_Type && m_NameID == rhs.m_NameID
            && m_InstanceID == rhs.m_InstanceID && m_BufferPointer == rhs.m_BufferPointer && m_MipLevel == rhs.m_MipLevel
            && m_CubeFace == rhs.m_CubeFace && m_DepthSlice == rhs.m_DepthSlice;

        public override readonly bool Equals(object? obj) => obj is RenderTargetIdentifier other && Equals(other);

        public static bool operator ==(RenderTargetIdentifier lhs, RenderTargetIdentifier rhs) => lhs.Equals(rhs);

        public static bool operator !=(RenderTargetIdentifier lhs, RenderTargetIdentifier rhs) => !lhs.Equals(rhs);
    }
}
