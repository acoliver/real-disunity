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

### Pause reasons and voice eligibility

`pause_mask()` reads this mixer's independent `u32` reason set. New mixers start
at zero. `pause(bits, ExistingVoices::Continue)` ORs bits into it and blocks new
pause-eligible playback without changing existing voices. `ExistingVoices::Suspend`
also suspends every existing eligible voice whenever the resulting mask is
nonzero. Repeated reasons have no nesting count. A zero-bit Suspend call sweeps
when another reason is held; it does nothing with an empty mask. All 32 bits are
available to callers; the engine assigns no reason meanings.

`resume(bits)` clears those bits. Only a nonzero-to-zero transition releases
suspension. Clearing absent or zero bits does not resume voices. Suspension
retains each voice's exact rational source cursor and occupied capacity, emits
silence and cannot cause a natural ending. Final clearance continues that same
cursor without restarting. Gain, bus and mute changes never release suspension.
An unsuspended voice still advances under master or bus mute. Stop remains
destructive and emits no ending.

`PlaybackOptions::new(gain)` is unrouted and pause-eligible. Use
`play_with_options(clip, PlaybackOptions { gain, bus, pause_eligible })` to choose
eligibility independently of any named bus. `bus` is `Option<String>` and must
name an existing bus when present. Ordinary `play` and `play_on_bus` use this
same admission path with eligibility true. Admission resolves the bus, validates
gain, owner and capacity, then returns `AudioError::Paused` for an eligible
start under a nonzero mask. Refusal allocates no identity or slot and queues
nothing for later playback. Ineligible voices admit and advance normally,
subject to the same validation and capacity limits.

Typed commands expose `PlayWithOptions`, `Pause`, `Resume` and `PauseMask`;
the query returns `AudioReply::PauseMask(u32)`. Clearing voices and accepted
output-rate changes preserve the reason mask, including across retained service
device cleanup. This lifetime policy lets reasons outlive individual voices;
callers must explicitly clear their reasons.

Suspended voices contribute neither Activity nor Peak detector input. Ducking
envelopes still advance on every output frame, including silent frames, so hold
and release continue during suspension. Unsuspended and ineligible voices retain
the ordinary detector behavior. This detector policy and mask lifetime are
generic engine choices; they do not establish another engine's ducking or device
behavior. `tests/audio_pause.rs` uses synthetic PCM to check cursor retention,
admission, mask transitions, capacity, detectors and render-block independence.

### Option buses and ducking

`configure_bus(name, BusConfig { gain, muted, tier })` creates or updates a named
bus. Gain is finite in 0..=16; mute advances voices normally. Names contain
1..=128 UTF-8 bytes, are case-sensitive, and remain resident for the mixer lifetime.
There are at most `MAX_AUDIO_BUSES` (64) buses. `play_on_bus(clip, gain, name)`
routes a voice to exactly one existing bus; an unknown name returns
`AudioError::UnknownBus` carrying that name. Updates affect existing voices.
`play(clip, gain)` remains unrouted and bypasses bus gain and ducking. Its sample
arithmetic and voice order are unchanged, even when buses and rules exist.
Routed voices multiply their interpolated, voice-scaled samples by bus gain and
ducking gain before the creation-order sum. Master gain/mute and clipping follow.

`replace_ducking_rules(Vec<DuckingRule>)` validates and replaces the entire set
atomically. Failed validation leaves the running set and envelopes unchanged.
A successful replacement resets envelopes; an empty set disables ducking. There
are at most `MAX_DUCKING_RULES` (64) rules. Each has nonempty named source and
target sets, each bounded to 64 entries. Repeated names are deduplicated.
Unknown names are errors. No bus is implicitly created by a play or rule.

A rule affects each target using only its sources with a strictly higher numeric
`BusConfig::tier`. Equal or lower tiers cannot duck it. Larger numbers have higher
priority; the engine assigns no special meaning to negative tiers. Exclude a bus
from ducking by leaving it out of source and target sets. Multiple applicable
rules choose the smallest current gain, without multiplying reductions.
Each target has its own envelope so different target tiers work in one rule.

