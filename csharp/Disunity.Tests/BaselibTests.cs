using System.Diagnostics;
using System.Text;
using static GraphicsTestSupport;
using static Unity.Baselib.LowLevel.Binding;

/// <summary>Baselib page memory, timers and registered-buffer UDP sockets.</summary>
internal static unsafe class BaselibTests
{
    public static void Run()
    {
        Memory();
        Console.WriteLine("PASS: Baselib pages are aligned, zeroed and checked for page size and count");
        Timers();
        Console.WriteLine("PASS: Baselib timers count up from the process's high-resolution clock");
        Loopback();
        QueueLimits();
        Console.WriteLine("PASS: Baselib UDP sockets send and receive through registered buffers over loopback");
        BindingATakenPortIsExplained();
        Console.WriteLine("PASS: binding a port another socket holds fails with AddressInUse, and Baselib explains the error");
    }

    private static void Memory()
    {
        Baselib_Memory_PageSizeInfo info;
        Baselib_Memory_GetPageSizeInfo(&info);
        var page = (ulong)Environment.SystemPageSize;
        Ensure(info.defaultPageSize == page && info.pageSizes0 == page && info.pageSizesLen == 1, "the page size is the system's");
        Baselib_ErrorState error;
        var pages = Baselib_Memory_AllocatePages(page, 3, 2, Baselib_Memory_PageState.ReadWrite, &error);
        Ensure(error.code == Baselib_ErrorCode.Success && pages.pageCount == 3 && pages.pageSize == page, "three pages");
        Ensure((ulong)pages.ptr % (page * 2) == 0, "aligned to two pages");
        Ensure(new ReadOnlySpan<byte>((void*)pages.ptr, (int)(page * 3)).IndexOfAnyExcept((byte)0) < 0, "zeroed");
        Baselib_Memory_SetPageState(pages.ptr, page, 3, Baselib_Memory_PageState.ReadOnly, &error);
        Ensure(error.code == Baselib_ErrorCode.Success, "a page state is accepted");
        Baselib_Memory_ReleasePages(pages, &error);
        Ensure(error.code == Baselib_ErrorCode.Success, "released");
        var bad = Baselib_Memory_AllocatePages(3000, 1, 1, Baselib_Memory_PageState.ReadWrite, &error);
        Ensure(error.code == Baselib_ErrorCode.InvalidPageSize && bad.ptr == IntPtr.Zero, "a page size that is not a power of two");
        Baselib_Memory_AllocatePages(page, 0, 1, Baselib_Memory_PageState.ReadWrite, &error);
        Ensure(error.code == Baselib_ErrorCode.InvalidPageCount, "no pages");
    }

    private static void Timers()
    {
        var ratio = Baselib_Timer_GetTicksToNanosecondsConversionRatio();
        Ensure(ratio.ticksToNanosecondsNumerator == 1_000_000_000 && ratio.ticksToNanosecondsDenominator == (ulong)Stopwatch.Frequency, "ticks convert to nanoseconds");
        var (ticks, seconds) = (Baselib_Timer_GetHighPrecisionTimerTicks(), Baselib_Timer_GetTimeSinceStartupInSeconds());
        Baselib_Timer_WaitForAtLeast(5);
        Ensure(Baselib_Timer_GetHighPrecisionTimerTicks() - ticks >= (ulong)(Stopwatch.Frequency / 1000 * 5), "at least 5 ms of ticks passed");
        Ensure(seconds >= 0 && Baselib_Timer_GetTimeSinceStartupInSeconds() - seconds >= 0.005, "time since startup counts up");
    }

    private sealed class Pages : IDisposable
    {
        public readonly Baselib_RegisteredNetwork_Buffer Buffer;
        public Pages()
        {
            Baselib_ErrorState error;
            var allocation = Baselib_Memory_AllocatePages((ulong)Environment.SystemPageSize, 1, 1, Baselib_Memory_PageState.ReadWrite, &error);
            Buffer = Baselib_RegisteredNetwork_Buffer_Register(allocation, &error);
        }
        public Baselib_RegisteredNetwork_BufferSlice Slice(uint offset, uint size) => Baselib_RegisteredNetwork_BufferSlice_Create(Buffer, offset, size);
        public void Dispose()
        {
            Baselib_RegisteredNetwork_Buffer_Deregister(Buffer);
            Baselib_Memory_ReleasePages(Buffer.allocation, null);
        }
    }

