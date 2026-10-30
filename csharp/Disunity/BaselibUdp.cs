using System.Net;
using System.Net.Sockets;

namespace Unity.Baselib.LowLevel;

/// <summary>The open registered-buffer UDP sockets, by handle, with their request queues.</summary>
internal static unsafe class UdpSockets
{
    private sealed class Slot(Socket socket, uint sendSize, uint recvSize)
    {
        public readonly Socket Socket = socket;
        public readonly Queues Recv = new(recvSize);
        public readonly Queues Send = new(sendSize);
        public Queues For(bool receive) => receive ? Recv : Send;
    }

    private sealed class Queues(uint size)
    {
        public readonly uint Size = size;
        public readonly Queue<Binding.Baselib_RegisteredNetwork_Request> Pending = new();
        public readonly Queue<Binding.Baselib_RegisteredNetwork_CompletionResult> Done = new();
        public uint Free => Size - (uint)(Pending.Count + Done.Count);
    }

    private static readonly Dictionary<IntPtr, Slot> Sockets = new();
    private static long _next;

    public static IntPtr Open(EndPoint bind, bool reuse, uint sendQueueSize, uint recvQueueSize)
    {
        var socket = new Socket(bind.AddressFamily, SocketType.Dgram, ProtocolType.Udp) { Blocking = false };
        try
        {
            if (reuse) socket.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
            socket.Bind(bind);
        }
        catch
        {
            socket.Dispose();
            throw;
        }
        lock (Sockets)
        {
            var handle = (IntPtr)Interlocked.Increment(ref _next);
            Sockets[handle] = new Slot(socket, Math.Max(sendQueueSize, 1), Math.Max(recvQueueSize, 1));
            return handle;
        }
    }

    public static uint Schedule(IntPtr handle, Binding.Baselib_RegisteredNetwork_Request* requests, uint count, bool receive,
        Binding.Baselib_ErrorState* errorState)
    {
        lock (Sockets)
        {
            if (!Find(handle, errorState, out var slot)) return 0;
            var queues = slot.For(receive);
            var taken = Math.Min(count, queues.Free);
            for (var i = 0; i < taken; i++) queues.Pending.Enqueue(requests[i]);
            Binding.Succeed(errorState);
            return taken;
        }
    }

    public static Binding.Baselib_RegisteredNetwork_ProcessStatus Process(IntPtr handle, bool receive, Binding.Baselib_ErrorState* errorState)
    {
        lock (Sockets)
        {
            if (!Find(handle, errorState, out var slot)) return Binding.Baselib_RegisteredNetwork_ProcessStatus.Done;
            var blocked = receive ? ReceiveAll(slot) : SendAll(slot);
            Binding.Succeed(errorState);
            return !receive && blocked ? Binding.Baselib_RegisteredNetwork_ProcessStatus.Pending : Binding.Baselib_RegisteredNetwork_ProcessStatus.Done;
        }
    }

    public static Binding.Baselib_RegisteredNetwork_CompletionQueueStatus Wait(IntPtr handle, uint timeoutMs, bool receive,
        Binding.Baselib_ErrorState* errorState)
    {
        Socket socket;
        lock (Sockets)
        {
            if (!Find(handle, errorState, out var slot)) return Binding.Baselib_RegisteredNetwork_CompletionQueueStatus.NoResultsAvailable;
            if (slot.For(receive).Done.Count > 0) return Available();
            socket = slot.Socket;
        }
        if (receive && timeoutMs > 0) socket.Poll((int)Math.Min(timeoutMs * 1000L, int.MaxValue), SelectMode.SelectRead);
        Process(handle, receive, errorState);
        lock (Sockets)
            return Find(handle, errorState, out var slot) && slot.For(receive).Done.Count > 0
                ? Available()
                : Binding.Baselib_RegisteredNetwork_CompletionQueueStatus.NoResultsAvailable;
    }

