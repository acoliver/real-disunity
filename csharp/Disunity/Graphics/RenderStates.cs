namespace UnityEngine.Rendering;

/// <summary>The blend setup of one render target. Plain data.</summary>
public struct RenderTargetBlendState : IEquatable<RenderTargetBlendState>
{
    public ColorWriteMask writeMask { get; set; }
    public BlendMode sourceColorBlendMode { get; set; }
    public BlendMode destinationColorBlendMode { get; set; }
    public BlendMode sourceAlphaBlendMode { get; set; }
    public BlendMode destinationAlphaBlendMode { get; set; }
    public BlendOp colorBlendOperation { get; set; }
    public BlendOp alphaBlendOperation { get; set; }

    public RenderTargetBlendState(ColorWriteMask writeMask = ColorWriteMask.All, BlendMode sourceColorBlendMode = BlendMode.One,
        BlendMode destinationColorBlendMode = BlendMode.Zero, BlendMode sourceAlphaBlendMode = BlendMode.One,
        BlendMode destinationAlphaBlendMode = BlendMode.Zero, BlendOp colorBlendOperation = BlendOp.Add, BlendOp alphaBlendOperation = BlendOp.Add)
    {
        this.writeMask = writeMask;
        this.sourceColorBlendMode = sourceColorBlendMode;
        this.destinationColorBlendMode = destinationColorBlendMode;
        this.sourceAlphaBlendMode = sourceAlphaBlendMode;
        this.destinationAlphaBlendMode = destinationAlphaBlendMode;
        this.colorBlendOperation = colorBlendOperation;
        this.alphaBlendOperation = alphaBlendOperation;
    }

    /// <summary>Write all channels, source one, destination zero, add.</summary>
    public static RenderTargetBlendState defaultValue =>
        new(ColorWriteMask.All, BlendMode.One, BlendMode.Zero, BlendMode.One, BlendMode.Zero, BlendOp.Add, BlendOp.Add);

    public readonly bool Equals(RenderTargetBlendState other) =>
        writeMask == other.writeMask && sourceColorBlendMode == other.sourceColorBlendMode
        && destinationColorBlendMode == other.destinationColorBlendMode && sourceAlphaBlendMode == other.sourceAlphaBlendMode
        && destinationAlphaBlendMode == other.destinationAlphaBlendMode && colorBlendOperation == other.colorBlendOperation
        && alphaBlendOperation == other.alphaBlendOperation;

    public override readonly bool Equals(object? obj) => obj is RenderTargetBlendState other && Equals(other);

    public override readonly int GetHashCode() => HashCode.Combine(writeMask, sourceColorBlendMode, destinationColorBlendMode,
        HashCode.Combine(sourceAlphaBlendMode, destinationAlphaBlendMode, colorBlendOperation, alphaBlendOperation));

    public static bool operator ==(RenderTargetBlendState left, RenderTargetBlendState right) => left.Equals(right);
    public static bool operator !=(RenderTargetBlendState left, RenderTargetBlendState right) => !left.Equals(right);
}

/// <summary>The blend setup for up to eight render targets. Plain data.</summary>
public struct BlendState : IEquatable<BlendState>
{
    public bool separateMRTBlendStates { get; set; }
    public bool alphaToMask { get; set; }
    public RenderTargetBlendState blendState0 { get; set; }
    public RenderTargetBlendState blendState1 { get; set; }
    public RenderTargetBlendState blendState2 { get; set; }
    public RenderTargetBlendState blendState3 { get; set; }
    public RenderTargetBlendState blendState4 { get; set; }
    public RenderTargetBlendState blendState5 { get; set; }
    public RenderTargetBlendState blendState6 { get; set; }
    public RenderTargetBlendState blendState7 { get; set; }

    public BlendState(bool separateMRTBlend = false, bool alphaToMask = false)
    {
        separateMRTBlendStates = separateMRTBlend;
        this.alphaToMask = alphaToMask;
        blendState0 = blendState1 = blendState2 = blendState3 = RenderTargetBlendState.defaultValue;
        blendState4 = blendState5 = blendState6 = blendState7 = RenderTargetBlendState.defaultValue;
    }

