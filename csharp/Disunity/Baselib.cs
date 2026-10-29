using System.Runtime.InteropServices;

namespace Unity.Baselib.LowLevel;

// The type declarations below are generated from a Unity 6000.3 player's UnityEngine.CoreModule
// metadata, so their fields, layouts and enum values are Unity's.

/// <summary>
/// The data types of Unity's Baselib platform layer, which Unity Transport uses for sockets,
/// timers and page memory. Unity declares them internal and packages reach them through
/// <c>InternalsVisibleTo</c>; the engine declares them public so its module facade can
/// forward them. Page memory, timers and registered-buffer UDP sockets are implemented
/// (BaselibMemory.cs, BaselibTimer.cs, BaselibNetwork.cs); any other function is missing, so a
/// game fails naming the first one it calls. The invalid handles are the engine's: a socket handle of -1, since 0 is a valid
/// descriptor, and zero for the others.
/// </summary>
public static unsafe partial class Binding
{
    public static readonly UIntPtr Baselib_Memory_MaxAlignment = 65536;
    public static readonly UIntPtr Baselib_Memory_MinGuaranteedAlignment = 16;
    public static readonly IntPtr Baselib_RegisteredNetwork_Buffer_Id_Invalid = IntPtr.Zero;
    public static readonly IntPtr Baselib_Thread_InvalidId = IntPtr.Zero;
    public static readonly UIntPtr Baselib_Thread_MaxThreadNameLength = 64;
    public static readonly Baselib_Memory_PageAllocation Baselib_Memory_PageAllocation_Invalid;
    public static readonly Baselib_RegisteredNetwork_Socket_UDP Baselib_RegisteredNetwork_Socket_UDP_Invalid;
    public static readonly Baselib_Socket_Handle Baselib_Socket_Handle_Invalid = new() { handle = -1 };
    public static readonly Baselib_DynamicLibrary_Handle Baselib_DynamicLibrary_Handle_Invalid;
    public static readonly Baselib_FileIO_EventQueue Baselib_FileIO_EventQueue_Invalid;

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_DynamicLibrary_Handle
    {
        public IntPtr handle;
    }

    public enum Baselib_ErrorCode
    {
        Success = 0,
        OutOfMemory = 16777216,
        OutOfSystemResources = 16777217,
        InvalidAddressRange = 16777218,
        InvalidArgument = 16777219,
        InvalidBufferSize = 16777220,
        InvalidState = 16777221,
        NotSupported = 16777222,
        Timeout = 16777223,
        UnsupportedAlignment = 33554432,
        InvalidPageSize = 33554433,
        InvalidPageCount = 33554434,
        UnsupportedPageState = 33554435,
        ThreadCannotJoinSelf = 50331648,
        NetworkInitializationError = 67108864,
        AddressInUse = 67108865,
        AddressUnreachable = 67108866,
        AddressFamilyNotSupported = 67108867,
        Disconnected = 67108868,
        InvalidSocketType = 67108869,
        InvalidAddressFamily = 67108870,
        InvalidPathname = 83886080,
        RequestedAccessIsNotAllowed = 83886081,
        IOError = 83886082,
        FailedToOpenDynamicLibrary = 100663296,
        FunctionNotFound = 100663297,
        NoSupportedAddressFound = 117440512,
        TryAgain = 117440513,
        UnexpectedError = -1,
    }

    public enum Baselib_ErrorState_NativeErrorCodeType : byte
    {
        None = 0,
        PlatformDefined = 1,
    }

    public enum Baselib_ErrorState_ExtraInformationType : byte
    {
        None = 0,
        StaticString = 1,
        GenerationCounter = 2,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_ErrorState
    {
        public Baselib_SourceLocation sourceLocation;
        public ulong nativeErrorCode;
        public ulong extraInformation;
        public Baselib_ErrorCode code;
        public Baselib_ErrorState_NativeErrorCodeType nativeErrorCodeType;
        public Baselib_ErrorState_ExtraInformationType extraInformationType;
    }

