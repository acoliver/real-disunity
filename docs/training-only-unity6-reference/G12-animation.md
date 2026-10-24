# G12 — Animation

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6.3 remains an unverified target. All code/signatures are illustrative authored subsets from memory, not full definitions or compile-tested examples. See U001–U003, U037–U039.

**Boundary:** engine animation/Animator/Playables modules, plus optional packages such as Timeline (`com.unity.timeline`) and Animation Rigging (`com.unity.animation.rigging`). Animator Controller authoring lives mainly in editor APIs. Imported controller/clip/avatar content compatibility is not implied by API coverage.

## G12.T01 — Animator controllers and parameters

`Animator : Behaviour` evaluates an animation graph/controller and writes animated properties. Common references/settings: runtimeAnimatorController, avatar, enabled, speed, applyRootMotion, updateMode, cullingMode, layerCount and parameterCount. `RuntimeAnimatorController` is a runtime asset base; editor `UnityEditor.Animations.AnimatorController` is not a player authoring API. `AnimatorOverrideController` substitutes clips while preserving a controller's structure.

```csharp
void SetFloat(string name, float value);
void SetFloat(int id, float value);
void SetFloat(string name, float value, float dampTime, float deltaTime);
float GetFloat(string name);
void SetInteger(string name, int value);
int GetInteger(string name);
void SetBool(string name, bool value);
bool GetBool(string name);
void SetTrigger(string name);
void ResetTrigger(string name);
static int StringToHash(string name);
```

ID overloads exist across parameter families. Parameter types include `AnimatorControllerParameterType.Float`, `Int`, `Bool`, `Trigger`. Triggers are transition-oriented flags, not persistent booleans with identical semantics. A parameter name mismatch is not an instruction to create a new controller parameter dynamically. Damped SetFloat uses supplied damping/time inputs, not necessarily a direct immediate assignment.

Hash values identify state/parameter paths for efficient access, but collision possibility and persistence must be considered; do not substitute arbitrary .NET GetHashCode. Controller state names, full paths, short names and tags are distinct lookup domains. Controller graphs contain layers, states, transitions, blend trees and behaviors; supplying parameter storage alone is not Animator compatibility.

## G12.T02 — Animator state, layers, transitions

```csharp
void Play(string stateName, int layer, float normalizedTime);
void Play(int stateNameHash, int layer, float normalizedTime);
void CrossFade(string stateName, float normalizedTransitionDuration,
               int layer, float normalizedTimeOffset, float normalizedTransitionTime);
void CrossFadeInFixedTime(string stateName, float fixedTransitionDuration,
                          int layer, float fixedTimeOffset, float normalizedTransitionTime);
AnimatorStateInfo GetCurrentAnimatorStateInfo(int layerIndex);
AnimatorStateInfo GetNextAnimatorStateInfo(int layerIndex);
AnimatorTransitionInfo GetAnimatorTransitionInfo(int layerIndex);
bool IsInTransition(int layerIndex);
float GetLayerWeight(int layerIndex);
void SetLayerWeight(int layerIndex, float weight);
```

These explicit-argument shapes omit familiar shorter/defaulted overloads. Sentinel defaults for normalizedTime and layer selection are subtle and remain U037; do not replace them with guessed zeros. Play changes/evaluates state according to engine evaluation timing, not necessarily an immediate property write of all bound transforms.

`AnimatorStateInfo` includes fullPathHash, shortNameHash, normalizedTime, length, speed, speedMultiplier, tagHash, loop and tests like `IsName`/`IsTag`. For a looping state, integer portions of normalizedTime conventionally count loops and the fraction is phase. Transition duration units differ between normalized CrossFade and CrossFadeInFixedTime. Next-state information is only meaningful when transitioning. Clip-info APIs return `AnimatorClipInfo` with clip and weight; blended weights and state weights are different concepts.

`AvatarMask` restricts layer/body/property influence. Override and additive layers blend differently. `AnimatorCullingMode.AlwaysAnimate`, `CullUpdateTransforms`, `CullCompletely` are remembered. Update-mode spelling across historical `AnimatePhysics` and newer fixed-mode naming is U037; `Normal` and `UnscaledTime` concepts are familiar but exact enum inventory is not certified.

## G12.T03 — Root motion and animation callbacks

`Animator.deltaPosition`, `deltaRotation`, rootPosition/rootRotation, velocity/angularVelocity and `applyRootMotion` expose or control root movement. `OnAnimatorMove()` allows script handling after animation/root-motion evaluation. `OnAnimatorIK(int layerIndex)` provides an IK update hook for configured layers/rigs. Do not directly add root motion to Transform and also let the animator apply it unintentionally.

IK APIs include `SetIKPosition(AvatarIKGoal, Vector3)`, `SetIKPositionWeight`, `SetIKRotation`, `SetIKRotationWeight`, `SetLookAtPosition`, and `SetLookAtWeight` families. `AvatarIKGoal.LeftFoot`, `RightFoot`, `LeftHand`, `RightHand` are remembered. Humanoid requirements and layer IK-pass settings matter; a generic skeleton is not automatically a valid humanoid IK rig.

