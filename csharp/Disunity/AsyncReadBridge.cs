using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

namespace Disunity.Hosting;

/// <summary>The native host's scoped file-read callbacks. Hosts install this before running managed code.</summary>
public static unsafe class AsyncReadBridge
{
    private static readonly object ConfigureGate = new();
    private static Session? _current;

    [UnmanagedCallersOnly]
    public static int Configure(nint context, nint callback)
    {
        lock (ConfigureGate)
        {
            _current?.Close();
            Volatile.Write(ref _current, callback == 0 ? null : new Session(context, callback));
        }
        return 0;
    }

    internal static AsyncReadRequest Submit(string path, long offset, int size)
    {
        ArgumentException.ThrowIfNullOrEmpty(path);
        ArgumentOutOfRangeException.ThrowIfNegative(offset);
        ArgumentOutOfRangeException.ThrowIfNegative(size);
        ArgumentOutOfRangeException.ThrowIfGreaterThan(offset, long.MaxValue - size);
        if (Encoding.UTF8.GetByteCount(path) > 4096 || path.Contains('\0')) throw new ArgumentException("file path must fit 4096 UTF-8 bytes and contain no NUL", nameof(path));
        var bytes = Encoding.UTF8.GetBytes(path);
        var session = Volatile.Read(ref _current) ?? throw new InvalidOperationException("the native runtime host has not installed file reads");
        fixed (byte* buffer = bytes)
        {
            var id = session.Call(0, (ulong)offset, buffer, bytes.Length, (ulong)size);
            return new AsyncReadRequest(session, id);
        }
    }

    internal sealed class Session(nint context, nint callback)
    {
        private readonly ReaderWriterLockSlim _gate = new();
        private bool _active = true;
        internal bool Active
        {
            get { _gate.EnterReadLock(); try { return _active; } finally { _gate.ExitReadLock(); } }
        }

        internal long Call(uint operation, ulong id, byte* buffer = null, int length = 0, ulong value = 0)
        {
            _gate.EnterReadLock();
            try
            {
                ObjectDisposedException.ThrowIf(!_active, this);
                return Invoke(operation, id, buffer, length, value);
            }
            finally { _gate.ExitReadLock(); }
        }

        private long Invoke(uint operation, ulong id, byte* buffer, int length, ulong value)
        {
            var invoke = (delegate* unmanaged[Cdecl]<nint, uint, ulong, byte*, uint, ulong, long>)callback;
            var result = invoke(context, operation, id, buffer, checked((uint)length), value);
            return result switch
            {
                -1 => throw new ObjectDisposedException("native file read"),
                -2 => throw new ArgumentException("invalid native file-read argument or range"),
                -3 => throw new InvalidOperationException("native file-read capacity is exhausted"),
                -4 => throw new InvalidOperationException("file read is still in progress"),
                _ => result,
            };
        }

        internal void Release(ulong id)
        {
            _gate.EnterReadLock();
            try { if (_active) Invoke(6, id, null, 0, 0); }
            finally { _gate.ExitReadLock(); }
        }

        internal void Close()
        {
            _gate.EnterWriteLock();
            try { _active = false; }
            finally { _gate.ExitWriteLock(); }
        }
    }
}

internal sealed unsafe class AsyncReadRequest(AsyncReadBridge.Session session, long identity) : IDisposable
{
    private long _identity = identity;
    internal long Identity => Volatile.Read(ref _identity);
    internal bool IsValid => Identity != 0 && session.Active;
    internal long Call(uint operation) => session.Call(operation, checked((ulong)Identity));

    internal byte[] Copy(uint operation)
    {
        var length = checked((int)Call(operation));
        var data = new byte[length];
        fixed (byte* buffer = data)
        {
            var written = session.Call(operation, checked((ulong)Identity), buffer, length);
            if (written != length) throw new InvalidOperationException("native file-read result changed during copy");
        }
        return data;
    }

    public void Dispose()
    {
        var id = Interlocked.Exchange(ref _identity, 0);
        if (id != 0) session.Release((ulong)id);
    }
}
