using System.Collections;
using System.Runtime.CompilerServices;
using Unity.Collections.LowLevel.Unsafe;
using Unity.Jobs;

namespace Unity.Collections
{
    public enum Allocator
    {
        Invalid = 0,
        None = 1,
        Temp = 2,
        TempJob = 3,
        Persistent = 4,
        AudioKernel = 5,
        Domain = 6,
        FirstUserIndex = 64,
    }

    public enum NativeArrayOptions
    {
        UninitializedMemory = 0,
        ClearMemory = 1,
    }

    public enum NativeLeakDetectionMode
    {
        Disabled = 1,
        Enabled = 2,
        EnabledWithStackTrace = 3,
    }

    public static class NativeLeakDetection
    {
        public static NativeLeakDetectionMode Mode { get; set; } = NativeLeakDetectionMode.Disabled;
    }

    /// <summary>A named memory area for profiling. The engine records only its allocator.</summary>
    public readonly struct MemoryLabel
    {
        internal readonly Allocator Allocator;

        public MemoryLabel(string areaName, string objectName, Allocator allocator) => Allocator = allocator;

        public bool IsCreated => Allocator != Allocator.Invalid;
        public static bool SupportsAllocator(Allocator allocator) => allocator is Allocator.Persistent or Allocator.Domain;
    }

    /// <summary>
    /// An array in native memory. The fields are Unity's, in Unity's order, so code that reads
    /// a <c>NativeArray</c> through pointers sees the same layout. There are no safety checks,
    /// as in a release player.
    /// </summary>
    public unsafe struct NativeArray<T> : IDisposable, IEnumerable<T>, IEquatable<NativeArray<T>> where T : struct
    {
        internal void* m_Buffer;
        internal int m_Length;
        internal Allocator m_AllocatorLabel;

        public NativeArray(int length, Allocator allocator, NativeArrayOptions options = NativeArrayOptions.ClearMemory)
        {
            ArgumentOutOfRangeException.ThrowIfNegative(length);
            var size = (long)UnsafeUtility.SizeOf<T>() * length;
            m_Buffer = UnsafeUtility.Malloc(size, UnsafeUtility.AlignOf<T>(), allocator);
            m_Length = length;
            m_AllocatorLabel = allocator;
            if (options == NativeArrayOptions.ClearMemory) UnsafeUtility.MemClear(m_Buffer, size);
        }

        public NativeArray(T[] array, Allocator allocator) :
            this((array ?? throw new ArgumentNullException(nameof(array))).Length, allocator, NativeArrayOptions.UninitializedMemory) =>
            CopyFrom(array);

        public NativeArray(NativeArray<T> array, Allocator allocator) : this(array.Length, allocator, NativeArrayOptions.UninitializedMemory) =>
            CopyFrom(array);

        public NativeArray(ReadOnly array, Allocator allocator) : this(array.Length, allocator, NativeArrayOptions.UninitializedMemory) =>
            Copy(array, 0, this, 0, array.Length);

        public readonly bool IsCreated => m_Buffer != null;
        public readonly int Length => m_Length;

        public T this[int index]
        {
            readonly get => UnsafeUtility.ReadArrayElement<T>(m_Buffer, Check(index));
            set => UnsafeUtility.WriteArrayElement(m_Buffer, Check(index), value);
        }

        private readonly int Check(int index) =>
            (uint)index < (uint)m_Length ? index : throw new IndexOutOfRangeException($"index {index} is out of range of '{m_Length}' Length");

        /// <summary>
        /// Frees the memory. As in a Unity player, which has no safety handle to recognise a second
        /// call, the array forgets its allocator, so disposing it again, a default array or an
        /// array over memory it does not own (<c>Allocator.None</c>) throws.
        /// </summary>
        public void Dispose()
        {
            if (!UnsafeUtility.IsValidAllocator(m_AllocatorLabel))
                throw new InvalidOperationException("The NativeArray can not be Disposed because it was not allocated with a valid allocator.");
            UnsafeUtility.Free(m_Buffer, m_AllocatorLabel);
            m_Buffer = null;
            m_Length = 0;
            m_AllocatorLabel = Allocator.Invalid;
        }