`DuckingRule::gain` is the fully ducked linear gain factor in 0..=1, where 0.25
means one quarter of the amplitude. Convert a positive reduction of `d` decibels
with `10^(-d / 20)`. `attack`, `hold` and `release` are `Duration`s, each at most
`MAX_DUCKING_TIME` (3600 seconds). Each converts to `ceil(seconds * output_rate)`
frames using integer nanosecond arithmetic. Zero attack or release is immediate.

The detector and interpolation controls are independent:

- `DuckingDetector::Activity` demands full ducking while any eligible source
  voice exists, including a silent, zero-gain or muted voice.
- `DuckingDetector::Peak` demands the maximum absolute channel sample among
  eligible source voices, after voice gain, bus gain and bus mute, clamped to
  0..=1. It reads before ducking, master gain and clipping, so rules cannot feed
  back into their own detector. This is per-sample peak detection, not RMS or a
  summed-bus loudness measurement.
- `EnvelopeMode::LinearGain` interpolates reduction linearly in amplitude:
  `gain(depth) = 1 + (rule_gain - 1) * depth`.
- `EnvelopeMode::LinearDb` interpolates in decibels:
  `gain(depth) = rule_gain^depth`. This mode requires `rule_gain > 0`.
  The supported string names are `linear-gain` and `linear-db`; parsing any other
  name returns `UnsupportedEnvelope` carrying the rejected name.

Envelope depth starts at zero. Demand above the current depth ramps up over the
attack duration. A decrease in demand below the current depth freezes that depth
for exactly the hold duration, then ramps down to the demand over release.
A further demand decrease restarts hold; an increase cancels hold. Changed ramp
goals start from the current depth and use attack when moving up, release when
moving down. Each ramp's first mixed frame advances
one step and its Nth frame reaches the exact endpoint. A natural ending ceases
to trigger on the frame after the source's final sample; explicit stop ceases
on the next mixed frame. Rule replacement, `clear_voices` and an accepted output
rate change reset envelopes.

Rendering evaluates detectors and envelopes on every output frame, including
silence. Rule configuration caches descending source-tier order and each target's
eligible prefix; bus tier changes refresh those lookups without resetting envelopes.
Each frame computes source maxima once per rule, then looks up demand for each
target, with work linear in the source and target counts rather than their product.
Dividing the same ordered frames into different caller blocks does not change
results. Rendering allocates no memory, resolves no names, reads no wall clock
and uses no device. Floating-point math is deterministic for the same inputs on
the same target; cross-platform bit identity of `powf` is not promised.

`cargo bench --locked -j 2 -p worldrunner-world-simulation --bench audio_ducking`
runs a CPU-only optimized workload with 64 rules, 64 sources and 64 targets per
rule, and 32 active voices at 48 kHz. It asserts bit-exact output and that rendering
4,800 stereo frames finishes within their 100 ms playback duration. Unoptimized
`cargo test --all-targets` runs check the output without asserting playback speed.
Run the benchmark on an otherwise idle machine to measure playback throughput
without scheduling contention.

For option groups that also have distinct priority presets, create one bus per
(group, preset) pair and apply a group's option gain/mute to each of its buses.
Put all buses using a preset in that preset rule's source set, and enumerate the
intended target buses. Convert the game's priority ordering explicitly to higher
numeric engine tiers. The caller owns authored strength-unit conversion and the
choice of activity or peak detection; interpolation mode is a separate choice.

## Graphics: a draw's material

A script sets values on a material, draws with it, sets others and draws again, but the world holds one state per material and a host replays the frame from a snapshot taken at the flush. A blit, a mesh draw or a procedural draw (`Blit` with a material, `DrawMesh`, `DrawProcedural`) therefore reads the material as it stood when the script recorded it. The record names the material by its own id while the material still holds that state. The first `edit_material` (or `set_material_buffer`) after such draws copies the state they saw and renames them to the copy, under a `MaterialId` of generation 0, which no slot of the world has and which is never reused in the world's life. Several edits between two draws make one copy, and a frame whose draws saw one state of each material makes none and names its materials as before. Every change a material takes is covered: values of every kind, textures, keywords, passes, the render queue, the shader and bound buffers.