    public static BlendState defaultValue => new(false, false);

    private readonly RenderTargetBlendState[] Targets() =>
        [blendState0, blendState1, blendState2, blendState3, blendState4, blendState5, blendState6, blendState7];

    public readonly bool Equals(BlendState other) =>
        separateMRTBlendStates == other.separateMRTBlendStates && alphaToMask == other.alphaToMask
        && Targets().SequenceEqual(other.Targets());

    public override readonly bool Equals(object? obj) => obj is BlendState other && Equals(other);

    public override readonly int GetHashCode() =>
        HashCode.Combine(separateMRTBlendStates, alphaToMask, blendState0, blendState1, blendState2, blendState3, blendState7);

    public static bool operator ==(BlendState left, BlendState right) => left.Equals(right);
    public static bool operator !=(BlendState left, BlendState right) => !left.Equals(right);
}

/// <summary>Depth write and depth test. Plain data.</summary>
public struct DepthState : IEquatable<DepthState>
{
    public bool writeEnabled { get; set; }
    public CompareFunction compareFunction { get; set; }

    public DepthState(bool writeEnabled = true, CompareFunction compareFunction = CompareFunction.Less)
    {
        this.writeEnabled = writeEnabled;
        this.compareFunction = compareFunction;
    }

    /// <summary>Depth writes on, compare Less.</summary>
    public static DepthState defaultValue => new(true, CompareFunction.Less);

    public readonly bool Equals(DepthState other) => writeEnabled == other.writeEnabled && compareFunction == other.compareFunction;
    public override readonly bool Equals(object? obj) => obj is DepthState other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(writeEnabled, compareFunction);
    public static bool operator ==(DepthState left, DepthState right) => left.Equals(right);
    public static bool operator !=(DepthState left, DepthState right) => !left.Equals(right);
}

/// <summary>Culling, depth bias and depth clip. Plain data.</summary>
public struct RasterState : IEquatable<RasterState>
{
    public CullMode cullingMode { get; set; }
    public bool depthClip { get; set; }
    public int offsetUnits { get; set; }
    public float offsetFactor { get; set; }

    public RasterState(CullMode cullingMode = CullMode.Back, int offsetUnits = 0, float offsetFactor = 0f, bool depthClip = true)
    {
        this.cullingMode = cullingMode;
        this.depthClip = depthClip;
        this.offsetUnits = offsetUnits;
        this.offsetFactor = offsetFactor;
    }

    /// <summary>Cull back faces, no depth bias, depth clip on.</summary>
    public static RasterState defaultValue => new(CullMode.Back, 0, 0f, true);

    public readonly bool Equals(RasterState other) =>
        cullingMode == other.cullingMode && depthClip == other.depthClip && offsetUnits == other.offsetUnits && offsetFactor.Equals(other.offsetFactor);

    public override readonly bool Equals(object? obj) => obj is RasterState other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(cullingMode, depthClip, offsetUnits, offsetFactor);
    public static bool operator ==(RasterState left, RasterState right) => left.Equals(right);
    public static bool operator !=(RasterState left, RasterState right) => !left.Equals(right);
}

/// <summary>
/// Stencil test and operations for front and back faces. The constructor sets the same compare function and
/// operations for both faces. Plain data.
/// </summary>
public struct StencilState : IEquatable<StencilState>
{
    public bool enabled { get; set; }
    public byte readMask { get; set; }
    public byte writeMask { get; set; }
    public CompareFunction compareFunctionFront { get; set; }
    public StencilOp passOperationFront { get; set; }
    public StencilOp failOperationFront { get; set; }
    public StencilOp zFailOperationFront { get; set; }
    public CompareFunction compareFunctionBack { get; set; }
    public StencilOp passOperationBack { get; set; }
    public StencilOp failOperationBack { get; set; }
    public StencilOp zFailOperationBack { get; set; }

