# Native world hierarchy and lifetime

`World` owns object identity, typed components, hierarchy, script reference fields and lifetime state. It has no dependency on serialization or a managed host. Detached snapshots permit callers to iterate while mutating the live world.

Local transforms contain translation, a unit quaternion and scale. World matrices multiply the complete ancestor matrices. This preserves affine composition under rotated nonuniform scale. `ParentSpace::KeepLocal` changes only the parent relation. `KeepWorld` computes a local matrix through the new parent's inverse and accepts it only when a TRS reconstruction matches. Singular parents and local shear return distinct typed errors before mutation. Zero local scale remains valid, including representable keep-world changes. Quaternion world orientation resolves unobservable rotation axes when converting a collapsed transform; the resulting TRS must still reconstruct the requested matrix.

`instantiate` preflights capacity, allocates the entire subtree and its components, then remaps typed fields and establishes parents. Object and component remaps are separate. References outside the cloned subtree retain their identity within the same world. Field writes reject foreign-world object and component references recursively before replacing any fields. Components already queued for removal are excluded. No clone lifecycle readiness is published before reference binding finishes.

Instantiation, reparenting and activation changes return world-local mutation IDs. Lifecycle work lists eligible Awake messages in hierarchy preorder, followed by eligible enable/disable transitions. `lifecycle_work` borrows a pending batch; `take_lifecycle_work` transfers it to the caller and releases its native storage, including empty batches. Consumed IDs remain invalid and are never reused. The caller must consume every returned batch, even when discarding its work. Phase retirement does not discard outstanding batches. Inactive objects wait for activation. Disabled scripts on active objects can awaken without becoming enabled. Native readiness describes eligibility and ordered work, not evidence that managed code has executed. Managed callback invocation, nested dispatch, callback outcomes and Start scheduling belong to the later scheduler integration.

`destroy(object, DestroyWhen::EndOfFrame)` marks the subtree and its components pending, matching Unity's `Destroy` timing. Their scripts stop receiving ordinary callbacks at once, but the handles stay readable, writable and visible in snapshots and searches until the frame's cleanup runs OnDisable and OnDestroy and retires the storage. After that every handle is stale. `DestroyWhen::AfterScaled(delay)` schedules the same end-of-frame retirement once scaled time passes the delay; repeated requests keep the earliest deadline. `tests/pending_destroy.rs` pins this behavior. Native disable/destroy cleanup data is queued once per eligible script. `destroy_component` applies the same policy to a removable component. A required Transform cannot be removed.

`take_cleanup` transfers cleanup work to the caller. `finish_phase` retires queued storage. Script cleanup records retain their identities independently of retired storage. The existing `destroy_object` and `remove_component` convenience operations retire only their requested subtree or component; unrelated deferred entries remain occupied until the phase owner calls `finish_phase`. Exact queued object and component identities coalesce duplicate destruction requests until retirement, then become stale. The C ABI's object destruction uses deferred invalidation, and a successful world step completes storage retirement.

`track_changes` turns on an opt-in change journal; it is off by default and costs one flag test per mutation while off. `drain_changes` returns the coalesced `WorldChanges` since the previous drain, in object handle order, and starts a new interval. It reports created objects, destroyed objects (when storage retires, which matches when a handle becomes stale), local transform changes, parent changes, name, tag, layer and active changes, and component additions, removals and changes by `ComponentKind`. An object created and destroyed within one interval reports nothing, repeated changes report once, and a destroyed object reports only its destruction. Changes made before `track_changes` are never reported.

`apply_edits` and `step_with_edits` validate a batch, apply it in place and keep an undo log of the components it touched and a copy of the physics state. A failure restores component contents, arena slot identities, physics state and the journal, so no whole-world clone is needed.

## Short-clip audio

The caller-owned `audio::AudioMixer` holds typed clip and voice identities,
commands, gains, mute and natural-ending notifications. It has no device,
filesystem or clock dependency. The retained host supplies its device or session
clock; this service does not create a second world.

`load_wav` accepts at most 8 MiB of little-endian RIFF/WAVE bytes. Supported
encodings are unsigned PCM8, signed PCM16/24 and IEEE float32, mono or stereo,
at integer rates from 1 through 384000 Hz. RIFF size, chunk bounds and padding,
a unique format and data chunk, format extension length, block alignment, byte
rate and complete nonempty frames are validated before decoding. Unknown chunks
are skipped. Extensible WAV, compression and big-endian RIFX return typed errors.
Nonfinite float samples fail; finite floats outside [-1, 1] are clamped. Mono is
duplicated to stereo. Clip storage is bounded to 64 clips and 64 MiB decoded;
clips remain resident until the mixer is dropped. Host file readers should use
`save::ByteFile` with `MAX_WAV_BYTES`, rather than an unbounded read.

`render(&mut [[f32; 2]])` performs linear sample-rate conversion with an integer
rational phase accumulator. It interpolates adjacent source frames and holds the
last source sample until the clip duration expires. This is a short-sound
approximation without an anti-alias filter; downsampling can alias frequencies
above the destination Nyquist frequency. The known-tone regression pins exact
upsampled values. Clip duration is rounded up to a whole output frame.

Per-voice and master gains are finite in 0..=16. Voices sum in creation order;
master gain follows the sum, then output hard-clips to [-1, 1]. Mute outputs zero
while advancing voices. There are 32 shared voice/completion slots. Undrained
endings still occupy slots, and excess admission returns `VoiceLimit`. Rendering
uses caller storage and preallocated voice/completion queues. `take_finished`
releases a slot. `stop` removes an active voice without producing an ending.
Handles from another mixer cannot control this one. Output rate can only change
when all voice/completion slots are empty.
