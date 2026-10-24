# G15 — Application, platform, screen, preferences, files

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** No exact Unity 6.3 guarantee. All code/signatures are authored illustrative subsets from memory, not full definitions or compile-tested examples. See U001–U003 and U046–U048.

**Boundary:** engine Application/Screen/SystemInfo/PlayerPrefs/Debug facilities plus ordinary .NET filesystem APIs. Platform-specific services, permissions, player build settings and supported .NET profiles are separate contracts. The editor running on a platform is not equivalent to that platform's built player.

## G15.T01 — Application lifecycle and platform

Familiar `Application` properties: isPlaying, isEditor, isFocused, isBatchMode, platform, version, companyName, productName, identifier, unityVersion, targetFrameRate, runInBackground, backgroundLoadingPriority, internetReachability and paths covered below.

```csharp
static void Quit();
static void Quit(int exitCode);
static void OpenURL(string url);
// Remembered events, delegate declarations abbreviated.
Application.quitting;
Application.wantsToQuit;
Application.focusChanged;
Application.logMessageReceived;
Application.logMessageReceivedThreaded;
```

Quit is a player lifecycle request and historically has no equivalent effect on editor play mode without editor APIs. Mobile/web/console platforms may impose different quit behavior. `wantsToQuit` callbacks can veto on supported paths; process termination, crashes or OS kills are not guaranteed to run orderly callbacks. Save important data before relying on OnApplicationQuit. Focus and pause are separate observations, particularly for mobile keyboards, browser tabs and background execution.

`RuntimePlatform` has names such as WindowsPlayer, OSXPlayer, LinuxPlayer, WindowsEditor, OSXEditor, LinuxEditor, Android, IPhonePlayer, WebGLPlayer and platform-specific console entries; exact current inventory and obsolete values are U046. Compile-time symbols (`UNITY_EDITOR`, `UNITY_ANDROID`, `UNITY_IOS`, `UNITY_STANDALONE`, etc.) are not the same as runtime platform checks. IL2CPP versus Mono and editor versus player affect reflection, AOT code generation, stripping and supported .NET behavior.

`NetworkReachability.NotReachable`, `ReachableViaCarrierDataNetwork`, `ReachableViaLocalAreaNetwork` reports a reachability category, not proof that an application endpoint is available, authenticated or reachable through a captive portal. `OpenURL` delegates to the platform and is security-sensitive when given untrusted schemes/paths.

## G15.T02 — Screen, display, quality

```csharp
// Selected static Screen shapes.
static int width { get; }
static int height { get; }
static float dpi { get; }
static bool fullScreen { get; set; }
static FullScreenMode fullScreenMode { get; set; }
static Resolution currentResolution { get; }
static Resolution[] resolutions { get; }
static Rect safeArea { get; }
static void SetResolution(int width, int height, bool fullscreen);
static void SetResolution(int width, int height, FullScreenMode fullscreenMode);
```

Refresh-rate overloads and `Resolution.refreshRateRatio`/`RefreshRate` forms coexist with older integer-rate forms across versions; exact preferred signatures are U047. `FullScreenMode.ExclusiveFullScreen`, `FullScreenWindow`, `MaximizedWindow`, `Windowed` are remembered names with platform-specific support. Resolution changes may commit at a later frame boundary and the OS can choose a different supported mode. Do not promise an immediate exact width/height assignment.

Screen.orientation uses `ScreenOrientation` values such as Portrait, PortraitUpsideDown, LandscapeLeft, LandscapeRight and AutoRotation; autorotate permission flags govern permitted orientations. SafeArea is screen-space information for notches/cutouts/system UI, not a replacement for all layout margins. DPI can be missing or unreliable. `Display.displays`, main, rendering/system dimensions and Activate families support multi-display configurations, with substantial platform differences.

`QualitySettings` exposes quality-level selection, names, vSyncCount, antiAliasing, shadows and other rendering settings. `Application.targetFrameRate` is a target, not a real-time guarantee; vSync and platform policies can override or quantize it. Setting antiAliasing does not force every pipeline/render target to use the same MSAA. Pipeline assets and quality tiers interact with URP/HDRP.

## G15.T03 — PlayerPrefs

```csharp
static void SetInt(string key, int value);
static int GetInt(string key, int defaultValue);
static void SetFloat(string key, float value);
static float GetFloat(string key, float defaultValue);
static void SetString(string key, string value);
static string GetString(string key, string defaultValue);
static bool HasKey(string key);
static void DeleteKey(string key);
static void DeleteAll();
static void Save();
```

Single-key getter overloads have defaults in familiar usage; exact default signatures are not a full metadata declaration here. PlayerPrefs stores only the supported primitive preference categories. It is not a general object database, an encrypted vault, a transactional save-game format, or guaranteed durable storage on every write. Save flushes pending changes according to platform implementation and can be expensive. Automatic quit-time save cannot protect against every crash/kill.