        /// <summary>Jobs finish when they are scheduled, so the array is freed at once.</summary>
        public JobHandle Dispose(JobHandle inputDeps)
        {
            Dispose();
            return inputDeps;
        }

        public void CopyFrom(T[] array) => Copy(array, this);
        public void CopyFrom(NativeArray<T> array) => Copy(array, this);
        public void CopyFrom(ReadOnly array) => Copy(array, this);
        public readonly void CopyTo(T[] array) => Copy(this, array);
        public readonly void CopyTo(NativeArray<T> array) => Copy(this, array);

        public readonly T[] ToArray()
        {
            var array = new T[Length];
            Copy(this, 0, array, 0, Length);
            return array;
        }

        public readonly Span<T> AsSpan() => new(m_Buffer, m_Length);
        public readonly ReadOnlySpan<T> AsReadOnlySpan() => new(m_Buffer, m_Length);
        public static implicit operator Span<T>(in NativeArray<T> source) => source.AsSpan();
        public static implicit operator ReadOnlySpan<T>(in NativeArray<T> source) => source.AsReadOnlySpan();
        public readonly ReadOnly AsReadOnly() => new(m_Buffer, m_Length);

        public readonly NativeArray<T> GetSubArray(int start, int length)
        {
            if (start < 0 || length < 0 || start > m_Length - length)
                throw new ArgumentOutOfRangeException(nameof(length), $"sub-array [{start}, {start + length}) is outside the array of {m_Length}");
            return NativeArrayUnsafeUtility.ConvertExistingDataToNativeArray<T>(
                (byte*)m_Buffer + (long)UnsafeUtility.SizeOf<T>() * start, length, Allocator.None);
        }

        public readonly NativeArray<U> Reinterpret<U>() where U : struct => Reinterpret<U>(UnsafeUtility.SizeOf<T>());

        /// <summary>The same memory as elements of <typeparamref name="U"/>. <paramref name="expectedTypeSize"/>
        /// is the caller's belief about the size of <typeparamref name="T"/>, which must be right.</summary>
        public readonly NativeArray<U> Reinterpret<U>(int expectedTypeSize) where U : struct
        {
            var size = UnsafeUtility.SizeOf<T>();
            if (size != expectedTypeSize)
                throw new InvalidOperationException($"Type {typeof(T)} was expected to be {expectedTypeSize} but is {size} bytes");
            return NativeArrayUnsafeUtility.ConvertExistingDataToNativeArray<U>(m_Buffer, ReinterpretedLength<U>(m_Length), Allocator.None);
        }

        private static int ReinterpretedLength<U>(int length) where U : struct
        {
            var bytes = (long)UnsafeUtility.SizeOf<T>() * length;
            var size = UnsafeUtility.SizeOf<U>();
            if (bytes % size != 0)
                throw new InvalidOperationException(
                    $"Types {typeof(T)} (array length {length}) and {typeof(U)} cannot be aliased due to size constraints. The size of the types and lengths involved must line up.");
            return (int)(bytes / size);
        }

        private static void RequireSameLength(int source, int destination)
        {
            if (source != destination) throw new ArgumentException("source and destination length must be the same");
        }

        public readonly U ReinterpretLoad<U>(int sourceIndex) where U : struct =>
            UnsafeUtility.ReadArrayElement<U>((byte*)m_Buffer + (long)UnsafeUtility.SizeOf<T>() * sourceIndex, 0);

        public void ReinterpretStore<U>(int destIndex, U data) where U : struct =>
            UnsafeUtility.WriteArrayElement((byte*)m_Buffer + (long)UnsafeUtility.SizeOf<T>() * destIndex, 0, data);