`GraphicsSnapshot::take` puts each copy in `materials` under its own id, with its shader and buffers, next to the live materials, so a host maps a copy like any other material and the wire encoding carries it unchanged. `World::take_offscreen` returns the copies with the work, each under the id the records name. A buffer binding in a copy names the compute buffer, and the buffer's contents are the ones the snapshot copies, as before. The records of a camera render and a renderer list read the renderers' materials as the snapshot holds them, not as of the record. A material destroyed after a draw leaves the draw naming the destroyed id, which a host skips as stale, whether or not the script edited the material first.

## Camera renders and the renderers they draw

`Camera.Render` into a render texture draws at once, so a script may switch an object on for the call and off after it, as a bake of tile data into a render texture does.
The world therefore fixes the renderers of a camera render when it records the render.
A render pipeline renders a camera by culling inside `Camera.Render`, so a `Cull` record is fixed the same way, with its own culling mask.
`World::record_offscreen` of a `CameraRender` or a `Cull` captures the mesh renderers that are enabled, not forced off, have a drawn mesh and sit on an object active in the hierarchy, on a layer the camera's culling mask admits.
Records of one mask share a set while nothing that decides drawing changes, so a frame with hundreds of camera renders scans the renderers once for each change, not once for each render.
A counter gives each set its validity. `set_active`, a parent or layer change and a published construction advance it, and so does a mesh renderer or mesh filter that is added or removed while it decides drawing.
Editing a component advances it only when what the component contributes to drawing changes: a renderer's enabled, forced-off or kind setting, or a filter gaining or losing its drawn mesh. A new material, property block or mesh id keeps the set.

`GraphicsSnapshot::take` lists the renderers the sets name, each once, and gives each camera render and each cull its set as indices into `renderers` (`renderer_sets`, `record_sets`, `record_renderers`).
A renderer in a set whose object or components are gone by then is left out.
A renderer that is drawable when the snapshot is taken but was not when a record was made is in no set, so no record draws it.
The world makes a set for every cull and for every camera render whose camera it can read; a record with none draws nothing.
`snapshot_wire::decode` refuses a set that names a renderer past the end of `renderers` with `WireError::RendererOutOfRange`. No encoder writes such a snapshot, so the wire version is unchanged.

The limits: a renderer's pose, materials and property block are read when the snapshot is taken, not when the record is made, and the camera's mask and settings are read then too, except that the mask at the call decides the set's layers.
A script that edits a renderer's material between two camera renders of one frame sees the later value in both.

## Graphics: the global shader state and the offscreen work list

The world keeps one global shader state (`global_shader_state`) and one list of offscreen work for each frame. A direct write (`set_global_property`, `set_global_keyword`) changes the state at once, so reads see it. A host replays the frame's work in order, so a draw would see only the last value of a global that a script changed between two records. The world therefore keeps the names written directly since the previous record. When any record is appended (`record_offscreen`, a readback or renderer list request, or a command), the world first appends an `OffscreenWork::SetGlobal` for each noted value and an `OffscreenWork::SetGlobalKeyword` for each noted keyword, holding the state's value at that point; several writes to one name between two records give one record with the last value. A name set as two kinds, such as a float and a vector, is two values. A record refused for any reason leaves the noted names for the next one. `take_offscreen` appends the names written after the last record in the same way, so every write of a take is a record, and it returns, with the work and the material copies, the global state the records replay from: the state as it was before the take's first write, which the world copies when that write happens (`None` when the take wrote nothing and the state itself is the start; `GraphicsSnapshot::global` holds whichever applies). A draw before the first write of a name in the take therefore reads what the previous take left, and a draw before a trailing write reads the value it was recorded with. A take with no record has nothing to order its writes with: it drops them and the next take starts from the state that holds them, and a new frame (`begin_frame`) drops the work nobody took and the copy with it. The managed engine sets the globals of an executed command buffer with these direct writes, so they outlive the buffer and the take that ran it. The one global the ABI records instead of writing is a texture set to a temporary render texture, which has no identity beyond its take (`disunity_offscreen_set_global_texture`); that `OffscreenWork::SetGlobal` is not folded into the state and is gone when the work is taken.

One limit remains. A `clear_global_properties` removes values from the state, and the work list has no record that removes one, so a cleared name is not ordered: the world removes it from the take's starting copy as well, and a host reads its absence from the first record of the take on.
