# G13 — Audio

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6.3 is an unverified compatibility target. Code/signatures are authored illustrative subsets from memory, not full declarations or compile-tested examples. Relevant uncertainties: U001–U003, U040–U042.

**Boundary:** engine audio module (`UnityEngine` and `UnityEngine.Audio`); native audio output, codecs, DSP plugins and platform backends are not specified by the managed facade.

## G13.T01 — AudioSource playback

`AudioSource : Behaviour` supplies clip playback and source processing. Important properties: clip, volume, pitch, loop, playOnAwake, mute, priority, outputAudioMixerGroup, spatialBlend, panStereo, dopplerLevel, spread, minDistance, maxDistance, rolloffMode, bypassEffects, bypassListenerEffects and bypassReverbZones.

```csharp
void Play();
void PlayOneShot(AudioClip clip);
void PlayOneShot(AudioClip clip, float volumeScale);
void Stop();
void Pause();
void UnPause();
void PlayDelayed(float delay);
void PlayScheduled(double time);
void SetScheduledStartTime(double time);
void SetScheduledEndTime(double time);
bool isPlaying { get; }
float time { get; set; }
int timeSamples { get; set; }
```

Play uses the assigned clip. PlayOneShot layers a clip without simply replacing `.clip`, and multiple one-shots can overlap. `volumeScale` multiplies source gain rather than setting a new persistent source volume. Stop versus Pause/UnPause differs in playback-position retention. `isPlaying` during pause, loading, virtualized voices and one-shot overlap requires U040 review; it should not be treated as “this AudioSource owns no future scheduled audio.”

`time` is seconds; `timeSamples` uses sample-frame position in familiar usage, not byte offsets. Seeking accuracy varies with codec/streaming import and can be packet-granular. Pitch can alter effective playback rate. Source priority and voice limits can cause virtualization; inaudibility is not necessarily completion. `AudioSource.PlayClipAtPoint(AudioClip, Vector3, float)` is a convenience that creates/manages a temporary source-like lifetime; it is not a substitute for explicit mixer/source ownership.

## G13.T02 — AudioClip data and loading

`AudioClip` exposes samples, channels, frequency, length, loadType, loadState, preloadAudioData, loadInBackground and ambisonic-related state where available. Length is duration; sample count is per-channel frame count in recalled semantics. Interleaved data arrays contain frame/channel sample values.

```csharp
static AudioClip Create(string name, int lengthSamples, int channels,
                        int frequency, bool stream);
bool GetData(float[] data, int offsetSamples);
bool SetData(float[] data, int offsetSamples);
bool LoadAudioData();
bool UnloadAudioData();
```

Create overloads accept `AudioClip.PCMReaderCallback` and `PCMSetPositionCallback`. The reader fills a supplied float array; the position callback observes seeking/loop-related position changes. Streaming callbacks can run on the audio thread. Do not allocate heavily or call arbitrary engine APIs there. Exact callback thread/lifetime guarantees are U041.

`AudioClipLoadType.DecompressOnLoad`, `CompressedInMemory`, `Streaming` select memory/CPU/latency trade-offs. `AudioDataLoadState.Unloaded`, `Loading`, `Loaded`, `Failed` is a remembered enum. Import settings constrain GetData/SetData and whether decompressed samples are accessible. Do not promise that every compressed/streamed clip supports arbitrary immediate sample access. Boundary wrapping behavior for data reads/writes and modern native-buffer overloads remain U041.

## G13.T03 — Listeners, spatialization, attenuation

`AudioListener` defines the listening transform. In the recalled API, AudioListener.volume and AudioListener.pause are static properties, whereas velocityUpdateMode is an instance property on the listener component. Normal scenes should not have multiple competing enabled listeners unless an explicitly supported setup handles them. `AudioSource.ignoreListenerPause`/ignoreListenerVolume can exempt selected sources in remembered APIs.

`spatialBlend` mixes 2D and 3D behavior, conventionally 0 fully 2D and 1 fully 3D. Panning, distance attenuation, Doppler, spread, source/listener velocity update modes and optional spatializer plugins affect output. `AudioRolloffMode.Logarithmic`, `Linear`, `Custom` are recalled. `SetCustomCurve(AudioSourceCurveType, AnimationCurve)` supports custom attenuation-related curves; enum examples include CustomRolloff, SpatialBlend, ReverbZoneMix, Spread.

MaxDistance is not universally a hard silence cutoff for every rolloff mode. Doppler depends on relative motion and speed-of-sound settings; teleporting a source can cause artifacts. `AudioReverbZone`, `AudioLowPassFilter`, `AudioHighPassFilter`, `AudioEchoFilter`, `AudioDistortionFilter`, `AudioChorusFilter`, and `AudioReverbFilter` are remembered component families. Their exact DSP transfer functions and parameter limits are not specified here.

