using System.Reflection;
using System.Runtime.CompilerServices;
using MemUnsafe = System.Runtime.CompilerServices.Unsafe;
using System.Runtime.InteropServices;

namespace Unity.Collections.LowLevel.Unsafe
{
    /// <summary>
    /// Unity's unmanaged memory and type utilities. Allocations come from the process's aligned
    /// allocator for every allocator label; <c>Temp</c> memory is not reclaimed at the end of
    /// a frame, so code that relies on that leaks until it frees the memory itself.
    /// </summary>
    public static unsafe class UnsafeUtility
    {
        public static void* Malloc(long size, int alignment, Allocator allocator)
        {
            if (allocator <= Allocator.None) throw new ArgumentException($"cannot allocate with Allocator.{allocator}", nameof(allocator));
            ArgumentOutOfRangeException.ThrowIfNegative(size);
            var align = (nuint)Math.Max(alignment, 1);
            if ((align & (align - 1)) != 0) throw new ArgumentException($"alignment {alignment} is not a power of two", nameof(alignment));
            return NativeMemory.AlignedAlloc((nuint)Math.Max(size, 1), align);
        }

        public static void* Malloc(long size, int alignment, MemoryLabel label) => Malloc(size, alignment, label.Allocator);

        public static void Free(void* memory, Allocator allocator)
        {
            if (allocator <= Allocator.None) return;
            NativeMemory.AlignedFree(memory);
        }

        public static void Free(void* memory, MemoryLabel label) => Free(memory, label.Allocator);
        public static void* MallocTracked(long size, int alignment, Allocator allocator, int callstacksToSkip) => Malloc(size, alignment, allocator);
        public static void* MallocTracked(long size, int alignment, MemoryLabel label, int callstacksToSkip) => Malloc(size, alignment, label);
        public static void FreeTracked(void* memory, Allocator allocator) => Free(memory, allocator);
        public static void FreeTracked(void* memory, MemoryLabel label) => Free(memory, label);
        public static bool IsValidAllocator(Allocator allocator) => allocator > Allocator.None;
        public static int CheckForLeaks() => 0;
        public static int ForgiveLeaks() => 0;
        public static NativeLeakDetectionMode GetLeakDetectionMode() => NativeLeakDetection.Mode;
        public static void SetLeakDetectionMode(NativeLeakDetectionMode value) => NativeLeakDetection.Mode = value;

        public static void MemCpy(void* destination, void* source, long size) => Buffer.MemoryCopy(source, destination, size, size);
        public static void MemMove(void* destination, void* source, long size) => Buffer.MemoryCopy(source, destination, size, size);
        public static void MemSet(void* destination, byte value, long size) => NativeMemory.Fill(destination, (nuint)size, value);
        public static void MemClear(void* destination, long size) => NativeMemory.Clear(destination, (nuint)size);
        public static int MemCmp(void* ptr1, void* ptr2, long size) =>
            new ReadOnlySpan<byte>(ptr1, checked((int)size)).SequenceCompareTo(new ReadOnlySpan<byte>(ptr2, checked((int)size)));

        public static void MemSwap(void* ptr1, void* ptr2, long size)
        {
            var a = new Span<byte>(ptr1, checked((int)size));
            var b = new Span<byte>(ptr2, checked((int)size));
            for (var i = 0; i < a.Length; i++) (a[i], b[i]) = (b[i], a[i]);
        }

        public static void MemCpyReplicate(void* destination, void* source, int size, int count)
        {
            for (var i = 0; i < count; i++) MemCpy((byte*)destination + (long)i * size, source, size);
        }

        public static void MemCpyStride(void* destination, int destinationStride, void* source, int sourceStride, int elementSize, int count)
        {
            for (var i = 0; i < count; i++)
                MemCpy((byte*)destination + (long)i * destinationStride, (byte*)source + (long)i * sourceStride, elementSize);
        }

