# G17 — Jobs, native collections, Burst, ECS

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6.3 and all package-version details are unverified. Code/signatures are authored illustrative subsets from memory, not full declarations or compile-tested examples. See U001–U003 and U052–U054.

**Boundary:** the engine Job System integrates with native scheduling; Collections (`com.unity.collections`), Burst (`com.unity.burst`), Mathematics (`com.unity.mathematics`) and Entities (`com.unity.entities`) have separately versioned surfaces. Some foundational types ship through engine assemblies while advanced containers/extensions live in packages. A namespace alone is not a package/version manifest. Entities is optional; MonoBehaviour/GameObject is not implemented on top of ECS merely by contract.

## G17.T01 — Job interfaces and dependencies

```csharp
// Familiar job interface shapes.
public interface IJob { void Execute(); }
public interface IJobParallelFor { void Execute(int index); }
// Selected JobHandle operations, simplified.
void Complete();
bool IsCompleted { get; }
static JobHandle CombineDependencies(JobHandle a, JobHandle b);
```

Job extension methods provide `Schedule(...)`/`Run(...)`; a common IJob schedule takes a dependency JobHandle, and IJobParallelFor scheduling takes array length, inner-loop batch size and dependency. Exact extension types, defaults, newer `IJobFor` and batch/deferred variants are U052 rather than exhaustively listed.

Scheduling copies job struct data into scheduled work in the usual model. Primitive field writes inside a job do not update the original job struct on the scheduling thread; native container views can refer to shared storage. A dependency ensures preceding work finishes before dependent work executes. It does not by itself transfer arbitrary ownership, lock unrelated data, or make hidden managed references safe.

`IsCompleted` polling is not a replacement for `Complete()` when ownership/safety bookkeeping must be finalized before reading output. `Complete` can execute/wait for work and is a synchronization boundary. `ScheduleBatchedJobs()` is a remembered static JobHandle facility to flush scheduling batches; performance details should not become semantic requirements.

`UnityEngine.Jobs.IJobParallelForTransform` and `TransformAccessArray` provide controlled access to transforms. The Execute shape takes an index and TransformAccess. They do not permit unrestricted GameObject/Component operations from worker threads. Allocation/disposal and ancestor/descendant write rules need verification for the chosen version.

## G17.T02 — Native containers and allocation

Important types include `NativeArray<T>`, `NativeSlice<T>`, `NativeList<T>`, `NativeQueue<T>`, native hash/parallel hash maps and sets, native streams/references, plus unsafe variants. Exact container naming (for example NativeHashMap versus NativeParallelHashMap availability) is package-sensitive. The `T` constraints must match the selected version; “any managed class” is not valid.

```csharp
// Selected NativeArray usage shapes, not full declarations.
var values = new Unity.Collections.NativeArray<float>(
    16, Unity.Collections.Allocator.Persistent);
int count = values.Length;
values[0] = 3f;
values.Dispose();
```

NativeArray is a value-type owner/view of unmanaged storage; copying it aliases the allocation rather than deep-copying elements. Disposing one owner invalidates aliases. NativeSlice is a view with stride/offset and normally does not own an independent allocation. A container being a struct does not make its contents value-copied.

`Allocator.Temp`, `TempJob`, `Persistent`, `None` and invalid-related values are familiar legacy allocator concepts. Temp is short-lived/thread-context-sensitive: do not pass main-thread Temp allocations into scheduled jobs. Use an allocator/lifetime appropriate to the scheduled consumers, commonly TempJob or Persistent; Temp allocation inside a job's own thread scope is a different case. TempJob has a small bounded frame lifetime traditionally described as four frames, and Persistent requires explicit disposal. AllocatorManager/custom allocator handles add package-version-specific capabilities. Exact enforcement and restrictions are U053. Do not rely on safety-check warnings as the only memory-management policy.

`NativeArrayOptions.ClearMemory` and `UninitializedMemory` choose initialization. Reading uninitialized elements before writes is invalid application behavior even if a particular test observes zeros. Many containers expose `IsCreated`; this is not a universal proof that every copied alias remains valid after another alias disposes it.

## G17.T03 — Parallel safety and data ownership

Attributes such as `[ReadOnly]`, `[WriteOnly]`, `[DeallocateOnJobCompletion]`, and low-level safety restrictions communicate scheduler/safety expectations. Some live in `Unity.Collections.LowLevel.Unsafe`. `[NativeDisableParallelForRestriction]` or disabling container safety is not a correctness proof; the author must ensure disjoint access and proper lifetime.

Parallel writers (`AsParallelWriter` families) offer specific concurrent operations with capacity/order constraints. They do not convert every operation on a container into a general concurrent collection. Parallel iteration order is not stable unless explicitly guaranteed. Floating-point reduction order can change results.

Many containers support `Dispose(JobHandle inputDeps)` returning a dependency for deferred cleanup. Disposal must depend on every user. Completing one unrelated handle does not make the memory safe to free. A copy of a JobHandle represents the same dependency relation, not a new job instance. Main-thread access before dependent writes complete should be prevented or diagnosed, not silently raced.

A compatibility scheduler can execute serially while preserving public dependency/results semantics for a declared subset, but that does not establish performance, race diagnostics, scheduling fairness, safety-handle behavior or Burst equivalence. Native buffer layout, alignment and pointer APIs are additional contracts.