    public enum Baselib_ErrorState_ExplainVerbosity
    {
        ErrorType = 0,
        ErrorType_SourceLocation_Explanation = 1,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_FileIO_EventQueue
    {
        public IntPtr handle;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_FileIO_AsyncFile
    {
        public IntPtr handle;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_FileIO_SyncFile
    {
        public IntPtr handle;
    }

    public enum Baselib_FileIO_OpenFlags : uint
    {
        Read = 1,
        Write = 2,
        OpenAlways = 4,
        CreateAlways = 8,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_FileIO_ReadRequest
    {
        public ulong offset;
        public IntPtr buffer;
        public ulong size;
    }

    public enum Baselib_FileIO_Priority
    {
        Normal = 0,
        High = 1,
    }

    public enum Baselib_FileIO_EventQueue_ResultType
    {
        Baselib_FileIO_EventQueue_Callback = 1,
        Baselib_FileIO_EventQueue_OpenFile = 2,
        Baselib_FileIO_EventQueue_ReadFile = 3,
        Baselib_FileIO_EventQueue_CloseFile = 4,
    }

    public unsafe delegate void EventQueueCallback(ulong arg0);

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_FileIO_EventQueue_Result_Callback
    {
        public IntPtr callback;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_FileIO_EventQueue_Result_OpenFile
    {
        public ulong fileSize;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_FileIO_EventQueue_Result_ReadFile
    {
        public ulong bytesTransferred;
    }

    [StructLayout(LayoutKind.Explicit)]
    public unsafe struct Baselib_FileIO_EventQueue_Result
    {
        [FieldOffset(0)] public Baselib_FileIO_EventQueue_ResultType type;
        [FieldOffset(8)] public ulong userdata;
        [FieldOffset(16)] public Baselib_ErrorState errorState;
        [FieldOffset(64)] public Baselib_FileIO_EventQueue_Result_Callback callback;
        [FieldOffset(64)] public Baselib_FileIO_EventQueue_Result_OpenFile openFile;
        [FieldOffset(64)] public Baselib_FileIO_EventQueue_Result_ReadFile readFile;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_NetworkAddress_HostnameLookupHandle
    {
        public byte _placeholder;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_Memory_PageSizeInfo
    {
        public ulong defaultPageSize;
        public ulong pageSizes0;
        public ulong pageSizes1;
        public ulong pageSizes2;
        public ulong pageSizes3;
        public ulong pageSizes4;
        public ulong pageSizes5;
        public ulong pageSizesLen;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_Memory_PageAllocation
    {
        public IntPtr ptr;
        public ulong pageSize;
        public ulong pageCount;
    }

    public enum Baselib_Memory_PageState
    {
        Reserved = 0,
        NoAccess = 1,
        ReadOnly = 2,
        ReadWrite = 4,
        ReadOnly_Executable = 18,
        ReadWrite_Executable = 20,
    }

    public enum Baselib_NetworkAddress_Family
    {
        Invalid = 0,
        IPv4 = 1,
        IPv6 = 2,
    }

    [StructLayout(LayoutKind.Explicit)]
    public unsafe struct Baselib_NetworkAddress
    {
        [FieldOffset(0)] public byte data0;
        [FieldOffset(1)] public byte data1;
        [FieldOffset(2)] public byte data2;
        [FieldOffset(3)] public byte data3;
        [FieldOffset(4)] public byte data4;
        [FieldOffset(5)] public byte data5;
        [FieldOffset(6)] public byte data6;
        [FieldOffset(7)] public byte data7;
        [FieldOffset(8)] public byte data8;
        [FieldOffset(9)] public byte data9;
        [FieldOffset(10)] public byte data10;
        [FieldOffset(11)] public byte data11;
        [FieldOffset(12)] public byte data12;
        [FieldOffset(13)] public byte data13;
        [FieldOffset(14)] public byte data14;
        [FieldOffset(15)] public byte data15;
        [FieldOffset(0)] public byte ipv6_0;
        [FieldOffset(1)] public byte ipv6_1;
        [FieldOffset(2)] public byte ipv6_2;
        [FieldOffset(3)] public byte ipv6_3;
        [FieldOffset(4)] public byte ipv6_4;
        [FieldOffset(5)] public byte ipv6_5;
        [FieldOffset(6)] public byte ipv6_6;
        [FieldOffset(7)] public byte ipv6_7;
        [FieldOffset(8)] public byte ipv6_8;
        [FieldOffset(9)] public byte ipv6_9;
        [FieldOffset(10)] public byte ipv6_10;
        [FieldOffset(11)] public byte ipv6_11;
        [FieldOffset(12)] public byte ipv6_12;
        [FieldOffset(13)] public byte ipv6_13;
        [FieldOffset(14)] public byte ipv6_14;
        [FieldOffset(15)] public byte ipv6_15;
        [FieldOffset(0)] public byte ipv4_0;
        [FieldOffset(1)] public byte ipv4_1;
        [FieldOffset(2)] public byte ipv4_2;
        [FieldOffset(3)] public byte ipv4_3;
        [FieldOffset(16)] public byte port0;
        [FieldOffset(17)] public byte port1;
        [FieldOffset(18)] public byte family;
        [FieldOffset(19)] public byte _padding;
        [FieldOffset(20)] public uint ipv6_scope_id;
    }

    public enum Baselib_NetworkAddress_AddressReuse
    {
        DoNotAllow = 0,
        Allow = 1,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_RegisteredNetwork_Buffer
    {
        public IntPtr id;
        public Baselib_Memory_PageAllocation allocation;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_RegisteredNetwork_BufferSlice
    {
        public IntPtr id;
        public IntPtr data;
        public uint size;
        public uint offset;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_RegisteredNetwork_Endpoint
    {
        public Baselib_RegisteredNetwork_BufferSlice slice;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_RegisteredNetwork_Request
    {
        public Baselib_RegisteredNetwork_BufferSlice payload;
        public Baselib_RegisteredNetwork_Endpoint remoteEndpoint;
        public IntPtr requestUserdata;
    }

    public enum Baselib_RegisteredNetwork_CompletionStatus
    {
        Failed = 0,
        Success = 1,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_RegisteredNetwork_CompletionResult
    {
        public Baselib_RegisteredNetwork_CompletionStatus status;
        public uint bytesTransferred;
        public IntPtr requestUserdata;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_RegisteredNetwork_Socket_UDP
    {
        public IntPtr handle;
    }

    public enum Baselib_RegisteredNetwork_ProcessStatus
    {
        NonePendingImmediately = 0,
        Done = 0,
        Pending = 1,
    }

    public enum Baselib_RegisteredNetwork_CompletionQueueStatus
    {
        NoResultsAvailable = 0,
        ResultsAvailable = 1,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_Socket_Handle
    {
        public IntPtr handle;
    }

    public enum Baselib_Socket_Protocol
    {
        UDP = 1,
        TCP = 2,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_Socket_Message
    {
        public Baselib_NetworkAddress* address;
        public IntPtr data;
        public uint dataLen;
    }

    public enum Baselib_Socket_PollEvents
    {
        Readable = 1,
        Writable = 2,
        Connected = 4,
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_Socket_PollFd
    {
        public Baselib_Socket_Handle handle;
        public Baselib_Socket_PollEvents requestedEvents;
        public Baselib_Socket_PollEvents resultEvents;
        public Baselib_ErrorState* errorState;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_SourceLocation
    {
        public byte* file;
        public byte* function;
        public uint lineNumber;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_SystemSemaphore_Handle
    {
        public IntPtr handle;
    }

    [StructLayout(LayoutKind.Sequential)]
    public unsafe struct Baselib_Timer_TickToNanosecondConversionRatio
    {
        public ulong ticksToNanosecondsNumerator;
        public ulong ticksToNanosecondsDenominator;
    }

    public enum Baselib_WakeupFallbackStrategy
    {
        OneByOne = 0,
        All = 1,
    }
}