        public static int SizeOf<T>() where T : struct => MemUnsafe.SizeOf<T>();
        public static int SizeOf(Type type) => (int)typeof(UnsafeUtility).GetMethod(nameof(SizeOfAny), BindingFlags.NonPublic | BindingFlags.Static)!
            .MakeGenericMethod(type).Invoke(null, null)!;
        private static int SizeOfAny<T>() => MemUnsafe.SizeOf<T>();

        /// <summary>The alignment Unity reports: the size of the type's widest field, up to 16.</summary>
        public static int AlignOf<T>() where T : struct
        {
            var size = MemUnsafe.SizeOf<AlignOfHelper<T>>() - MemUnsafe.SizeOf<T>();
            return Math.Clamp(size, 1, 16);
        }

        // Only its layout is used: the padding before Data is the alignment of T.
        private struct AlignOfHelper<T> where T : struct
        {
            public byte Dummy = 0;
            public T Data = default;
            public AlignOfHelper() { }
        }

        public static bool IsUnmanaged<T>() => !RuntimeHelpers.IsReferenceOrContainsReferences<T>();
        public static bool IsUnmanaged(Type type) => (bool)typeof(UnsafeUtility).GetMethod(nameof(IsUnmanaged), 1, Type.EmptyTypes)!
            .MakeGenericMethod(type).Invoke(null, null)!;
        public static bool IsBlittable<T>() where T : struct => IsUnmanaged<T>();
        public static bool IsBlittable(Type type) => type.IsValueType && IsUnmanaged(type);
        public static bool IsValidNativeContainerElementType<T>() => IsUnmanaged<T>();
        public static bool IsValidNativeContainerElementType(Type type) => IsUnmanaged(type);
        public static bool IsNativeContainerType<T>() => typeof(T).IsDefined(typeof(NativeContainerAttribute), false);

        public static void CopyPtrToStructure<T>(void* ptr, out T output) where T : struct => output = MemUnsafe.Read<T>(ptr);
        public static void CopyStructureToPtr<T>(ref T input, void* ptr) where T : struct => MemUnsafe.Write(ptr, input);
        public static T ReadArrayElement<T>(void* source, int index) => MemUnsafe.Read<T>((byte*)source + (long)index * MemUnsafe.SizeOf<T>());
        public static T ReadArrayElementWithStride<T>(void* source, int index, int stride) => MemUnsafe.Read<T>((byte*)source + (long)index * stride);
        public static void WriteArrayElement<T>(void* destination, int index, T value) =>
            MemUnsafe.Write((byte*)destination + (long)index * MemUnsafe.SizeOf<T>(), value);
        public static void WriteArrayElementWithStride<T>(void* destination, int index, int stride, T value) =>
            MemUnsafe.Write((byte*)destination + (long)index * stride, value);
        public static void* AddressOf<T>(ref T output) where T : struct => MemUnsafe.AsPointer(ref output);
        public static ref U As<T, U>(ref T from) => ref MemUnsafe.As<T, U>(ref from);
        public static ref T AsRef<T>(void* ptr) where T : struct => ref MemUnsafe.AsRef<T>(ptr);
        public static ref T ArrayElementAsRef<T>(void* ptr, int index) where T : struct =>
            ref MemUnsafe.AsRef<T>((byte*)ptr + (long)index * MemUnsafe.SizeOf<T>());
        public static int EnumToInt<T>(T enumValue) where T : struct, IConvertible => Convert.ToInt32(enumValue, System.Globalization.CultureInfo.InvariantCulture);
        public static bool EnumEquals<T>(T lhs, T rhs) where T : struct, IConvertible => EqualityComparer<T>.Default.Equals(lhs, rhs);
        public static int GetFieldOffset(FieldInfo field) => (int)Marshal.OffsetOf(field.DeclaringType!, field.Name);

        public static void* PinGCObjectAndGetAddress(object target, out ulong gcHandle)
        {
            var handle = GCHandle.Alloc(target, GCHandleType.Pinned);
            gcHandle = (ulong)(long)GCHandle.ToIntPtr(handle);
            return (void*)handle.AddrOfPinnedObject();
        }

