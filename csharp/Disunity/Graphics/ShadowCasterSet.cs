using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine.Rendering;

/// <summary>What a native shadow query computes; the numbers are those of the ABI.</summary>
internal enum ShadowQueryKind : uint
{
    Bounds = 0,
    Directional = 1,
    Spot = 2,
    Point = 3,
}

/// <summary>The fields of a shadow query that only some kinds read: a directional light's split, a point light's face.</summary>
internal readonly struct ShadowQueryArguments
{
    public uint SplitIndex { get; init; }
    public uint SplitCount { get; init; }
    public Vector3 Ratio { get; init; }
    public uint Resolution { get; init; }
    public float NearPlaneOffset { get; init; }
    public uint Face { get; init; }
    public float FovBias { get; init; }

    public DisunityShadowRequest Apply(DisunityShadowRequest request)
    {
        request.split_index = SplitIndex;
        request.split_count = SplitCount;
        (request.ratio_x, request.ratio_y, request.ratio_z) = (Ratio.x, Ratio.y, Ratio.z);
        request.resolution = Resolution;
        request.near_plane_offset = NearPlaneOffset;
        request.face = Face;
        request.fov_bias = FovBias;
        return request;
    }
}

/// <summary>
/// The shadow casters of one <see cref="ScriptableRenderContext.Cull"/> and the native shadow query they are asked through. The
/// renderers are found at the first shadow query, the active and enabled ones on the layers the culling mask draws that have a mesh
/// to give a box; the native shadow module decides which of them cast and shadow a light, and fits the shadow maps. A cull that no
/// shadow query follows never looks at the renderers.
/// </summary>
internal sealed unsafe class ShadowCasterSet
{
    /// <summary>The floats a matrix holds.</summary>
    private const int MatrixFloats = 16;

    private readonly ScriptableCullingParameters _parameters;
    private DisunityShadowCaster[]? _casters;

    public ShadowCasterSet(in ScriptableCullingParameters parameters) => _parameters = parameters;

    /// <summary>What a native shadow query answered.</summary>
    internal readonly record struct Answer(bool Found, DisunityShadowResult Result, float[] Output)
    {
        public Matrix4x4 Matrix(int index)
        {
            var matrix = default(Matrix4x4);
            for (var element = 0; element < MatrixFloats; element++) matrix[element] = Output[index * MatrixFloats + element];
            return matrix;
        }
    }

    /// <summary>The query with the fields every kind of query shares: the light's kind, range and cone, the layers drawn and the shadow
    /// distance. The caller fills in the rest.</summary>
    internal DisunityShadowRequest RequestFor(in VisibleLight light, uint query) => new()
    {
        query = query,
        light_kind = (uint)light.lightType,
        light_range = light.range,
        light_spot_angle = light.spotAngle,
        camera_x = _parameters.origin.x,
        camera_y = _parameters.origin.y,
        camera_z = _parameters.origin.z,
        shadow_distance = _parameters.shadowDistance,
        culling_mask = _parameters.cullingMask,
    };

    /// <summary>Asks the native shadow module. A query it refuses throws <see cref="ArgumentException"/> with its reason.</summary>
    internal Answer Ask(in VisibleLight light, DisunityShadowRequest request)
    {
        var casters = Casters();
        request.caster_count = (uint)casters.Length;
        var matrices = new float[2 * MatrixFloats];
        for (var element = 0; element < MatrixFloats; element++)
        {
            matrices[element] = light.localToWorldMatrix[element];
            matrices[MatrixFloats + element] = _parameters.cullingMatrix[element];
        }
        DisunityShadowResult result;
        var output = new float[3 * MatrixFloats + ShadowSplitData.MaxCullingPlanes * 4];
        DisunityResult status;
        fixed (DisunityShadowCaster* castersPointer = casters)
        fixed (float* matricesPointer = matrices, outputPointer = output)
            status = NativeBindings.disunity_shadow_query(&request, matricesPointer, casters.Length == 0 ? null : castersPointer, &result, outputPointer);
        try { DisunityHost.ThrowIfError(status); }
        catch (DisunityException error) when (error.Reason == DisunityErrorReason.InvalidArgument) { throw new ArgumentException(error.Message); }
        return new Answer(result.found == 1, result, output);
    }

    /// <summary>The renderers that can cast, each as its world box, shadow casting mode and layer.</summary>
    private DisunityShadowCaster[] Casters() => _casters ??= Gather();

    private static DisunityShadowCaster[] Gather()
    {
        var found = new List<DisunityShadowCaster>();
        foreach (var renderer in ObjectRegistry.SceneObjects(typeof(Renderer)).OfType<Renderer>())
        {
            if (!renderer || !renderer.enabled || !renderer.gameObject.activeInHierarchy) continue;
            var bounds = renderer.bounds;
            if (bounds.size == Vector3.zero) continue;
            found.Add(new DisunityShadowCaster
            {
                min_x = bounds.min.x, min_y = bounds.min.y, min_z = bounds.min.z,
                max_x = bounds.max.x, max_y = bounds.max.y, max_z = bounds.max.z,
                casting_mode = (uint)renderer.shadowCastingMode,
                layer = (uint)renderer.gameObject.layer,
            });
        }
        return found.ToArray();
    }
}