## G17.T04 — Burst compilation boundary

`Unity.Burst.BurstCompileAttribute` marks supported jobs/methods for Burst compilation. Options include FloatMode and FloatPrecision-related settings in remembered versions. `BurstDiscard` excludes a method call from Burst-compiled execution under specific rules. Exact options/limitations are U053.

Burst is a compiler/toolchain, not another thread scheduler or an automatic accelerator for all C# code. Supported code largely requires unmanaged data and a restricted managed feature set. Arbitrary engine object operations, allocations, reflection and managed polymorphism are not universally legal. Editor fallback behavior can hide a Burst-incompatible path; test both the intended compiled path and fallback where relevant.

`Unity.Mathematics` provides SIMD-oriented value types and `math` functions; its quaternion/float types do not inherit UnityEngine math types. `Unity.Mathematics.Random` is an explicit-state generator unlike UnityEngine.Random's global state. Exact random sequences and conversions are separate contracts. Fast floating-point modes can change precision, reassociation, NaN and determinism expectations.

## G17.T05 — Entities worlds, components, queries

Modern Entities (broadly 1.x-style recollection, not a selected release) uses `World`, `EntityManager`, `Entity`, archetypes/chunks and queries. `Entity` is an index/version identity interpreted by its owning world, not a UnityEngine.Object or a globally unique handle. In recalled behavior, the Entity value does not itself encode its World: an equal index/version can exist in another world, so EntityManager.Exists is not proof of world provenance. Retain the owning world/manager alongside cross-boundary entity references. Within a world, a stale version is rejected after index reuse. `Entity.Null` and EntityManager.Exists are remembered identity checks; version-wrap and exact release details remain U054.

Components include unmanaged `IComponentData`, buffer elements (`IBufferElementData`, `DynamicBuffer<T>`), shared components (`ISharedComponentData`), enableable components (`IEnableableComponent`), and specialized managed components. Structural changes alter entity archetype/storage membership; changing a component's field ordinarily does not. Shared values can partition chunks and have different cost/semantics from ordinary component data.

```csharp
// Selected EntityManager shapes; generic constraints deliberately omitted.
EntityArchetype CreateArchetype(params ComponentType[] types);
Entity CreateEntity(EntityArchetype archetype);
void DestroyEntity(Entity entity);
void AddComponentData<T>(Entity entity, T componentData);
T GetComponentData<T>(Entity entity);
void SetComponentData<T>(Entity entity, T componentData);
```

Other creation overloads accept component type information; their complete forms are not asserted. Managed and unmanaged component access use different APIs in modern versions. Verify the exact constraints, accepted component categories and structural-change behavior before implementing (U054).

`EntityQuery`, `ComponentType.ReadOnly<T>()`/`ReadWrite<T>()`, query builders and generated `SystemAPI.Query<...>()` provide selection/access. Required/excluded/optional/enable-state/shared-filter semantics matter. `RefRO<T>`, `RefRW<T>`, `EnabledRefRW<T>`, `DynamicBuffer<T>`, aspects and lookup types are remembered modern patterns, not a stable universal declaration set. Queries are not equivalent to scanning all GameObjects with GetComponent.

## G17.T06 — Systems, command buffers, baking boundaries

`SystemBase` is a managed system base with `OnUpdate`; `ISystem` is an unmanaged system interface with `OnCreate(ref SystemState)`, `OnUpdate(ref SystemState)`, `OnDestroy(ref SystemState)` shapes. Exact required methods/default behavior are version-sensitive. System groups and `[UpdateInGroup]`, `[UpdateBefore]`, `[UpdateAfter]` order systems within valid scopes. Dependencies still control jobs; ordering attributes alone do not eliminate every data hazard.

`IJobEntity` and `IJobChunk` are package job patterns, often involving source generation and query/chunk access. Do not attempt to reproduce their source-compatible syntax solely through runtime reflection stubs. Source generators, codegen, safety, Burst and assembly processing are separate build-time pieces.

`EntityCommandBuffer` records structural changes for later playback; its parallel writer records using sort keys in familiar APIs. A recorded creation returns a temporary/deferred entity reference meaningful within the buffer's remapping rules, not immediately queryable live storage. Playback, disposal, producer dependencies and failure atomicity require an explicit contract. A buffer is not automatically a transaction with rollback.

Baking converts authoring GameObjects/assets into entity data through `Baker<T>`/baking systems and SubScene workflows. It is an editor/build pipeline, not the old conversion API under a new namespace. `Unity.Transforms.LocalTransform`, `LocalToWorld`, and related components differ from older Translation/Rotation/Scale component patterns; `Unity.Rendering`/Entities Graphics is another package boundary. Exact baking/blob/scene serialization is U054.

Proposed tests (not run): chained jobs produce correct buffers; completion precedes main-thread reads; aliases observe writes but not independent ownership; disjoint parallel writes; deferred disposal follows consumers; stale entity identity is rejected; enabling a component differs from adding/removing it; ECB playback remaps deferred entities. Binary layouts, complete safety diagnostics, generated-query syntax, performance and exact Entities-version compatibility remain unknown.