    public static uint Dequeue(IntPtr handle, Binding.Baselib_RegisteredNetwork_CompletionResult* results, uint count, bool receive,
        Binding.Baselib_ErrorState* errorState)
    {
        lock (Sockets)
        {
            if (!Find(handle, errorState, out var slot)) return 0;
            var done = slot.For(receive).Done;
            uint taken = 0;
            while (taken < count && done.TryDequeue(out var result)) results[taken++] = result;
            Binding.Succeed(errorState);
            return taken;
        }
    }

    public static void LocalAddress(IntPtr handle, Binding.Baselib_NetworkAddress* address, Binding.Baselib_ErrorState* errorState)
    {
        lock (Sockets)
        {
            if (!Find(handle, errorState, out var slot)) return;
            BaselibAddress.Write((IPEndPoint)slot.Socket.LocalEndPoint!, address);
            Binding.Succeed(errorState);
        }
    }

    /// <summary>Sets the IPv4 don't-fragment flag; an IPv6 socket never fragments, so it is left as it is.</summary>
    public static void DontFragment(IntPtr handle, bool value, Binding.Baselib_ErrorState* errorState)
    {
        lock (Sockets)
        {
            if (!Find(handle, errorState, out var slot)) return;
            try
            {
                if (slot.Socket.AddressFamily == AddressFamily.InterNetwork) SetDontFragment(slot.Socket, value);
                Binding.Succeed(errorState);
            }
            catch (SocketException e) { Binding.Fail(errorState, BaselibAddress.Code(e), (ulong)e.ErrorCode); }
        }
    }

    // Darwin's IPPROTO_IP option for the don't-fragment bit. .NET's DontFragment option fails on
    // macOS, so the option is set directly there.
    private const int IpProtocolLevel = 0;
    private const int DarwinIpDontFrag = 28;

    private static void SetDontFragment(Socket socket, bool value)
    {
        if (OperatingSystem.IsMacOS())
            socket.SetRawSocketOption(IpProtocolLevel, DarwinIpDontFrag, BitConverter.GetBytes(value ? 1 : 0));
        else
            socket.DontFragment = value;
    }

    public static void Close(IntPtr handle)
    {
        lock (Sockets)
            if (Sockets.Remove(handle, out var slot)) slot.Socket.Dispose();
    }

    private static Binding.Baselib_RegisteredNetwork_CompletionQueueStatus Available() =>
        Binding.Baselib_RegisteredNetwork_CompletionQueueStatus.ResultsAvailable;

    private static bool Find(IntPtr handle, Binding.Baselib_ErrorState* errorState, out Slot slot)
    {
        if (Sockets.TryGetValue(handle, out slot!)) return true;
        Binding.Fail(errorState, Binding.Baselib_ErrorCode.InvalidArgument);
        return false;
    }

    /// <summary>Receives into pending requests until the socket has nothing more. Returns whether it would block.</summary>
    private static bool ReceiveAll(Slot slot)
    {
        var any = new IPEndPoint(slot.Socket.AddressFamily == AddressFamily.InterNetworkV6 ? IPAddress.IPv6Any : IPAddress.Any, 0);
        while (slot.Recv.Pending.TryPeek(out var request))
        {
            EndPoint from = any;
            int received;
            try { received = slot.Socket.ReceiveFrom(Span(request.payload), SocketFlags.None, ref from); }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.WouldBlock) { return true; }
            catch (SocketException) { Complete(slot.Recv, request, 0, success: false); continue; }
            BaselibAddress.Write((IPEndPoint)from, request.remoteEndpoint);
            Complete(slot.Recv, request, received, success: true);
        }
        return false;
    }

    /// <summary>Sends pending requests in order. Returns whether the socket would block.</summary>
    private static bool SendAll(Slot slot)
    {
        while (slot.Send.Pending.TryPeek(out var request))
        {
            int sent;
            try { sent = slot.Socket.SendTo(Span(request.payload), SocketFlags.None, BaselibAddress.ToEndPoint(request.remoteEndpoint)); }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.WouldBlock) { return true; }
            catch (Exception e) when (e is SocketException or ArgumentException) { Complete(slot.Send, request, 0, success: false); continue; }
            Complete(slot.Send, request, sent, success: true);
        }
        return false;
    }

    private static Span<byte> Span(Binding.Baselib_RegisteredNetwork_BufferSlice slice) => new((void*)slice.data, (int)slice.size);

    private static void Complete(Queues queues, Binding.Baselib_RegisteredNetwork_Request request, int bytes, bool success)
    {
        queues.Pending.Dequeue();
        queues.Done.Enqueue(new Binding.Baselib_RegisteredNetwork_CompletionResult
        {
            status = success ? Binding.Baselib_RegisteredNetwork_CompletionStatus.Success : Binding.Baselib_RegisteredNetwork_CompletionStatus.Failed,
            bytesTransferred = (uint)bytes,
            requestUserdata = request.requestUserdata,
        });
    }
}