Storage backing differs by platform (registry/preferences/files/browser storage are familiar examples); exact paths, hashing and quotas are U048. Key collisions, type mismatches and value limits should be tested for the selected implementation. Never store bearer tokens, passwords or anti-cheat secrets assuming PlayerPrefs provides confidentiality. DeleteAll is global to the application preference namespace, not a safe substitute for deleting one settings section.

## G15.T04 — Persistent paths and filesystem

Important paths: `Application.dataPath`, `persistentDataPath`, `streamingAssetsPath`, `temporaryCachePath`, and `consoleLogPath`. In recalled behavior, consoleLogPath identifies the Editor or player log file on supported platforms; it is not editor-only and is not a general application-data directory. Its availability and value remain platform-dependent. These are platform-dependent strings, not interchangeable writable directories. persistentDataPath is the intended durable application-data location where supported. dataPath often refers into a built application package and may be read-only. temporaryCachePath is disposable cache, not a save location.

StreamingAssets preserves special build content but is not uniformly a normal directory: Android archive/JAR and web URL-like locations may require UnityWebRequest or platform loading rather than `File.ReadAllBytes`. `Path.Combine` is a filesystem path operation, not a URL builder. Do not prepend `file://` mechanically to every path and assume the result is valid on all platforms.

Ordinary `System.IO.File`, `Directory`, `Path`, `FileStream`, and text/JSON libraries are .NET APIs subject to the player's runtime/profile/platform restrictions. For a robust application save: serialize versioned application data, validate it on read, write a temporary file in the same target filesystem, flush as appropriate, and replace/rename using supported atomic semantics. This is application design advice, not a Unity guarantee of crash-proof atomic persistence. Web storage synchronization, sandbox permissions, low disk space, encoding and concurrent writers require separate handling.

Untrusted filenames must not escape the intended storage root via traversal, rooted paths or symlinks. Asset loading keys and URLs should not be accepted directly as OS paths without an explicit trust boundary.

## G15.T05 — SystemInfo and capability checks

Recalled `SystemInfo` observations include operatingSystem, deviceModel, deviceName, deviceType, deviceUniqueIdentifier, processorType, processorCount, systemMemorySize, graphicsDeviceName, graphicsDeviceVendor, graphicsDeviceType, graphicsMemorySize, graphicsShaderLevel, maxTextureSize, supportsComputeShaders and supportsInstancing.

`SupportsTextureFormat(TextureFormat)`, `SupportsRenderTextureFormat(RenderTextureFormat)`, and newer graphics-format support tests (`IsFormatSupported` with usage types) are useful families. Exact namespace/type names for usage enums vary and are U047. `SystemInfo.supportsAsyncGPUReadback`, graphicsUVStartsAtTop and usesReversedZBuffer are remembered graphics capability properties, not substitutes for backend-specific tests.

DeviceUniqueIdentifier can be unavailable, resettable, permission-limited, scoped, or privacy-sensitive. It is not a cryptographic identity or a universally stable account key. Reported memory is an estimate/capability observation, not permission to allocate all of it. Capability detection should guide fallback choices and still handle runtime allocation/device failure.

Permission APIs such as `Application.RequestUserAuthorization(UserAuthorization)` and Android-specific permission helpers are platform-dependent. Presence of a sensor/microphone does not imply permission to use it. Requesting permission can suspend interaction and complete asynchronously.

## G15.T06 — Logging, environment, platform boundaries

`Debug.Log(object)`, `LogWarning(object)`, `LogError(object)`, `LogException(Exception)`, context-object overloads, assertions and `Debug.DrawLine`/`DrawRay` are common. `Debug.isDebugBuild`, `Debug.unityLogger`, `ILogger`, `ILogHandler`, and `LogType.Error`/`Assert`/`Warning`/`Log`/`Exception` describe logging. Draw helpers are diagnostic rendering, not persistent geometry or a guaranteed player HUD.

`Application.logMessageReceivedThreaded` can run from non-main threads; handlers must be thread-safe and avoid recursive logging loops. Redact credentials, user data, URLs with secrets and raw server responses where appropriate. Exact stack traces, formatting, duplicate suppression and console presentation are editor/backend concerns, not portable API contracts.

`Application.lowMemory` and platform suspend/resume signals require quick, bounded handlers. A low-memory notification is not a promise that an allocation will succeed afterward. Environment variables, process spawning, unrestricted sockets and reflection can differ radically between desktop, WebGL, mobile and consoles.

Proposed tests (not executed): preference save/reload and missing defaults; safe path composition; read-only dataPath handling; streaming content from non-filesystem locations; deferred resolution changes; pause/focus distinction; target-frame-rate fallback; missing device capability; and thread-safe log handling. OS storage layout, permission UX, exact platform enum inventory and backend-specific .NET support remain unspecified.