    private static Baselib_RegisteredNetwork_Socket_UDP Bind(uint queueSize = 4)
    {
        Baselib_NetworkAddress loopback = default;
        loopback.ipv4_0 = 127;
        loopback.ipv4_3 = 1;
        loopback.family = (byte)Baselib_NetworkAddress_Family.IPv4;
        Baselib_ErrorState error;
        var socket = Baselib_RegisteredNetwork_Socket_UDP_Create(&loopback, Baselib_NetworkAddress_AddressReuse.DoNotAllow, queueSize, queueSize, &error);
        Ensure(error.code == Baselib_ErrorCode.Success && socket.handle != IntPtr.Zero, "a loopback socket binds");
        Baselib_RegisteredNetwork_Socket_UDP_SetIPv4DontFragHeader(socket, true, &error);
        Ensure(error.code == Baselib_ErrorCode.Success, "the don't-fragment flag is set");
        return socket;
    }

    private static Baselib_NetworkAddress Address(Baselib_RegisteredNetwork_Socket_UDP socket)
    {
        Baselib_NetworkAddress address;
        Baselib_ErrorState error;
        Baselib_RegisteredNetwork_Socket_UDP_GetNetworkAddress(socket, &address, &error);
        Ensure(error.code == Baselib_ErrorCode.Success && address.family == (byte)Baselib_NetworkAddress_Family.IPv4, "a socket reports its IPv4 address");
        Ensure(address.ipv4_0 == 127 && address.ipv4_3 == 1 && (address.port0 | address.port1) != 0, "the loopback address and an assigned port");
        return address;
    }

    private static string Explain(Baselib_ErrorState* error, Baselib_ErrorState_ExplainVerbosity verbosity, uint capacity, out uint needed)
    {
        var buffer = stackalloc byte[64];
        new Span<byte>(buffer, 64).Fill(0xAA);
        needed = Baselib_ErrorState_Explain(error, buffer, capacity, verbosity);
        var length = new ReadOnlySpan<byte>(buffer, 64).IndexOf((byte)0);
        Ensure(capacity == 0 ? buffer[0] == 0xAA : length >= 0 && length < capacity, "the text ends with a zero inside the buffer");
        return capacity == 0 ? "" : Encoding.UTF8.GetString(buffer, length);
    }

    private static void BindingATakenPortIsExplained()
    {
        var holder = Bind();
        var taken = Address(holder);
        Baselib_ErrorState error;
        var second = Baselib_RegisteredNetwork_Socket_UDP_Create(&taken, Baselib_NetworkAddress_AddressReuse.DoNotAllow, 4, 4, &error);
        Ensure(error.code == Baselib_ErrorCode.AddressInUse && second.handle == IntPtr.Zero, "the second bind fails with AddressInUse");
        Ensure(error.nativeErrorCodeType == Baselib_ErrorState_NativeErrorCodeType.PlatformDefined && error.nativeErrorCode != 0, "with the platform's error number");
        var brief = Explain(&error, Baselib_ErrorState_ExplainVerbosity.ErrorType, 64, out var needed);
        Ensure(brief == "AddressInUse" && needed == 13, $"the brief explanation names the code: '{brief}' needs {needed}");
        var full = Explain(&error, Baselib_ErrorState_ExplainVerbosity.ErrorType_SourceLocation_Explanation, 64, out needed);
        Ensure(full == $"AddressInUse (platform error {error.nativeErrorCode})" && needed == full.Length + 1, $"the full one adds the platform error: '{full}'");
        Ensure(Explain(&error, Baselib_ErrorState_ExplainVerbosity.ErrorType, 6, out needed) == "Addre" && needed == 13, "a short buffer is cut and still ends with a zero");
        Ensure(Explain(&error, Baselib_ErrorState_ExplainVerbosity.ErrorType, 0, out needed) == "" && needed == 13, "a zero length writes nothing and gives the length needed");
        var unknown = new Baselib_ErrorState { code = (Baselib_ErrorCode)0x7, nativeErrorCodeType = Baselib_ErrorState_NativeErrorCodeType.None };
        Ensure(Explain(&unknown, Baselib_ErrorState_ExplainVerbosity.ErrorType_SourceLocation_Explanation, 64, out _) == "0x00000007", "an unnamed code is given in hex");
        Baselib_RegisteredNetwork_Socket_UDP_Close(holder);
    }

