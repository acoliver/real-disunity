using System.Net;
using System.Net.Sockets;

namespace Unity.Baselib.LowLevel;

/// <summary>
/// Baselib's registered-buffer UDP sockets, which Unity Transport uses for its connections.
/// A socket is a non-blocking OS UDP socket. Receive and send requests are queued by
/// <c>Schedule*</c>, carried out by <c>Process*</c> and reported by <c>Dequeue*</c>, each
/// queue holding at most the size given at creation. A receive writes the sender's address
/// into the request's endpoint slice. An address holds the IP bytes, the port in network byte
/// order in <c>port0</c> and <c>port1</c>, and the family.
/// </summary>
public static unsafe partial class Binding
{
    public static readonly uint Baselib_RegisteredNetwork_Endpoint_MaxSize = (uint)sizeof(Baselib_NetworkAddress);

    public static Baselib_RegisteredNetwork_Buffer Baselib_RegisteredNetwork_Buffer_Register(
        Baselib_Memory_PageAllocation pageAllocation, Baselib_ErrorState* errorState)
    {
        Succeed(errorState);
        return new Baselib_RegisteredNetwork_Buffer { id = pageAllocation.ptr, allocation = pageAllocation };
    }

    public static void Baselib_RegisteredNetwork_Buffer_Deregister(Baselib_RegisteredNetwork_Buffer buffer)
    {
    }

    public static Baselib_RegisteredNetwork_BufferSlice Baselib_RegisteredNetwork_BufferSlice_Create(
        Baselib_RegisteredNetwork_Buffer buffer, uint offset, uint size) =>
        new() { id = buffer.id, data = buffer.allocation.ptr + (nint)offset, size = size, offset = offset };

    public static Baselib_RegisteredNetwork_BufferSlice Baselib_RegisteredNetwork_BufferSlice_Empty() => default;

    public static Baselib_RegisteredNetwork_Endpoint Baselib_RegisteredNetwork_Endpoint_Create(
        Baselib_NetworkAddress* srcAddress, Baselib_RegisteredNetwork_BufferSlice dstSlice, Baselib_ErrorState* errorState)
    {
        if (dstSlice.data == IntPtr.Zero || dstSlice.size < Baselib_RegisteredNetwork_Endpoint_MaxSize)
        {
            Fail(errorState, Baselib_ErrorCode.InvalidBufferSize);
            return default;
        }
        *(Baselib_NetworkAddress*)dstSlice.data = srcAddress == null ? default : *srcAddress;
        Succeed(errorState);
        return new Baselib_RegisteredNetwork_Endpoint { slice = dstSlice };
    }

    public static Baselib_RegisteredNetwork_Endpoint Baselib_RegisteredNetwork_Endpoint_Empty() => default;

    public static void Baselib_RegisteredNetwork_Endpoint_GetNetworkAddress(Baselib_RegisteredNetwork_Endpoint endpoint,
        Baselib_NetworkAddress* dstAddress, Baselib_ErrorState* errorState)
    {
        if (endpoint.slice.data == IntPtr.Zero || endpoint.slice.size < Baselib_RegisteredNetwork_Endpoint_MaxSize)
        {
            Fail(errorState, Baselib_ErrorCode.InvalidArgument);
            return;
        }
        *dstAddress = *(Baselib_NetworkAddress*)endpoint.slice.data;
        Succeed(errorState);
    }

    public static Baselib_RegisteredNetwork_Socket_UDP Baselib_RegisteredNetwork_Socket_UDP_Create(Baselib_NetworkAddress* bindAddress,
        Baselib_NetworkAddress_AddressReuse endpointReuse, uint sendQueueSize, uint recvQueueSize, Baselib_ErrorState* errorState)
    {
        try
        {
            var socket = UdpSockets.Open(BaselibAddress.ToEndPoint(bindAddress), endpointReuse == Baselib_NetworkAddress_AddressReuse.Allow, sendQueueSize, recvQueueSize);
            Succeed(errorState);
            return new Baselib_RegisteredNetwork_Socket_UDP { handle = socket };
        }
        catch (SocketException e)
        {
            Fail(errorState, BaselibAddress.Code(e), (ulong)e.ErrorCode);
            return Baselib_RegisteredNetwork_Socket_UDP_Invalid;
        }
        catch (ArgumentException)
        {
            Fail(errorState, Baselib_ErrorCode.InvalidAddressFamily);
            return Baselib_RegisteredNetwork_Socket_UDP_Invalid;
        }
    }