Spatializer and ambisonic support depend on plugins/platforms and import metadata. Simply applying inverse-square gain and left/right pan is not full Unity spatial audio compatibility.

## G13.T04 — Mixer groups, parameters, snapshots

`UnityEngine.Audio.AudioMixer` routes groups, effects and snapshots; sources point to `AudioMixerGroup`. Important operations:

```csharp
bool SetFloat(string name, float value);
bool GetFloat(string name, out float value);
bool ClearFloat(string name);
AudioMixerGroup[] FindMatchingGroups(string subPath);
AudioMixerSnapshot FindSnapshot(string name);
void TransitionToSnapshots(AudioMixerSnapshot[] snapshots, float[] weights, float timeToReach);
// AudioMixerSnapshot:
void TransitionTo(float timeToReach);
```

Only exposed parameters can be controlled by name through the familiar SetFloat surface. Mixer volume parameters often use decibels; a slider's 0–1 linear amplitude is not numerically the same as dB. A useful application mapping handles silence explicitly rather than taking log10(0). Parameter names and paths are authoring contracts.

Snapshots describe mixer states and transitions; script-set parameter overrides can interact with snapshot control. ClearFloat releases a scripted override in remembered semantics, rather than always assigning zero. Transition timing can follow mixer update mode (`AudioMixerUpdateMode.Normal`, `UnscaledTime` are recalled) and must be verified for paused/scaled games. Loading an AudioMixer asset, discovering groups and connecting source outputs does not implement its native effect graph by itself.

## G13.T05 — DSP scheduling and callbacks

`AudioSettings.dspTime` is a double-precision DSP clock. `PlayScheduled` consumes an absolute DSP time, unlike PlayDelayed's relative delay. In recalled behavior, setting AudioListener.pause freezes dspTime and pauses scheduled playback along with affected sources; this differs from setting Time.timeScale to zero, which does not itself pause the audio clock. Sources configured to ignore listener pause are an exception to listener-controlled playback pausing, not a reason to treat dspTime as an always-advancing wall clock. Schedule sufficiently in advance for streaming/decompression/output buffering; a main-thread frame arriving late cannot guarantee sample-accurate starts in the past. Device/background suspension details remain U042.

`AudioSettings.outputSampleRate`, `GetDSPBufferSize(out int bufferLength, out int numBuffers)`, configuration retrieval/reset, and audio-configuration-change notification are relevant. Exact reset behavior and platform support are U042. A device switch can change sample rate/buffers and disrupt assumptions about a previously measured clock/output path.

`OnAudioFilterRead(float[] data, int channels)` is a MonoBehaviour audio-processing callback. Data is interleaved and work occurs on the audio thread in common behavior. Avoid blocking, logging heavily, allocating per block, sharing mutable unsynchronized buffers, or touching thread-affine engine objects. It can act as a procedural source in some configurations; exact pipeline placement and channel layouts are not a blanket guarantee.

`GetOutputData` and `GetSpectrumData` exist on AudioSource and AudioListener in array-filling forms. `FFTWindow` remembered values include Rectangular, Triangle, Hamming, Hanning, Blackman, BlackmanHarris. These are analysis windows, not playback effects. Buffer sizes and channel indexing have constraints. Analysis snapshots are not a latency-free record of the exact physical speaker signal.

## G13.T06 — Import, platform, lifecycle contracts

Audio clip/source/mixer identity, decompressed data, streaming state, virtual voices, scheduled events, DSP graph resources and physical device output have different lifetimes. Destroying a source, unloading a clip, pausing the listener and setting timeScale zero are different actions. TimeScale does not generally pause audio by itself; an application can coordinate AudioListener.pause and source policies.

Microphone APIs (`Microphone.devices`, `Start`, `End`, `IsRecording`, `GetPosition`, `GetDeviceCaps`) depend on permissions and platform support. Capture returns/uses an AudioClip ring buffer in familiar usage; read/write positions and latency must be handled, not treated as a finished immutable clip. Web/mobile platforms impose user-gesture, focus/background, codec and device constraints.

Proposed tests (not executed): Pause/UnPause preserves the intended cursor; one-shots overlap; sample indexing uses channel-interleaved frames; scheduled clips start in DSP-time order under an offline controlled clock; mixer exposed parameter errors are reported; audio filter data is processed without main-thread calls; clip unload fails/behaves safely while references remain. Exact native codec output, pitch resampling, DSP filter response, device latency and compressed asset interoperability remain unknown.
