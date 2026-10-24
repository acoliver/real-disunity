namespace UnityEngine
{
    public struct Matrix4x4
    {
        public float e00, e01, e02, e03;
        public float e10, e11, e12, e13;
        public float e20, e21, e22, e23;
        public float e30, e31, e32, e33;
    }
}
namespace UnityEngine.Tilemaps
{
    public sealed class Tile : UnityEngine.ScriptableObject
    {
        public UnityEngine.Sprite? m_Sprite;
        public UnityEngine.Color m_Color;
        public UnityEngine.Matrix4x4 m_Transform;
        public UnityEngine.GameObject? m_InstancedGameObject;
        public int m_Flags;
        public int m_ColliderType;
    }
}
