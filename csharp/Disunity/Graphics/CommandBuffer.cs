using System.Diagnostics.CodeAnalysis;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine.Rendering;

/// <summary>
/// A list of rendering commands. The engine does not record command buffers yet: every recording
/// member throws <see cref="NotSupportedException"/> naming itself, and <see cref="Clear"/> has nothing
/// to empty. After <see cref="Release"/> every member except <see cref="name"/> and Release throws
/// <see cref="ObjectDisposedException"/>; releasing twice is allowed. The name starts empty, which is
/// the engine's choice, and a null name becomes empty.
/// </summary>
public sealed class CommandBuffer : IDisposable
{
    private string _name = "";
    private bool _released;

    public string name
    {
        get => _name;
        set => _name = value ?? "";
    }

    public void Clear() => ThrowIfReleased();

    public void Release() => _released = true;

    public void Dispose() => Release();

    public void BeginSample(string name) => Fail(nameof(BeginSample));
    public void EndSample(string name) => Fail(nameof(EndSample));

    public void Blit(Texture source, RenderTargetIdentifier dest) => Fail(nameof(Blit));
    public void Blit(Texture source, RenderTargetIdentifier dest, Material mat) => Fail(nameof(Blit));
    public void Blit(Texture source, RenderTargetIdentifier dest, Material mat, int pass) => Fail(nameof(Blit));
    public void Blit(RenderTargetIdentifier source, RenderTargetIdentifier dest) => Fail(nameof(Blit));
    public void Blit(RenderTargetIdentifier source, RenderTargetIdentifier dest, Material mat) => Fail(nameof(Blit));
    public void Blit(RenderTargetIdentifier source, RenderTargetIdentifier dest, Material mat, int pass) => Fail(nameof(Blit));

    public void ClearRenderTarget(bool clearDepth, bool clearColor, Color backgroundColor) => Fail(nameof(ClearRenderTarget));
    public void ClearRenderTarget(bool clearDepth, bool clearColor, Color backgroundColor, float depth) => Fail(nameof(ClearRenderTarget));

    public void CopyTexture(RenderTargetIdentifier src, int srcElement, int srcMip, RenderTargetIdentifier dst, int dstElement, int dstMip) =>
        Fail(nameof(CopyTexture));

    public void EnableShaderKeyword(string keyword) => Fail(nameof(EnableShaderKeyword));
    public void DisableShaderKeyword(string keyword) => Fail(nameof(DisableShaderKeyword));

    public void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int submeshIndex, int shaderPass) => Fail(nameof(DrawMesh));

    public void DrawMeshInstancedIndirect(Mesh mesh, int submeshIndex, Material material, int shaderPass, ComputeBuffer bufferWithArgs,
        int argsOffset, MaterialPropertyBlock properties) => Fail(nameof(DrawMeshInstancedIndirect));

    public void GetTemporaryRT(int nameID, int width, int height, int depthBuffer, FilterMode filter, RenderTextureFormat format,
        RenderTextureReadWrite readWrite) => Fail(nameof(GetTemporaryRT));

    public void ReleaseTemporaryRT(int nameID) => Fail(nameof(ReleaseTemporaryRT));

    public void SetGlobalColor(int nameID, Color value) => Fail(nameof(SetGlobalColor));
    public void SetGlobalColor(string name, Color value) => Fail(nameof(SetGlobalColor));
    public void SetGlobalFloat(int nameID, float value) => Fail(nameof(SetGlobalFloat));
    public void SetGlobalMatrix(int nameID, Matrix4x4 value) => Fail(nameof(SetGlobalMatrix));
    public void SetGlobalTexture(int nameID, RenderTargetIdentifier value) => Fail(nameof(SetGlobalTexture));
    public void SetGlobalTexture(string name, RenderTargetIdentifier value) => Fail(nameof(SetGlobalTexture));
    public void SetGlobalVector(int nameID, Vector4 value) => Fail(nameof(SetGlobalVector));
    public void SetGlobalVector(string name, Vector4 value) => Fail(nameof(SetGlobalVector));

    public void SetRenderTarget(RenderTargetIdentifier rt) => Fail(nameof(SetRenderTarget));
    public void SetRenderTarget(RenderTargetIdentifier[] colors, RenderTargetIdentifier depth) => Fail(nameof(SetRenderTarget));

    public void SetViewport(Rect pixelRect) => Fail(nameof(SetViewport));
    public void SetViewProjectionMatrices(Matrix4x4 view, Matrix4x4 proj) => Fail(nameof(SetViewProjectionMatrices));

    private void ThrowIfReleased() => ObjectDisposedException.ThrowIf(_released, this);

    [DoesNotReturn]
    private void Fail(string member)
    {
        ThrowIfReleased();
        throw new NotSupportedException($"CommandBuffer.{member}: the engine does not record command buffers yet");
    }
}
