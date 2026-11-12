namespace UnityEngine;

[Flags]
public enum HideFlags
{
    None = 0,
    HideInHierarchy = 1,
    HideInInspector = 2,
    DontSaveInEditor = 4,
    NotEditable = 8,
    DontSaveInBuild = 16,
    DontUnloadUnusedAsset = 32,
    DontSave = DontSaveInEditor | DontSaveInBuild | DontUnloadUnusedAsset,
    HideAndDontSave = HideInHierarchy | DontSave | NotEditable,
}

public enum FindObjectsInactive
{
    Exclude = 0,
    Include = 1,
}

public enum SendMessageOptions
{
    RequireReceiver = 0,
    DontRequireReceiver = 1,
}

public enum PrimitiveType
{
    Sphere = 0,
    Capsule = 1,
    Cylinder = 2,
    Cube = 3,
    Plane = 4,
    Quad = 5,
}

public enum Space
{
    World = 0,
    Self = 1,
}

[Flags]
public enum DrivenTransformProperties
{
    None = 0,
    All = -1,
    AnchoredPositionX = 1 << 1,
    AnchoredPositionY = 1 << 2,
    AnchoredPositionZ = 1 << 3,
    Rotation = 1 << 4,
    ScaleX = 1 << 5,
    ScaleY = 1 << 6,
    ScaleZ = 1 << 7,
    AnchorMinX = 1 << 8,
    AnchorMinY = 1 << 9,
    AnchorMaxX = 1 << 10,
    AnchorMaxY = 1 << 11,
    SizeDeltaX = 1 << 12,
    SizeDeltaY = 1 << 13,
    PivotX = 1 << 14,
    PivotY = 1 << 15,
    AnchoredPosition = AnchoredPositionX | AnchoredPositionY,
    AnchoredPosition3D = AnchoredPosition | AnchoredPositionZ,
    Scale = ScaleX | ScaleY | ScaleZ,
    AnchorMin = AnchorMinX | AnchorMinY,
    AnchorMax = AnchorMaxX | AnchorMaxY,
    Anchors = AnchorMin | AnchorMax,
    SizeDelta = SizeDeltaX | SizeDeltaY,
    Pivot = PivotX | PivotY,
}

[Flags]
public enum AdditionalCanvasShaderChannels
{
    None = 0,
    TexCoord1 = 1,
    TexCoord2 = 2,
    TexCoord3 = 4,
    Normal = 8,
    Tangent = 16,
}

public enum FullScreenMode
{
    ExclusiveFullScreen = 0,
    FullScreenWindow = 1,
    MaximizedWindow = 2,
    Windowed = 3,
}

public enum CursorLockMode
{
    None = 0,
    Locked = 1,
    Confined = 2,
}

public enum CursorMode
{
    Auto = 0,
    ForceSoftware = 1,
}

public enum DeviceType
{
    Unknown = 0,
    Handheld = 1,
    Console = 2,
    Desktop = 3,
}

public enum OperatingSystemFamily
{
    Other = 0,
    MacOSX = 1,
    Windows = 2,
    Linux = 3,
}

public enum BatteryStatus
{
    Unknown = 0,
    Charging = 1,
    Discharging = 2,
    NotCharging = 3,
    Full = 4,
}

public enum TouchPhase
{
    Began = 0,
    Moved = 1,
    Stationary = 2,
    Ended = 3,
    Canceled = 4,
}

public enum TouchType
{
    Direct = 0,
    Indirect = 1,
    Stylus = 2,
}

/// <summary>What a pen on a tablet is doing. Player code names it in pointer event fields; the host reports no pen.</summary>
[Flags]
public enum PenStatus
{
    None = 0,
    Contact = 1,
    Barrel = 2,
    Inverted = 4,
    Eraser = 8,
}

public enum IMECompositionMode
{
    Auto = 0,
    On = 1,
    Off = 2,
}

public enum TouchScreenKeyboardType
{
    Default = 0,
    ASCIICapable = 1,
    NumbersAndPunctuation = 2,
    URL = 3,
    NumberPad = 4,
    PhonePad = 5,
    NamePhonePad = 6,
    EmailAddress = 7,
    NintendoNetworkAccount = 8,
    Social = 9,
    Search = 10,
    DecimalPad = 11,
}

public enum EventType
{
    MouseDown = 0,
    MouseUp = 1,
    MouseMove = 2,
    MouseDrag = 3,
    KeyDown = 4,
    KeyUp = 5,
    ScrollWheel = 6,
    Repaint = 7,
    Layout = 8,
    DragUpdated = 9,
    DragPerform = 10,
    Ignore = 11,
    Used = 12,
    ValidateCommand = 13,
    ExecuteCommand = 14,
    DragExited = 15,
    ContextClick = 16,
    MouseEnterWindow = 20,
    MouseLeaveWindow = 21,
}

[Flags]
public enum EventModifiers
{
    None = 0,
    Shift = 1,
    Control = 2,
    Alt = 4,
    Command = 8,
    Numeric = 16,
    CapsLock = 32,
    FunctionKey = 64,
}

public enum QueryTriggerInteraction
{
    UseGlobal = 0,
    Ignore = 1,
    Collide = 2,
}

public enum WrapMode
{
    Default = 0,
    Once = 1,
    Clamp = 1,
    Loop = 2,
    PingPong = 4,
    ClampForever = 8,
}

[Flags]
public enum WeightedMode
{
    None = 0,
    In = 1,
    Out = 2,
    Both = In | Out,
}

public enum GradientMode
{
    Blend = 0,
    Fixed = 1,
    PerceptualBlend = 2,
}

public enum AnimatorCullingMode
{
    AlwaysAnimate = 0,
    CullUpdateTransforms = 1,
    CullCompletely = 2,
}

public enum AnimatorUpdateMode
{
    Normal = 0,
    Fixed = 1,
    AnimatePhysics = 1,
    UnscaledTime = 2,
}

public enum DurationUnit
{
    Fixed = 0,
    Normalized = 1,
}

public enum AvatarMaskBodyPart
{
    Root = 0,
    Body = 1,
    Head = 2,
    LeftLeg = 3,
    RightLeg = 4,
    LeftArm = 5,
    RightArm = 6,
    LeftFingers = 7,
    RightFingers = 8,
    LeftFootIK = 9,
    RightFootIK = 10,
    LeftHandIK = 11,
    RightHandIK = 12,
    LastBodyPart = 13,
}

public enum ParticleSystemAnimationType
{
    WholeSheet = 0,
    SingleRow = 1,
}

public enum ParticleSystemScalingMode
{
    Hierarchy = 0,
    Local = 1,
    Shape = 2,
}

public enum ParticleSystemSimulationSpace
{
    Local = 0,
    World = 1,
    Custom = 2,
}

public enum ParticleSystemStopBehavior
{
    StopEmittingAndClear = 0,
    StopEmitting = 1,
}
