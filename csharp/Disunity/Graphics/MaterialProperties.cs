using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The typed property API of a material. A property is present once it has been set on this material; the
/// shader's declared properties are not known yet, so HasProperty and the Has methods do not report a default
/// the shader would declare. An unset property reads as zero, a matrix as identity and a texture as null.
/// Colors share the vector slot, so HasColor and HasVector agree, and SetInt stores a float as Unity does.
/// </summary>
public partial class Material
{
    public bool HasProperty(string name) => HasProperty(Shader.PropertyToID(name));
    public bool HasProperty(int nameID) => _properties.Has(nameID);
    public bool HasFloat(string name) => HasFloat(Shader.PropertyToID(name));
    public bool HasFloat(int nameID) => _properties.Floats.ContainsKey(nameID);
    public bool HasInt(string name) => HasInt(Shader.PropertyToID(name));
    public bool HasInt(int nameID) => _properties.Floats.ContainsKey(nameID);
    public bool HasInteger(string name) => HasInteger(Shader.PropertyToID(name));
    public bool HasInteger(int nameID) => _properties.Ints.ContainsKey(nameID);
    public bool HasTexture(string name) => HasTexture(Shader.PropertyToID(name));
    public bool HasTexture(int nameID) => _properties.Textures.ContainsKey(nameID);
    public bool HasMatrix(string name) => HasMatrix(Shader.PropertyToID(name));
    public bool HasMatrix(int nameID) => _properties.Matrices.ContainsKey(nameID);
    public bool HasVector(string name) => HasVector(Shader.PropertyToID(name));
    public bool HasVector(int nameID) => _properties.Vectors.ContainsKey(nameID);
    public bool HasColor(string name) => HasColor(Shader.PropertyToID(name));
    public bool HasColor(int nameID) => _properties.Vectors.ContainsKey(nameID);
    public bool HasBuffer(string name) => HasBuffer(Shader.PropertyToID(name));
    public bool HasBuffer(int nameID) => _properties.Buffers.ContainsKey(nameID);
    public bool HasConstantBuffer(string name) => HasConstantBuffer(Shader.PropertyToID(name));
    public bool HasConstantBuffer(int nameID) => _properties.ConstantBuffers.ContainsKey(nameID);

