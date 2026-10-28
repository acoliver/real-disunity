namespace UnityEngine;

// The values of Unity's own enums, which players compile against.

public enum StackTraceLogType
{
    None = 0,
    ScriptOnly = 1,
    Full = 2,
}

public enum NetworkReachability
{
    NotReachable = 0,
    ReachableViaCarrierDataNetwork = 1,
    ReachableViaLocalAreaNetwork = 2,
}

public enum UserAuthorization
{
    WebCam = 1,
    Microphone = 2,
}

public enum ApplicationInstallMode
{
    Unknown = 0,
    Store = 1,
    DeveloperBuild = 2,
    Adhoc = 3,
    Enterprise = 4,
    Editor = 5,
}

public enum ApplicationSandboxType
{
    Unknown = 0,
    NotSandboxed = 1,
    Sandboxed = 2,
    SandboxBroken = 3,
}

public enum RuntimePlatform
{
    OSXEditor = 0,
    OSXPlayer = 1,
    WindowsPlayer = 2,
    OSXWebPlayer = 3,
    OSXDashboardPlayer = 4,
    WindowsWebPlayer = 5,
    WindowsEditor = 7,
    IPhonePlayer = 8,
    XBOX360 = 10,
    PS3 = 9,
    Android = 11,
    NaCl = 12,
    FlashPlayer = 15,
    LinuxPlayer = 13,
    LinuxEditor = 16,
    WebGLPlayer = 17,
    MetroPlayerX86 = 18,
    WSAPlayerX86 = 18,
    MetroPlayerX64 = 19,
    WSAPlayerX64 = 19,
    MetroPlayerARM = 20,
    WSAPlayerARM = 20,
    WP8Player = 21,
    BlackBerryPlayer = 22,
    TizenPlayer = 23,
    PSP2 = 24,
    PS4 = 25,
    PSM = 26,
    XboxOne = 27,
    SamsungTVPlayer = 28,
    WiiU = 30,
    tvOS = 31,
    Switch = 32,
    Lumin = 33,
    Stadia = 34,
    CloudRendering = -1,
    LinuxHeadlessSimulation = 35,
    GameCoreScarlett = -1,
    GameCoreXboxSeries = 36,
    GameCoreXboxOne = 37,
    PS5 = 38,
    EmbeddedLinuxArm64 = 39,
    EmbeddedLinuxArm32 = 40,
    EmbeddedLinuxX64 = 41,
    EmbeddedLinuxX86 = 42,
    LinuxServer = 43,
    WindowsServer = 44,
    OSXServer = 45,
    QNXArm32 = 46,
    QNXArm64 = 47,
    QNXX64 = 48,
    QNXX86 = 49,
    VisionOS = 50,
    Switch2 = 51,
    KeplerArm64 = 52,
    KeplerX64 = 53,
}

public enum SystemLanguage
{
    Afrikaans = 0,
    Arabic = 1,
    Basque = 2,
    Belarusian = 3,
    Bulgarian = 4,
    Catalan = 5,
    Chinese = 6,
    Czech = 7,
    Danish = 8,
    Dutch = 9,
    English = 10,
    Estonian = 11,
    Faroese = 12,
    Finnish = 13,
    French = 14,
    German = 15,
    Greek = 16,
    Hebrew = 17,
    Icelandic = 19,
    Indonesian = 20,
    Italian = 21,
    Japanese = 22,
    Korean = 23,
    Latvian = 24,
    Lithuanian = 25,
    Norwegian = 26,
    Polish = 27,
    Portuguese = 28,
    Romanian = 29,
    Russian = 30,
    SerboCroatian = 31,
    Slovak = 32,
    Slovenian = 33,
    Spanish = 34,
    Swedish = 35,
    Thai = 36,
    Turkish = 37,
    Ukrainian = 38,
    Vietnamese = 39,
    ChineseSimplified = 40,
    ChineseTraditional = 41,
    Hindi = 42,
    Unknown = 43,
    Hungarian = 18,
}

public enum LogType
{
    Error = 0,
    Assert = 1,
    Warning = 2,
    Log = 3,
    Exception = 4,
}

public enum LogOption
{
    None = 0,
    NoStacktrace = 1,
}

public enum ThreadPriority
{
    Low = 0,
    BelowNormal = 1,
    Normal = 2,
    High = 4,
}
