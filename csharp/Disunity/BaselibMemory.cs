using System.Runtime.InteropServices;

namespace Unity.Baselib.LowLevel;

/// <summary>
/// Baselib's page memory, which Unity Transport uses for its packet buffers. Pages come from
/// the process allocator aligned to the requested multiple of the page size and zeroed. Page
/// protection is not applied: every state is accepted and pages stay readable and writable,
/// which is the engine's choice.
/// </summary>
public static unsafe partial class Binding
{
    public static void Baselib_Memory_GetPageSizeInfo(Baselib_Memory_PageSizeInfo* outPagesSizeInfo)
    {
        var size = (ulong)Environment.SystemPageSize;
        *outPagesSizeInfo = new Baselib_Memory_PageSizeInfo { defaultPageSize = size, pageSizes0 = size, pageSizesLen = 1 };
    }

    public static Baselib_Memory_PageAllocation Baselib_Memory_AllocatePages(ulong pageSize, ulong pageCount,
        ulong alignmentInMultipleOfPageSize, Baselib_Memory_PageState pageState, Baselib_ErrorState* errorState)
    {
        if (!ValidPageSize(pageSize) || pageCount == 0 || alignmentInMultipleOfPageSize == 0)
        {
            Fail(errorState, pageCount == 0 ? Baselib_ErrorCode.InvalidPageCount : Baselib_ErrorCode.InvalidPageSize);
            return Baselib_Memory_PageAllocation_Invalid;
        }
        var bytes = (nuint)(pageSize * pageCount);
        var pointer = NativeMemory.AlignedAlloc(bytes, (nuint)(pageSize * alignmentInMultipleOfPageSize));
        NativeMemory.Clear(pointer, bytes);
        Succeed(errorState);
        return new Baselib_Memory_PageAllocation { ptr = (IntPtr)pointer, pageSize = pageSize, pageCount = pageCount };
    }

    public static void Baselib_Memory_ReleasePages(Baselib_Memory_PageAllocation pageAllocation, Baselib_ErrorState* errorState)
    {
        if (pageAllocation.ptr != IntPtr.Zero) NativeMemory.AlignedFree((void*)pageAllocation.ptr);
        Succeed(errorState);
    }

    public static void Baselib_Memory_SetPageState(IntPtr addressOfFirstPage, ulong pageSize, ulong pageCount,
        Baselib_Memory_PageState pageState, Baselib_ErrorState* errorState)
    {
        if (!ValidPageSize(pageSize)) Fail(errorState, Baselib_ErrorCode.InvalidPageSize);
        else Succeed(errorState);
    }

    private static bool ValidPageSize(ulong pageSize) => pageSize != 0 && (pageSize & (pageSize - 1)) == 0;

    internal static void Succeed(Baselib_ErrorState* errorState)
    {
        if (errorState != null) *errorState = default;
    }

    internal static void Fail(Baselib_ErrorState* errorState, Baselib_ErrorCode code, ulong nativeErrorCode = 0)
    {
        if (errorState == null) return;
        *errorState = new Baselib_ErrorState
        {
            code = code,
            nativeErrorCode = nativeErrorCode,
            nativeErrorCodeType = nativeErrorCode == 0 ? Baselib_ErrorState_NativeErrorCodeType.None : Baselib_ErrorState_NativeErrorCodeType.PlatformDefined,
        };
    }
}