    public static uint Baselib_RegisteredNetwork_Socket_UDP_ScheduleRecv(Baselib_RegisteredNetwork_Socket_UDP socket,
        Baselib_RegisteredNetwork_Request* requests, uint requestsCount, Baselib_ErrorState* errorState) =>
        UdpSockets.Schedule(socket.handle, requests, requestsCount, receive: true, errorState);

    public static uint Baselib_RegisteredNetwork_Socket_UDP_ScheduleSend(Baselib_RegisteredNetwork_Socket_UDP socket,
        Baselib_RegisteredNetwork_Request* requests, uint requestsCount, Baselib_ErrorState* errorState) =>
        UdpSockets.Schedule(socket.handle, requests, requestsCount, receive: false, errorState);

    public static Baselib_RegisteredNetwork_ProcessStatus Baselib_RegisteredNetwork_Socket_UDP_ProcessRecv(
        Baselib_RegisteredNetwork_Socket_UDP socket, Baselib_ErrorState* errorState) =>
        UdpSockets.Process(socket.handle, receive: true, errorState);

    public static Baselib_RegisteredNetwork_ProcessStatus Baselib_RegisteredNetwork_Socket_UDP_ProcessSend(
        Baselib_RegisteredNetwork_Socket_UDP socket, Baselib_ErrorState* errorState) =>
        UdpSockets.Process(socket.handle, receive: false, errorState);

    public static Baselib_RegisteredNetwork_CompletionQueueStatus Baselib_RegisteredNetwork_Socket_UDP_WaitForCompletedRecv(
        Baselib_RegisteredNetwork_Socket_UDP socket, uint timeoutInMilliseconds, Baselib_ErrorState* errorState) =>
        UdpSockets.Wait(socket.handle, timeoutInMilliseconds, receive: true, errorState);

    public static Baselib_RegisteredNetwork_CompletionQueueStatus Baselib_RegisteredNetwork_Socket_UDP_WaitForCompletedSend(
        Baselib_RegisteredNetwork_Socket_UDP socket, uint timeoutInMilliseconds, Baselib_ErrorState* errorState) =>
        UdpSockets.Wait(socket.handle, timeoutInMilliseconds, receive: false, errorState);

    public static uint Baselib_RegisteredNetwork_Socket_UDP_DequeueRecv(Baselib_RegisteredNetwork_Socket_UDP socket,
        Baselib_RegisteredNetwork_CompletionResult* results, uint resultsCount, Baselib_ErrorState* errorState) =>
        UdpSockets.Dequeue(socket.handle, results, resultsCount, receive: true, errorState);

    public static uint Baselib_RegisteredNetwork_Socket_UDP_DequeueSend(Baselib_RegisteredNetwork_Socket_UDP socket,
        Baselib_RegisteredNetwork_CompletionResult* results, uint resultsCount, Baselib_ErrorState* errorState) =>
        UdpSockets.Dequeue(socket.handle, results, resultsCount, receive: false, errorState);

    public static void Baselib_RegisteredNetwork_Socket_UDP_GetNetworkAddress(Baselib_RegisteredNetwork_Socket_UDP socket,
        Baselib_NetworkAddress* dstAddress, Baselib_ErrorState* errorState) =>
        UdpSockets.LocalAddress(socket.handle, dstAddress, errorState);

    public static void Baselib_RegisteredNetwork_Socket_UDP_SetIPv4DontFragHeader(Baselib_RegisteredNetwork_Socket_UDP socket,
        bool value, Baselib_ErrorState* errorState) =>
        UdpSockets.DontFragment(socket.handle, value, errorState);

    public static void Baselib_RegisteredNetwork_Socket_UDP_Close(Baselib_RegisteredNetwork_Socket_UDP socket) =>
        UdpSockets.Close(socket.handle);
}