    public StencilState(bool enabled = false, byte readMask = 255, byte writeMask = 255, CompareFunction compareFunction = CompareFunction.Always,
        StencilOp passOperation = StencilOp.Keep, StencilOp failOperation = StencilOp.Keep, StencilOp zFailOperation = StencilOp.Keep)
    {
        this.enabled = enabled;
        this.readMask = readMask;
        this.writeMask = writeMask;
        compareFunctionFront = compareFunctionBack = compareFunction;
        passOperationFront = passOperationBack = passOperation;
        failOperationFront = failOperationBack = failOperation;
        zFailOperationFront = zFailOperationBack = zFailOperation;
    }

    /// <summary>Disabled, masks 255, compare Always, every operation Keep.</summary>
    public static StencilState defaultValue =>
        new(false, 255, 255, CompareFunction.Always, StencilOp.Keep, StencilOp.Keep, StencilOp.Keep);

    /// <summary>Sets the compare function of both faces.</summary>
    public void SetCompareFunction(CompareFunction value) => compareFunctionFront = compareFunctionBack = value;

    /// <summary>Sets the pass operation of both faces.</summary>
    public void SetPassOperation(StencilOp value) => passOperationFront = passOperationBack = value;

    /// <summary>Sets the fail operation of both faces.</summary>
    public void SetFailOperation(StencilOp value) => failOperationFront = failOperationBack = value;

    /// <summary>Sets the depth-fail operation of both faces.</summary>
    public void SetZFailOperation(StencilOp value) => zFailOperationFront = zFailOperationBack = value;

    public readonly bool Equals(StencilState other) =>
        enabled == other.enabled && readMask == other.readMask && writeMask == other.writeMask
        && compareFunctionFront == other.compareFunctionFront && passOperationFront == other.passOperationFront
        && failOperationFront == other.failOperationFront && zFailOperationFront == other.zFailOperationFront
        && compareFunctionBack == other.compareFunctionBack && passOperationBack == other.passOperationBack
        && failOperationBack == other.failOperationBack && zFailOperationBack == other.zFailOperationBack;

    public override readonly bool Equals(object? obj) => obj is StencilState other && Equals(other);

    public override readonly int GetHashCode() => HashCode.Combine(
        HashCode.Combine(enabled, readMask, writeMask), HashCode.Combine(compareFunctionFront, passOperationFront, failOperationFront, zFailOperationFront),
        HashCode.Combine(compareFunctionBack, passOperationBack, failOperationBack, zFailOperationBack));

    public static bool operator ==(StencilState left, StencilState right) => left.Equals(right);
    public static bool operator !=(StencilState left, StencilState right) => !left.Equals(right);
}

/// <summary>
/// A set of render states to override, chosen by <see cref="mask"/>. Plain data; the engine does not apply it yet.
/// A new block holds the default state of each kind and stencil reference 0.
/// </summary>
public struct RenderStateBlock : IEquatable<RenderStateBlock>
{
    public BlendState blendState { get; set; }
    public RasterState rasterState { get; set; }
    public DepthState depthState { get; set; }
    public StencilState stencilState { get; set; }
    public int stencilReference { get; set; }
    public RenderStateMask mask { get; set; }

    public RenderStateBlock(RenderStateMask mask)
    {
        this.mask = mask;
        blendState = BlendState.defaultValue;
        rasterState = RasterState.defaultValue;
        depthState = DepthState.defaultValue;
        stencilState = StencilState.defaultValue;
        stencilReference = 0;
    }

    public readonly bool Equals(RenderStateBlock other) =>
        blendState == other.blendState && rasterState == other.rasterState && depthState == other.depthState
        && stencilState == other.stencilState && stencilReference == other.stencilReference && mask == other.mask;

    public override readonly bool Equals(object? obj) => obj is RenderStateBlock other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(blendState, rasterState, depthState, stencilState, stencilReference, mask);
    public static bool operator ==(RenderStateBlock left, RenderStateBlock right) => left.Equals(right);
    public static bool operator !=(RenderStateBlock left, RenderStateBlock right) => !left.Equals(right);
}
