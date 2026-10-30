namespace UnityEngine.Rendering;

/// <summary>The stencil test and operations of a draw, with one set for front faces and one for back faces.
/// Setting an operation or comparison without a face sets both. The default has the test disabled, masks of
/// 255, <see cref="CompareFunction.Always"/> and <see cref="StencilOp.Keep"/>. It holds the values only; no
/// draw uses them yet.</summary>
public struct StencilState : IEquatable<StencilState>
{
    // The all-zero struct is the default, so values whose default is not zero are stored exclusive-or their default.
    private const int AlwaysBits = (int)CompareFunction.Always;

    private bool _enabled;
    private byte _readMaskBits;
    private byte _writeMaskBits;
    private byte _compareFrontBits;
    private byte _compareBackBits;
    private StencilOp _passFront;
    private StencilOp _passBack;
    private StencilOp _failFront;
    private StencilOp _failBack;
    private StencilOp _zFailFront;
    private StencilOp _zFailBack;

    /// <summary>The state a new <see cref="StencilState"/> starts as.</summary>
    public static StencilState defaultValue => default;

    public bool enabled { get => _enabled; set => _enabled = value; }

    public byte readMask { get => (byte)(_readMaskBits ^ 255); set => _readMaskBits = (byte)(value ^ 255); }

    public byte writeMask { get => (byte)(_writeMaskBits ^ 255); set => _writeMaskBits = (byte)(value ^ 255); }

    public CompareFunction compareFunctionFront { get => (CompareFunction)(_compareFrontBits ^ AlwaysBits); set => _compareFrontBits = (byte)((int)value ^ AlwaysBits); }

    public CompareFunction compareFunctionBack { get => (CompareFunction)(_compareBackBits ^ AlwaysBits); set => _compareBackBits = (byte)((int)value ^ AlwaysBits); }

    public StencilOp passOperationFront { get => _passFront; set => _passFront = value; }

    public StencilOp passOperationBack { get => _passBack; set => _passBack = value; }

    public StencilOp failOperationFront { get => _failFront; set => _failFront = value; }

    public StencilOp failOperationBack { get => _failBack; set => _failBack = value; }

    public StencilOp zFailOperationFront { get => _zFailFront; set => _zFailFront = value; }

    public StencilOp zFailOperationBack { get => _zFailBack; set => _zFailBack = value; }

    public void SetCompareFunction(CompareFunction value) => compareFunctionFront = compareFunctionBack = value;

    public void SetPassOperation(StencilOp value) => passOperationFront = passOperationBack = value;

    public void SetFailOperation(StencilOp value) => failOperationFront = failOperationBack = value;

    public void SetZFailOperation(StencilOp value) => zFailOperationFront = zFailOperationBack = value;

    public bool Equals(StencilState other) =>
        (_enabled, _readMaskBits, _writeMaskBits, _compareFrontBits, _compareBackBits) == (other._enabled, other._readMaskBits, other._writeMaskBits, other._compareFrontBits, other._compareBackBits)
        && (_passFront, _passBack, _failFront, _failBack, _zFailFront, _zFailBack) == (other._passFront, other._passBack, other._failFront, other._failBack, other._zFailFront, other._zFailBack);

    public override bool Equals(object? obj) => obj is StencilState other && Equals(other);

    public override int GetHashCode() => HashCode.Combine(_enabled, _readMaskBits, _writeMaskBits, _compareFrontBits, _compareBackBits,
        HashCode.Combine(_passFront, _passBack, _failFront, _failBack, _zFailFront, _zFailBack));

    public static bool operator ==(StencilState left, StencilState right) => left.Equals(right);

    public static bool operator !=(StencilState left, StencilState right) => !left.Equals(right);
}