        public static void Copy(NativeArray<T> src, NativeArray<T> dst)
        {
            RequireSameLength(src.Length, dst.Length);
            Copy(src, 0, dst, 0, src.Length);
        }

        public static void Copy(ReadOnly src, NativeArray<T> dst)
        {
            RequireSameLength(src.Length, dst.Length);
            Copy(src, 0, dst, 0, src.Length);
        }

        public static void Copy(T[] src, NativeArray<T> dst)
        {
            RequireSameLength(src.Length, dst.Length);
            Copy(src, 0, dst, 0, src.Length);
        }

        public static void Copy(NativeArray<T> src, T[] dst)
        {
            RequireSameLength(src.Length, dst.Length);
            Copy(src, 0, dst, 0, src.Length);
        }

        public static void Copy(NativeArray<T> src, NativeArray<T> dst, int length) => Copy(src, 0, dst, 0, length);
        public static void Copy(T[] src, NativeArray<T> dst, int length) => Copy(src, 0, dst, 0, length);
        public static void Copy(NativeArray<T> src, T[] dst, int length) => Copy(src, 0, dst, 0, length);

        public static void Copy(NativeArray<T> src, int srcIndex, NativeArray<T> dst, int dstIndex, int length) =>
            src.AsReadOnlySpan().Slice(srcIndex, length).CopyTo(dst.AsSpan().Slice(dstIndex, length));

        public static void Copy(ReadOnly src, int srcIndex, NativeArray<T> dst, int dstIndex, int length) =>
            src.AsReadOnlySpan().Slice(srcIndex, length).CopyTo(dst.AsSpan().Slice(dstIndex, length));

        public static void Copy(T[] src, int srcIndex, NativeArray<T> dst, int dstIndex, int length) =>
            src.AsSpan(srcIndex, length).CopyTo(dst.AsSpan().Slice(dstIndex, length));

        public static void Copy(NativeArray<T> src, int srcIndex, T[] dst, int dstIndex, int length) =>
            src.AsReadOnlySpan().Slice(srcIndex, length).CopyTo(dst.AsSpan(dstIndex, length));

        public readonly Enumerator GetEnumerator() => new(this);
        readonly IEnumerator<T> IEnumerable<T>.GetEnumerator() => GetEnumerator();
        readonly IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public readonly bool Equals(NativeArray<T> other) => m_Buffer == other.m_Buffer && m_Length == other.m_Length;
        public override readonly bool Equals(object? obj) => obj is NativeArray<T> other && Equals(other);
        public override readonly int GetHashCode() => HashCode.Combine((IntPtr)m_Buffer, m_Length);
        public static bool operator ==(NativeArray<T> left, NativeArray<T> right) => left.Equals(right);
        public static bool operator !=(NativeArray<T> left, NativeArray<T> right) => !left.Equals(right);

        public struct Enumerator : IEnumerator<T>
        {
            private readonly NativeArray<T> _array;
            private int _index;

            public Enumerator(ref NativeArray<T> array) : this(array) { }

            internal Enumerator(NativeArray<T> array)
            {
                _array = array;
                _index = -1;
            }

            public readonly T Current => _array[_index];
            readonly object IEnumerator.Current => Current;
            public bool MoveNext() => ++_index < _array.Length;
            public void Reset() => _index = -1;
            public readonly void Dispose() { }
        }