        public static void* PinGCArrayAndGetDataAddress(Array target, out ulong gcHandle) => PinGCObjectAndGetAddress(target, out gcHandle);
        public static void ReleaseGCObject(ulong gcHandle) => GCHandle.FromIntPtr((IntPtr)(long)gcHandle).Free();
    }

    public static unsafe class NativeArrayUnsafeUtility
    {
        /// <summary>An array over existing memory. With <c>Allocator.None</c> disposing it frees nothing.</summary>
        public static NativeArray<T> ConvertExistingDataToNativeArray<T>(void* dataPointer, int length, Allocator allocator) where T : struct =>
            new() { m_Buffer = dataPointer, m_Length = length, m_AllocatorLabel = allocator };

        /// <summary>An array over a span's memory, which must stay where it is while the array is used.</summary>
        public static NativeArray<T> ConvertExistingDataToNativeArray<T>(Span<T> data, Allocator allocator) where T : struct =>
            ConvertExistingDataToNativeArray<T>(MemUnsafe.AsPointer(ref MemoryMarshal.GetReference(data)), data.Length, allocator);

        public static void* GetUnsafePtr<T>(this NativeArray<T> nativeArray) where T : struct => nativeArray.m_Buffer;
        public static void* GetUnsafeReadOnlyPtr<T>(this NativeArray<T> nativeArray) where T : struct => nativeArray.m_Buffer;
        public static void* GetUnsafeReadOnlyPtr<T>(this NativeArray<T>.ReadOnly nativeArray) where T : struct => nativeArray.m_Buffer;
        public static void* GetUnsafeBufferPointerWithoutChecks<T>(NativeArray<T> nativeArray) where T : struct => nativeArray.m_Buffer;
    }

    public static unsafe class NativeSliceUnsafeUtility
    {
        public static NativeSlice<T> ConvertExistingDataToNativeSlice<T>(void* dataPointer, int stride, int length) where T : struct =>
            new() { m_Buffer = (byte*)dataPointer, m_Stride = stride, m_Length = length };

        public static void* GetUnsafePtr<T>(this NativeSlice<T> nativeSlice) where T : struct => nativeSlice.m_Buffer;
        public static void* GetUnsafeReadOnlyPtr<T>(this NativeSlice<T> nativeSlice) where T : struct => nativeSlice.m_Buffer;
    }

    // Markers the job system reads in the editor's safety checks; a player only needs them to exist.
    [AttributeUsage(AttributeTargets.Struct)] public sealed class NativeContainerAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Struct)] public sealed class NativeContainerIsReadOnlyAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Struct)] public sealed class NativeContainerIsAtomicWriteOnlyAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Struct)] public sealed class NativeContainerSupportsDeallocateOnJobCompletionAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Struct)] public sealed class NativeContainerSupportsDeferredConvertListToArray : Attribute;
    [AttributeUsage(AttributeTargets.Struct)] public sealed class NativeContainerSupportsMinMaxWriteRestrictionAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class NativeDisableUnsafePtrRestrictionAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class NativeDisableContainerSafetyRestrictionAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class NativeSetThreadIndexAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class NativeSetClassTypeToNullOnScheduleAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Method | AttributeTargets.Property)] public sealed class WriteAccessRequiredAttribute : Attribute;
}

namespace Unity.Collections
{
    [AttributeUsage(AttributeTargets.Field | AttributeTargets.Parameter)] public sealed class ReadOnlyAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class WriteOnlyAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class DeallocateOnJobCompletionAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class NativeDisableParallelForRestrictionAttribute : Attribute;
    [AttributeUsage(AttributeTargets.Field)] public sealed class NativeMatchesParallelForLengthAttribute : Attribute;

    [AttributeUsage(AttributeTargets.Field)]
    public sealed class NativeFixedLengthAttribute(int fixedLength) : Attribute
    {
        public int FixedLength = fixedLength;
    }
}
