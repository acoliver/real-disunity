namespace UnityEngine;

/// <summary>
/// The shorter forms of <see cref="DrawMesh(Mesh, Vector3, Quaternion, Material, int, Camera?, int, MaterialPropertyBlock?, bool, bool, bool)"/>.
/// A caller compiled against a form binds to that form, not to the form with optional arguments, so each prefix of the argument list
/// exists. All of them throw <see cref="NotSupportedException"/> naming DrawMesh, as the long forms do.
/// </summary>
public static partial class Graphics
{
    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera, int submeshIndex) =>
        throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties, bool castShadows) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties, bool castShadows, bool receiveShadows) => throw Unsupported(nameof(DrawMesh));

    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera, int submeshIndex) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties, bool castShadows) => throw Unsupported(nameof(DrawMesh));
    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties, bool castShadows, bool receiveShadows) => throw Unsupported(nameof(DrawMesh));
}
