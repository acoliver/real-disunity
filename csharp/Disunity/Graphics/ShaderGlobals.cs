using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The global shader properties. They are stored and read back; nothing applies them to a draw yet. A global
/// that was never set reads as zero, a matrix as identity and a texture as null.
/// </summary>
public sealed partial class Shader
{
    internal static readonly PropertyStore Globals = new();

    public static void SetGlobalFloat(string name, float value) => SetGlobalFloat(PropertyToID(name), value);
    public static void SetGlobalFloat(int nameID, float value) => Globals.Floats[nameID] = value;
    public static void SetGlobalInt(string name, int value) => SetGlobalInt(PropertyToID(name), value);
    public static void SetGlobalInt(int nameID, int value) => Globals.Floats[nameID] = value;
    public static void SetGlobalInteger(string name, int value) => SetGlobalInteger(PropertyToID(name), value);
    public static void SetGlobalInteger(int nameID, int value) => Globals.Ints[nameID] = value;
    public static void SetGlobalVector(string name, Vector4 value) => SetGlobalVector(PropertyToID(name), value);
    public static void SetGlobalVector(int nameID, Vector4 value) => Globals.Vectors[nameID] = value;
    public static void SetGlobalColor(string name, Color value) => SetGlobalColor(PropertyToID(name), value);
    public static void SetGlobalColor(int nameID, Color value) => Globals.Vectors[nameID] = value;
    public static void SetGlobalMatrix(string name, Matrix4x4 value) => SetGlobalMatrix(PropertyToID(name), value);
    public static void SetGlobalMatrix(int nameID, Matrix4x4 value) => Globals.Matrices[nameID] = value;
    public static void SetGlobalTexture(string name, Texture value) => SetGlobalTexture(PropertyToID(name), value);
    public static void SetGlobalTexture(int nameID, Texture value) => Globals.Textures[nameID] = new(value, RenderTextureSubElement.Default);
    public static void SetGlobalTexture(string name, RenderTexture value, RenderTextureSubElement element) => SetGlobalTexture(PropertyToID(name), value, element);
    public static void SetGlobalTexture(int nameID, RenderTexture value, RenderTextureSubElement element) => Globals.Textures[nameID] = new(value, element);
    public static void SetGlobalBuffer(string name, ComputeBuffer value) => SetGlobalBuffer(PropertyToID(name), value);
    public static void SetGlobalBuffer(int nameID, ComputeBuffer value) => Globals.Buffers[nameID] = value;
    public static void SetGlobalBuffer(string name, GraphicsBuffer value) => SetGlobalBuffer(PropertyToID(name), value);
    public static void SetGlobalBuffer(int nameID, GraphicsBuffer value) => Globals.Buffers[nameID] = value;
    public static void SetGlobalConstantBuffer(string name, ComputeBuffer value, int offset, int size) => SetGlobalConstantBuffer(PropertyToID(name), value, offset, size);
    public static void SetGlobalConstantBuffer(int nameID, ComputeBuffer value, int offset, int size) => Globals.ConstantBuffers[nameID] = new(value, offset, size);
    public static void SetGlobalConstantBuffer(string name, GraphicsBuffer value, int offset, int size) => SetGlobalConstantBuffer(PropertyToID(name), value, offset, size);
    public static void SetGlobalConstantBuffer(int nameID, GraphicsBuffer value, int offset, int size) => Globals.ConstantBuffers[nameID] = new(value, offset, size);
    public static void SetGlobalFloatArray(string name, List<float> values) => SetGlobalFloatArray(PropertyToID(name), values);
    public static void SetGlobalFloatArray(int nameID, List<float> values) => Globals.FloatArrays[nameID] = PropertyStore.Copy(values, "SetGlobalFloatArray");
    public static void SetGlobalFloatArray(string name, float[] values) => SetGlobalFloatArray(PropertyToID(name), values);
    public static void SetGlobalFloatArray(int nameID, float[] values) => Globals.FloatArrays[nameID] = PropertyStore.Copy(values, "SetGlobalFloatArray");
    public static void SetGlobalVectorArray(string name, List<Vector4> values) => SetGlobalVectorArray(PropertyToID(name), values);
    public static void SetGlobalVectorArray(int nameID, List<Vector4> values) => Globals.VectorArrays[nameID] = PropertyStore.Copy(values, "SetGlobalVectorArray");
    public static void SetGlobalVectorArray(string name, Vector4[] values) => SetGlobalVectorArray(PropertyToID(name), values);
    public static void SetGlobalVectorArray(int nameID, Vector4[] values) => Globals.VectorArrays[nameID] = PropertyStore.Copy(values, "SetGlobalVectorArray");
    public static void SetGlobalMatrixArray(string name, List<Matrix4x4> values) => SetGlobalMatrixArray(PropertyToID(name), values);
    public static void SetGlobalMatrixArray(int nameID, List<Matrix4x4> values) => Globals.MatrixArrays[nameID] = PropertyStore.Copy(values, "SetGlobalMatrixArray");
    public static void SetGlobalMatrixArray(string name, Matrix4x4[] values) => SetGlobalMatrixArray(PropertyToID(name), values);
    public static void SetGlobalMatrixArray(int nameID, Matrix4x4[] values) => Globals.MatrixArrays[nameID] = PropertyStore.Copy(values, "SetGlobalMatrixArray");
    public static float GetGlobalFloat(string name) => GetGlobalFloat(PropertyToID(name));
    public static float GetGlobalFloat(int nameID) => Globals.Floats.GetValueOrDefault(nameID);
    public static int GetGlobalInt(string name) => GetGlobalInt(PropertyToID(name));
    public static int GetGlobalInt(int nameID) => (int)Globals.Floats.GetValueOrDefault(nameID);
    public static int GetGlobalInteger(string name) => GetGlobalInteger(PropertyToID(name));
    public static int GetGlobalInteger(int nameID) => Globals.Ints.GetValueOrDefault(nameID);
    public static Vector4 GetGlobalVector(string name) => GetGlobalVector(PropertyToID(name));
    public static Vector4 GetGlobalVector(int nameID) => Globals.Vectors.GetValueOrDefault(nameID);
    public static Color GetGlobalColor(string name) => GetGlobalColor(PropertyToID(name));
    public static Color GetGlobalColor(int nameID) => Globals.Vectors.GetValueOrDefault(nameID);
    public static Matrix4x4 GetGlobalMatrix(string name) => GetGlobalMatrix(PropertyToID(name));
    public static Matrix4x4 GetGlobalMatrix(int nameID) => Globals.Matrices.TryGetValue(nameID, out var matrix) ? matrix : Matrix4x4.identity;
    public static Texture? GetGlobalTexture(string name) => GetGlobalTexture(PropertyToID(name));
    public static Texture? GetGlobalTexture(int nameID) => Globals.Textures.TryGetValue(nameID, out var binding) ? binding.Texture : null;
    public static float[]? GetGlobalFloatArray(string name) => GetGlobalFloatArray(PropertyToID(name));
    public static float[]? GetGlobalFloatArray(int nameID) => PropertyStore.Clone(Globals.FloatArrays.GetValueOrDefault(nameID));
    public static void GetGlobalFloatArray(string name, List<float> values) => GetGlobalFloatArray(PropertyToID(name), values);
    public static void GetGlobalFloatArray(int nameID, List<float> values) => PropertyStore.Fill(Globals.FloatArrays.GetValueOrDefault(nameID), values);
    public static Vector4[]? GetGlobalVectorArray(string name) => GetGlobalVectorArray(PropertyToID(name));
    public static Vector4[]? GetGlobalVectorArray(int nameID) => PropertyStore.Clone(Globals.VectorArrays.GetValueOrDefault(nameID));
    public static void GetGlobalVectorArray(string name, List<Vector4> values) => GetGlobalVectorArray(PropertyToID(name), values);
    public static void GetGlobalVectorArray(int nameID, List<Vector4> values) => PropertyStore.Fill(Globals.VectorArrays.GetValueOrDefault(nameID), values);
    public static Matrix4x4[]? GetGlobalMatrixArray(string name) => GetGlobalMatrixArray(PropertyToID(name));
    public static Matrix4x4[]? GetGlobalMatrixArray(int nameID) => PropertyStore.Clone(Globals.MatrixArrays.GetValueOrDefault(nameID));
    public static void GetGlobalMatrixArray(string name, List<Matrix4x4> values) => GetGlobalMatrixArray(PropertyToID(name), values);
    public static void GetGlobalMatrixArray(int nameID, List<Matrix4x4> values) => PropertyStore.Fill(Globals.MatrixArrays.GetValueOrDefault(nameID), values);
    public static void SetGlobalRayTracingAccelerationStructure(string name, RayTracingAccelerationStructure value) => SetGlobalRayTracingAccelerationStructure(PropertyToID(name), value);
    public static void SetGlobalRayTracingAccelerationStructure(int nameID, RayTracingAccelerationStructure value) => Globals.Buffers[nameID] = value;
}
