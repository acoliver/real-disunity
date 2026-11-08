using System.Runtime.InteropServices;

namespace Unity.Baselib.LowLevel;

// Baselib's data types, written from the engine's own knowledge of its C API and checked against
// the player's runtime behaviour. The functions that use them are in the Baselib*.cs files.

public static unsafe partial class Binding
{
    public static readonly Baselib_Memory_PageAllocation Baselib_Memory_PageAllocation_Invalid = default;

    public static readonly Baselib_RegisteredNetwork_Socket_UDP Baselib_RegisteredNetwork_Socket_UDP_Invalid = default;

    public enum Baselib_ErrorCode : uint
    {
        Success = 0,
        InvalidArgument = 0x01000000,
        InvalidBufferSize,
        InvalidAddressFamily,
        InvalidPageSize,
        InvalidPageCount,
        AddressInUse,
        AddressUnreachable,
        AddressFamilyNotSupported,
        UnexpectedError,
    }

    public enum Baselib_ErrorState_NativeErrorCodeType : byte
    {
        None = 0,
        PlatformDefined,
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_ErrorState
    {
        public ulong nativeErrorCode;
        public Baselib_ErrorState_NativeErrorCodeType nativeErrorCodeType;
        public Baselib_ErrorCode code;
    }

    public enum Baselib_Memory_PageState
    {
        ReadOnly = 0x02,
        ReadWrite = 0x04,
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_Memory_PageSizeInfo
    {
        public ulong defaultPageSize;
        public ulong pageSizesLen;
        public ulong pageSizes0;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_Memory_PageAllocation
    {
        public IntPtr ptr;
        public ulong pageSize;
        public ulong pageCount;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_Timer_TickToNanosecondConversionRatio
    {
        public ulong ticksToNanosecondsNumerator;
        public ulong ticksToNanosecondsDenominator;
    }

    public enum Baselib_NetworkAddress_Family : byte
    {
        IPv4 = 1,
        IPv6 = 2,
    }

    public enum Baselib_NetworkAddress_AddressReuse
    {
        DoNotAllow = 0,
        Allow = 1,
    }

    /// <summary>An IP address (16 bytes), a port in network byte order, the family and an IPv6 scope id.</summary>
    [StructLayout(LayoutKind.Explicit, Size = 24)]
    public struct Baselib_NetworkAddress
    {
        [FieldOffset(0)] public byte ipv4_0;
        [FieldOffset(1)] public byte ipv4_1;
        [FieldOffset(2)] public byte ipv4_2;
        [FieldOffset(3)] public byte ipv4_3;
        [FieldOffset(16)] public byte port0;
        [FieldOffset(17)] public byte port1;
        [FieldOffset(18)] public byte family;
        [FieldOffset(20)] public uint ipv6_scope_id;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_Socket_Handle
    {
        public IntPtr handle;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_RegisteredNetwork_Buffer
    {
        public IntPtr id;
        public Baselib_Memory_PageAllocation allocation;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_RegisteredNetwork_BufferSlice
    {
        public IntPtr id;
        public IntPtr data;
        public uint size;
        public uint offset;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_RegisteredNetwork_Endpoint
    {
        public Baselib_RegisteredNetwork_BufferSlice slice;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_RegisteredNetwork_Request
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
    public struct Baselib_RegisteredNetwork_CompletionResult
    {
        public Baselib_RegisteredNetwork_CompletionStatus status;
        public uint bytesTransferred;
        public IntPtr requestUserdata;
    }

    public enum Baselib_RegisteredNetwork_ProcessStatus
    {
        Done = 0,
        Pending = 1,
    }

    public enum Baselib_RegisteredNetwork_CompletionQueueStatus
    {
        NoResultsAvailable = 0,
        ResultsAvailable = 1,
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Baselib_RegisteredNetwork_Socket_UDP
    {
        public IntPtr handle;
    }
}