`StateMachineBehaviour` provides engine-driven callbacks such as `OnStateEnter(Animator, AnimatorStateInfo, int)`, `OnStateUpdate`, `OnStateExit`, `OnStateMove`, and `OnStateIK`. They are not MonoBehaviour Update methods. Transitions can involve multiple active states; callbacks cannot be simplified to exactly one active state's script per frame. Callback ordering, evaluation threading, and root-motion/physics synchronization are U038.

`AnimationEvent` invokes a named function at clip times, with supported payload categories such as float/int/string/object or an AnimationEvent argument. Events are not arbitrary serialized delegates. Loop boundaries, seeking, reverse playback and skipped intervals require explicit verification.

## G12.T04 — Clips, curves, legacy Animation

`AnimationClip` contains duration, frameRate, looping/legacy metadata, bounds, animation events and animated curves. `AnimationCurve` holds `Keyframe` values with time/value/tangents/weights in version-dependent forms. Common operations: `Evaluate(float time)`, `AddKey`, `MoveKey`, `RemoveKey`, `.keys`, `preWrapMode`, `postWrapMode`; factory helpers include Linear, Constant and EaseInOut.

`WrapMode` includes `Default`, `Once`, `Loop`, `PingPong`, `ClampForever`, with historical aliases/details to check. Clip sampling/evaluation is more than linear interpolation: tangents, weighted curves, discontinuities, object reference curves and quaternion channels matter. `AnimationClip.SetCurve(string relativePath, Type type, string propertyName, AnimationCurve curve)` is a remembered runtime shape, but runtime modification limitations differ for legacy/non-legacy clips; editor `AnimationUtility` handles broader authoring.

Legacy `Animation : Behaviour` uses named `AnimationState` entries and clips marked/configured for the legacy system. Familiar methods include `Play`, `CrossFade`, `Blend`, `Stop`, `IsPlaying`, `AddClip`, `RemoveClip`, and a string indexer returning AnimationState. State includes speed, time, normalizedTime, weight, layer, wrapMode, blendMode and enabled. It is separate from Mecanim Animator; an Animator controller is not consumable as an AnimationState collection.

`AnimationClip.SampleAnimation(GameObject go, float time)` samples onto object properties; it does not emulate a whole controller transition graph, lifecycle, or root-motion policy. Editor preview sampling should restore state through editor animation facilities rather than permanently mutating authored objects.

## G12.T05 — Playables and Timeline boundaries

Engine `UnityEngine.Playables` provides `PlayableGraph`, `Playable`, `PlayableHandle`, `PlayableOutput`, `ScriptPlayable<T>`, `PlayableBehaviour`, and `PlayableDirector`. `UnityEngine.Animations` includes `AnimationClipPlayable`, `AnimationMixerPlayable`, `AnimationLayerMixerPlayable`, and `AnimationPlayableOutput`. Typed playable structs are handles into a graph, not standalone owned animation resources.

```csharp
// Selected PlayableGraph shapes.
static PlayableGraph Create(string name);
void Play();
void Stop();
void Evaluate(float deltaTime);
void Destroy();
bool IsValid();
void SetTimeUpdateMode(DirectorUpdateMode value);
```

Graph creation, node connection, input weights, output targets, explicit evaluation and destruction define lifetime. `DirectorUpdateMode` remembered values include DSPClock, GameTime, UnscaledGameTime, Manual. A copied graph handle does not duplicate the graph. Destroy the graph once and treat child handles as invalid afterward. Exact callback/evaluation order is U039.

`PlayableDirector` binds a PlayableAsset to scene objects, supports Play/Pause/Resume/Stop/Evaluate and time-related properties. Timeline builds on this through package assets, tracks, clips, signals and bindings. Reproducing a generic graph scheduler does not load every Timeline asset or implement every track. Animation Rigging adds constraint/jobs workflows with another package boundary.

## G12.T06 — Humanoid rigs, skinning, compatibility

`Avatar`, `HumanBodyBones`, `Animator.GetBoneTransform(HumanBodyBones)`, `HumanPoseHandler`, `HumanPose`, and human-description-related data support humanoid retargeting. Rig mapping, muscle limits, root conventions and valid avatar construction are substantial native systems. Bone names alone do not establish a valid Humanoid avatar.

`SkinnedMeshRenderer` uses bones/rootBone/sharedMesh, bind poses, bone weights, localBounds, updateWhenOffscreen and blend shape weights (`SetBlendShapeWeight`, `GetBlendShapeWeight`). Mesh bone weighting and deformation may be computed on different CPU/GPU paths. A bounding box affects culling; an animated mesh can disappear if bounds are stale. Animation evaluation, skinning and rendering visibility are separate stages.

Proposed tests (not executed): parameter storage and trigger consumption against a minimal controller; normalized loop phase; CrossFade normalized versus fixed-time duration; layer weights; root-motion applied exactly once; clip event at a loop boundary; curve interpolation/wrap; graph disposal; and bone-transform results under a known hierarchy. Exact imported clip/controller/avatar formats, humanoid retargeting, animation compression, deterministic curve evaluation and package Timeline schemas remain unspecified.