    private static void Loopback()
    {
        using var pages = new Pages();
        var (sender, receiver) = (Bind(), Bind());
        var (from, to) = (Address(sender), Address(receiver));
        Baselib_ErrorState error;
        var text = Encoding.ASCII.GetBytes("hello");
        var payload = pages.Slice(0, (uint)text.Length);
        text.CopyTo(new Span<byte>((void*)payload.data, text.Length));
        var request = new Baselib_RegisteredNetwork_Request
        {
            payload = payload,
            remoteEndpoint = Baselib_RegisteredNetwork_Endpoint_Create(&to, pages.Slice(64, Baselib_RegisteredNetwork_Endpoint_MaxSize), &error),
            requestUserdata = 7,
        };
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_ScheduleSend(sender, &request, 1, &error) == 1, "a send is scheduled");
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_ProcessSend(sender, &error) == Baselib_RegisteredNetwork_ProcessStatus.Done, "and sent");
        Baselib_RegisteredNetwork_CompletionResult result;
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_DequeueSend(sender, &result, 1, &error) == 1, "its completion");
        Ensure(result.status == Baselib_RegisteredNetwork_CompletionStatus.Success && result.bytesTransferred == 5 && result.requestUserdata == 7, "reports five bytes");
        var receive = new Baselib_RegisteredNetwork_Request
        {
            payload = pages.Slice(128, 64),
            remoteEndpoint = new Baselib_RegisteredNetwork_Endpoint { slice = pages.Slice(256, Baselib_RegisteredNetwork_Endpoint_MaxSize) },
            requestUserdata = 9,
        };
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_ScheduleRecv(receiver, &receive, 1, &error) == 1, "a receive is scheduled");
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_WaitForCompletedRecv(receiver, 2000, &error) == Baselib_RegisteredNetwork_CompletionQueueStatus.ResultsAvailable, "the packet arrives");
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_DequeueRecv(receiver, &result, 1, &error) == 1 && result.bytesTransferred == 5 && result.requestUserdata == 9, "five bytes received");
        Ensure(Encoding.ASCII.GetString(new ReadOnlySpan<byte>((void*)receive.payload.data, 5)) == "hello", "into the payload slice");
        Baselib_NetworkAddress source;
        Baselib_RegisteredNetwork_Endpoint_GetNetworkAddress(receive.remoteEndpoint, &source, &error);
        Ensure(source.port0 == from.port0 && source.port1 == from.port1 && source.ipv4_0 == 127, "the endpoint holds the sender's address");
        Baselib_RegisteredNetwork_Socket_UDP_Close(sender);
        Baselib_RegisteredNetwork_Socket_UDP_Close(receiver);
        Baselib_RegisteredNetwork_Socket_UDP_ProcessSend(sender, &error);
        Ensure(error.code == Baselib_ErrorCode.InvalidArgument, "a closed socket is rejected");
    }

    private static void QueueLimits()
    {
        using var pages = new Pages();
        var socket = Bind(queueSize: 2);
        var requests = stackalloc Baselib_RegisteredNetwork_Request[3];
        for (var i = 0; i < 3; i++) requests[i] = new Baselib_RegisteredNetwork_Request { payload = pages.Slice((uint)i * 64, 64) };
        Baselib_ErrorState error;
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_ScheduleRecv(socket, requests, 3, &error) == 2, "a queue holds at most its size");
        Ensure(Baselib_RegisteredNetwork_Socket_UDP_WaitForCompletedRecv(socket, 0, &error) == Baselib_RegisteredNetwork_CompletionQueueStatus.NoResultsAvailable, "nothing arrived");
        Baselib_RegisteredNetwork_Socket_UDP_Close(socket);
        Baselib_NetworkAddress unknown = default;
        Baselib_RegisteredNetwork_Socket_UDP_Create(&unknown, Baselib_NetworkAddress_AddressReuse.Allow, 1, 1, &error);
        Ensure(error.code == Baselib_ErrorCode.InvalidAddressFamily, "an address with no family is rejected");
    }
}