    public void SetFloat(string name, float value) => SetFloat(Shader.PropertyToID(name), value);
    public void SetFloat(int nameID, float value) => _properties.Floats[nameID] = value;
    public void SetInt(string name, int value) => SetInt(Shader.PropertyToID(name), value);
    public void SetInt(int nameID, int value) => _properties.Floats[nameID] = value;
    public void SetInteger(string name, int value) => SetInteger(Shader.PropertyToID(name), value);
    public void SetInteger(int nameID, int value) => _properties.Ints[nameID] = value;
    public void SetVector(string name, Vector4 value) => SetVector(Shader.PropertyToID(name), value);
    public void SetVector(int nameID, Vector4 value) => _properties.Vectors[nameID] = value;
    public void SetColor(string name, Color value) => SetColor(Shader.PropertyToID(name), value);
    public void SetColor(int nameID, Color value) => _properties.Vectors[nameID] = value;
    public void SetMatrix(string name, Matrix4x4 value) => SetMatrix(Shader.PropertyToID(name), value);
    public void SetMatrix(int nameID, Matrix4x4 value) => _properties.Matrices[nameID] = value;
    public void SetTexture(string name, Texture value) => SetTexture(Shader.PropertyToID(name), value);
    public void SetTexture(int nameID, Texture value) => _properties.Textures[nameID] = new(value, RenderTextureSubElement.Default);
    public void SetTexture(string name, RenderTexture value, RenderTextureSubElement element) => SetTexture(Shader.PropertyToID(name), value, element);
    public void SetTexture(int nameID, RenderTexture value, RenderTextureSubElement element) => _properties.Textures[nameID] = new(value, element);
    public void SetBuffer(string name, ComputeBuffer value) => SetBuffer(Shader.PropertyToID(name), value);
    public void SetBuffer(int nameID, ComputeBuffer value) => _properties.Buffers[nameID] = value;
    public void SetBuffer(string name, GraphicsBuffer value) => SetBuffer(Shader.PropertyToID(name), value);
    public void SetBuffer(int nameID, GraphicsBuffer value) => _properties.Buffers[nameID] = value;
    public void SetConstantBuffer(string name, ComputeBuffer value, int offset, int size) => SetConstantBuffer(Shader.PropertyToID(name), value, offset, size);
    public void SetConstantBuffer(int nameID, ComputeBuffer value, int offset, int size) => _properties.ConstantBuffers[nameID] = new(value, offset, size);
    public void SetConstantBuffer(string name, GraphicsBuffer value, int offset, int size) => SetConstantBuffer(Shader.PropertyToID(name), value, offset, size);
    public void SetConstantBuffer(int nameID, GraphicsBuffer value, int offset, int size) => _properties.ConstantBuffers[nameID] = new(value, offset, size);
    public void SetFloatArray(string name, List<float> values) => SetFloatArray(Shader.PropertyToID(name), values);
    public void SetFloatArray(int nameID, List<float> values) => _properties.FloatArrays[nameID] = PropertyStore.Copy(values, "SetFloatArray");
    public void SetFloatArray(string name, float[] values) => SetFloatArray(Shader.PropertyToID(name), values);
    public void SetFloatArray(int nameID, float[] values) => _properties.FloatArrays[nameID] = PropertyStore.Copy(values, "SetFloatArray");
    public void SetVectorArray(string name, List<Vector4> values) => SetVectorArray(Shader.PropertyToID(name), values);
    public void SetVectorArray(int nameID, List<Vector4> values) => _properties.VectorArrays[nameID] = PropertyStore.Copy(values, "SetVectorArray");
    public void SetVectorArray(string name, Vector4[] values) => SetVectorArray(Shader.PropertyToID(name), values);
    public void SetVectorArray(int nameID, Vector4[] values) => _properties.VectorArrays[nameID] = PropertyStore.Copy(values, "SetVectorArray");
    public void SetMatrixArray(string name, List<Matrix4x4> values) => SetMatrixArray(Shader.PropertyToID(name), values);
    public void SetMatrixArray(int nameID, List<Matrix4x4> values) => _properties.MatrixArrays[nameID] = PropertyStore.Copy(values, "SetMatrixArray");
    public void SetMatrixArray(string name, Matrix4x4[] values) => SetMatrixArray(Shader.PropertyToID(name), values);
    public void SetMatrixArray(int nameID, Matrix4x4[] values) => _properties.MatrixArrays[nameID] = PropertyStore.Copy(values, "SetMatrixArray");
    public void SetColorArray(string name, List<Color> values) => SetColorArray(Shader.PropertyToID(name), values);
    public void SetColorArray(int nameID, List<Color> values) => _properties.ColorArrays[nameID] = PropertyStore.Copy(values, "SetColorArray");
    public void SetColorArray(string name, Color[] values) => SetColorArray(Shader.PropertyToID(name), values);
    public void SetColorArray(int nameID, Color[] values) => _properties.ColorArrays[nameID] = PropertyStore.Copy(values, "SetColorArray");
    public float GetFloat(string name) => GetFloat(Shader.PropertyToID(name));
    public float GetFloat(int nameID) => _properties.Floats.GetValueOrDefault(nameID);
    public int GetInt(string name) => GetInt(Shader.PropertyToID(name));
    public int GetInt(int nameID) => (int)_properties.Floats.GetValueOrDefault(nameID);
    public int GetInteger(string name) => GetInteger(Shader.PropertyToID(name));
    public int GetInteger(int nameID) => _properties.Ints.GetValueOrDefault(nameID);
    public Vector4 GetVector(string name) => GetVector(Shader.PropertyToID(name));
    public Vector4 GetVector(int nameID) => _properties.Vectors.GetValueOrDefault(nameID);
    public Color GetColor(string name) => GetColor(Shader.PropertyToID(name));
    public Color GetColor(int nameID) => _properties.Vectors.GetValueOrDefault(nameID);
    public Matrix4x4 GetMatrix(string name) => GetMatrix(Shader.PropertyToID(name));
    public Matrix4x4 GetMatrix(int nameID) => _properties.Matrices.TryGetValue(nameID, out var matrix) ? matrix : Matrix4x4.identity;
    public Texture? GetTexture(string name) => GetTexture(Shader.PropertyToID(name));
    public Texture? GetTexture(int nameID) => _properties.Textures.TryGetValue(nameID, out var binding) ? binding.Texture : null;
    public float[]? GetFloatArray(string name) => GetFloatArray(Shader.PropertyToID(name));
    public float[]? GetFloatArray(int nameID) => PropertyStore.Clone(_properties.FloatArrays.GetValueOrDefault(nameID));
    public void GetFloatArray(string name, List<float> values) => GetFloatArray(Shader.PropertyToID(name), values);
    public void GetFloatArray(int nameID, List<float> values) => PropertyStore.Fill(_properties.FloatArrays.GetValueOrDefault(nameID), values);
    public Vector4[]? GetVectorArray(string name) => GetVectorArray(Shader.PropertyToID(name));
    public Vector4[]? GetVectorArray(int nameID) => PropertyStore.Clone(_properties.VectorArrays.GetValueOrDefault(nameID));
    public void GetVectorArray(string name, List<Vector4> values) => GetVectorArray(Shader.PropertyToID(name), values);
    public void GetVectorArray(int nameID, List<Vector4> values) => PropertyStore.Fill(_properties.VectorArrays.GetValueOrDefault(nameID), values);
    public Matrix4x4[]? GetMatrixArray(string name) => GetMatrixArray(Shader.PropertyToID(name));
    public Matrix4x4[]? GetMatrixArray(int nameID) => PropertyStore.Clone(_properties.MatrixArrays.GetValueOrDefault(nameID));
    public void GetMatrixArray(string name, List<Matrix4x4> values) => GetMatrixArray(Shader.PropertyToID(name), values);
    public void GetMatrixArray(int nameID, List<Matrix4x4> values) => PropertyStore.Fill(_properties.MatrixArrays.GetValueOrDefault(nameID), values);
    public Color[]? GetColorArray(string name) => GetColorArray(Shader.PropertyToID(name));
    public Color[]? GetColorArray(int nameID) => PropertyStore.Clone(_properties.ColorArrays.GetValueOrDefault(nameID));
    public void GetColorArray(string name, List<Color> values) => GetColorArray(Shader.PropertyToID(name), values);
    public void GetColorArray(int nameID, List<Color> values) => PropertyStore.Fill(_properties.ColorArrays.GetValueOrDefault(nameID), values);
}