/// <summary>Converts Baselib network addresses to and from socket endpoints.</summary>
internal static unsafe class BaselibAddress
{
    public static IPEndPoint ToEndPoint(Binding.Baselib_NetworkAddress* address)
    {
        if (address == null) return new IPEndPoint(IPAddress.Any, 0);
        var bytes = new ReadOnlySpan<byte>(address, 16);
        var port = (address->port0 << 8) | address->port1;
        return (Binding.Baselib_NetworkAddress_Family)address->family switch
        {
            Binding.Baselib_NetworkAddress_Family.IPv4 => new IPEndPoint(new IPAddress(bytes[..4]), port),
            Binding.Baselib_NetworkAddress_Family.IPv6 => new IPEndPoint(new IPAddress(bytes, address->ipv6_scope_id), port),
            _ => throw new ArgumentException($"network address family {address->family} is not IPv4 or IPv6"),
        };
    }

    public static IPEndPoint ToEndPoint(Binding.Baselib_RegisteredNetwork_Endpoint endpoint) =>
        endpoint.slice.data == IntPtr.Zero ? throw new ArgumentException("the send request has no remote endpoint")
            : ToEndPoint((Binding.Baselib_NetworkAddress*)endpoint.slice.data);

    public static void Write(IPEndPoint from, Binding.Baselib_RegisteredNetwork_Endpoint endpoint)
    {
        if (endpoint.slice.data != IntPtr.Zero && endpoint.slice.size >= Binding.Baselib_RegisteredNetwork_Endpoint_MaxSize)
            Write(from, (Binding.Baselib_NetworkAddress*)endpoint.slice.data);
    }

    public static void Write(IPEndPoint from, Binding.Baselib_NetworkAddress* address)
    {
        *address = default;
        var ip = from.Address.IsIPv4MappedToIPv6 ? from.Address.MapToIPv4() : from.Address;
        ip.TryWriteBytes(new Span<byte>(address, 16), out _);
        var v6 = ip.AddressFamily == AddressFamily.InterNetworkV6;
        address->family = (byte)(v6 ? Binding.Baselib_NetworkAddress_Family.IPv6 : Binding.Baselib_NetworkAddress_Family.IPv4);
        if (v6) address->ipv6_scope_id = (uint)ip.ScopeId;
        address->port0 = (byte)(from.Port >> 8);
        address->port1 = (byte)from.Port;
    }

    public static Binding.Baselib_ErrorCode Code(SocketException e) => e.SocketErrorCode switch
    {
        SocketError.AddressAlreadyInUse => Binding.Baselib_ErrorCode.AddressInUse,
        SocketError.AddressNotAvailable or SocketError.HostUnreachable or SocketError.NetworkUnreachable => Binding.Baselib_ErrorCode.AddressUnreachable,
        SocketError.AddressFamilyNotSupported => Binding.Baselib_ErrorCode.AddressFamilyNotSupported,
        _ => Binding.Baselib_ErrorCode.UnexpectedError,
    };
}
