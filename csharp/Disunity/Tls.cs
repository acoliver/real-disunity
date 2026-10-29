namespace Unity.TLS.LowLevel;

/// <summary>
/// The data types of Unity's TLS library, which Unity Transport embeds in its job data. Unity
/// declares them internal and Transport reaches them through <c>InternalsVisibleTo</c>; the
/// engine declares them public so its module facade can forward them. The library's
/// functions are not implemented: a game that opens a secured connection fails naming the
/// first one it calls.
/// </summary>
public static unsafe class Binding
{
    public struct unitytls_errorstate
    {
        public uint magic;
        public uint code;
        public ulong reserved;
    }

    public struct unitytls_dataRef
    {
        public byte* dataPtr;
        public UIntPtr dataLen;
    }

    public struct unitytls_client_config
    {
        public unitytls_dataRef caPEM;
        public unitytls_dataRef serverPEM;
        public unitytls_dataRef privateKeyPEM;
        public uint clientAuth;
        public uint transportProtocol;
        public unitytls_dataRef psk;
        public unitytls_dataRef pskIdentity;
        public IntPtr onDataCB;
        public IntPtr dataSendCB;
        public IntPtr dataReceiveCB;
        public IntPtr dataReceiveTimeoutCB;
        public IntPtr transportUserData;
        public IntPtr applicationUserData;
        public int handshakeReturnsOnStep;
        public int handshakeReturnsIfWouldBlock;
        public uint ssl_read_timeout_ms;
        public byte* hostname;
        public uint tracelevel;
        public IntPtr logCallback;
        public uint ssl_handshake_timeout_min;
        public uint ssl_handshake_timeout_max;
        public ushort mtu;
    }

    public struct unitytls_client
    {
        public uint role;
        public uint state;
        public uint handshakeState;
        public IntPtr ctx;
        public unitytls_client_config* config;
        public IntPtr internalCtx;
    }

    public delegate int unitytls_client_data_send_callback(IntPtr userData, byte* data, UIntPtr dataLen);
    public delegate int unitytls_client_data_receive_callback(IntPtr userData, byte* data, UIntPtr dataLen);
    public delegate int unitytls_client_data_receive_timeout_callback(IntPtr userData, byte* data, UIntPtr dataLen, uint timeoutMS);
    public delegate void unitytls_client_log_callback(int level, byte* file, UIntPtr line, byte* function, byte* message, UIntPtr messageLen);
}