        /// <summary>A read-only view of a native array.</summary>
        public readonly struct ReadOnly : IEnumerable<T>
        {
            internal readonly void* m_Buffer;
            internal readonly int m_Length;

            internal ReadOnly(void* buffer, int length)
            {
                m_Buffer = buffer;
                m_Length = length;
            }

            public bool IsCreated => m_Buffer != null;
            public int Length => m_Length;

            public T this[int index] => (uint)index < (uint)m_Length
                ? UnsafeUtility.ReadArrayElement<T>(m_Buffer, index)
                : throw new IndexOutOfRangeException($"index {index} is out of range of '{m_Length}' Length");

            public ref readonly T UnsafeElementAt(int index) => ref UnsafeUtility.ArrayElementAsRef<T>(m_Buffer, index);
            public ReadOnlySpan<T> AsReadOnlySpan() => new(m_Buffer, m_Length);

            public void CopyTo(T[] array)
            {
                RequireSameLength(m_Length, array.Length);
                AsReadOnlySpan().CopyTo(array);
            }

            public void CopyTo(NativeArray<T> array) => Copy(this, array);
            public T[] ToArray() => AsReadOnlySpan().ToArray();

            public NativeArray<U>.ReadOnly Reinterpret<U>() where U : struct => new(m_Buffer, ReinterpretedLength<U>(m_Length));

            public static implicit operator ReadOnlySpan<T>(in ReadOnly source) => source.AsReadOnlySpan();
            public NativeArray<T>.Enumerator GetEnumerator() =>
                new(NativeArrayUnsafeUtility.ConvertExistingDataToNativeArray<T>(m_Buffer, m_Length, Allocator.None));
            IEnumerator<T> IEnumerable<T>.GetEnumerator() => GetEnumerator();
            IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        }
    }

    /// <summary>A strided view of native memory, such as one field of an array of structs.</summary>
    public unsafe struct NativeSlice<T> : IEnumerable<T>, IEquatable<NativeSlice<T>> where T : struct
    {
        internal byte* m_Buffer;
        internal int m_Stride;
        internal int m_Length;

        public NativeSlice(NativeArray<T> array) : this(array, 0, array.Length) { }
        public NativeSlice(NativeArray<T> array, int start) : this(array, start, array.Length - start) { }

        public NativeSlice(NativeArray<T> array, int start, int length)
        {
            if (start < 0 || length < 0 || start > array.Length - length)
                throw new ArgumentOutOfRangeException(nameof(length), $"slice [{start}, {start + length}) is outside the array of {array.Length}");
            m_Stride = UnsafeUtility.SizeOf<T>();
            m_Buffer = (byte*)array.m_Buffer + (long)m_Stride * start;
            m_Length = length;
        }

        public readonly int Length => m_Length;
        public readonly int Stride => m_Stride;

        public T this[int index]
        {
            readonly get => UnsafeUtility.ReadArrayElementWithStride<T>(m_Buffer, Check(index), m_Stride);
            set => UnsafeUtility.WriteArrayElementWithStride(m_Buffer, Check(index), m_Stride, value);
        }

        private readonly int Check(int index) =>
            (uint)index < (uint)m_Length ? index : throw new IndexOutOfRangeException($"index {index} is out of range of '{m_Length}' Length");

        public readonly void CopyTo(T[] array)
        {
            RequireLength(array.Length);
            for (var i = 0; i < m_Length; i++) array[i] = this[i];
        }

        public void CopyFrom(T[] array)
        {
            RequireLength(array.Length);
            for (var i = 0; i < m_Length; i++) this[i] = array[i];
        }

        private readonly void RequireLength(int length)
        {
            if (length != m_Length) throw new ArgumentException("slice and array length must be the same");
        }

        public readonly T[] ToArray()
        {
            var array = new T[m_Length];
            CopyTo(array);
            return array;
        }

        public readonly IEnumerator<T> GetEnumerator()
        {
            for (var i = 0; i < m_Length; i++) yield return this[i];
        }

        readonly IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
        public readonly bool Equals(NativeSlice<T> other) => m_Buffer == other.m_Buffer && m_Stride == other.m_Stride && m_Length == other.m_Length;
        public override readonly bool Equals(object? obj) => obj is NativeSlice<T> other && Equals(other);
        public override readonly int GetHashCode() => HashCode.Combine((IntPtr)m_Buffer, m_Stride, m_Length);
        public static implicit operator NativeSlice<T>(NativeArray<T> array) => new(array);
    }
}
