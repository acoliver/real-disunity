namespace UnityEngine;

/// <summary>An Animator's parameters, state commands and the members that need playback. See <see cref="Animator"/>.</summary>
public partial class Animator
{
    public float GetFloat(string name) => Ignored($"GetFloat({name})", 0f);
    public float GetFloat(int id) => Ignored($"GetFloat({id})", 0f);
    public bool GetBool(string name) => Ignored($"GetBool({name})", false);
    public bool GetBool(int id) => Ignored($"GetBool({id})", false);
    public int GetInteger(string name) => Ignored($"GetInteger({name})", 0);
    public int GetInteger(int id) => Ignored($"GetInteger({id})", 0);
    public Vector3 GetVector(string name) => Ignored($"GetVector({name})", Vector3.zero);
    public Vector3 GetVector(int id) => Ignored($"GetVector({id})", Vector3.zero);
    public Quaternion GetQuaternion(string name) => Ignored($"GetQuaternion({name})", Quaternion.identity);
    public Quaternion GetQuaternion(int id) => Ignored($"GetQuaternion({id})", Quaternion.identity);

    public void SetFloat(string name, float value) => Ignored($"SetFloat({name})");
    public void SetFloat(string name, float value, float dampTime, float deltaTime) => Ignored($"SetFloat({name})");
    public void SetFloat(int id, float value) => Ignored($"SetFloat({id})");
    public void SetFloat(int id, float value, float dampTime, float deltaTime) => Ignored($"SetFloat({id})");
    public void SetBool(string name, bool value) => Ignored($"SetBool({name})");
    public void SetBool(int id, bool value) => Ignored($"SetBool({id})");
    public void SetInteger(string name, int value) => Ignored($"SetInteger({name})");
    public void SetInteger(int id, int value) => Ignored($"SetInteger({id})");
    public void SetTrigger(string name) => Ignored($"SetTrigger({name})");
    public void SetTrigger(int id) => Ignored($"SetTrigger({id})");
    public void ResetTrigger(string name) => Ignored($"ResetTrigger({name})");
    public void ResetTrigger(int id) => Ignored($"ResetTrigger({id})");
    public void SetVector(string name, Vector3 value) => Ignored($"SetVector({name})");
    public void SetVector(int id, Vector3 value) => Ignored($"SetVector({id})");
    public void SetQuaternion(string name, Quaternion value) => Ignored($"SetQuaternion({name})");
    public void SetQuaternion(int id, Quaternion value) => Ignored($"SetQuaternion({id})");
    public void SetLayerWeight(int layerIndex, float weight) => Ignored("SetLayerWeight");

    public void Play(string stateName) => Ignored($"Play({stateName})");
    public void Play(string stateName, int layer) => Ignored($"Play({stateName})");
    public void Play(string stateName, int layer, float normalizedTime) => Ignored($"Play({stateName})");
    public void Play(int stateNameHash) => Ignored($"Play({stateNameHash})");
    public void Play(int stateNameHash, int layer) => Ignored($"Play({stateNameHash})");
    public void Play(int stateNameHash, int layer, float normalizedTime) => Ignored($"Play({stateNameHash})");
    public void PlayInFixedTime(string stateName) => Ignored($"PlayInFixedTime({stateName})");
    public void PlayInFixedTime(string stateName, int layer) => Ignored($"PlayInFixedTime({stateName})");
    public void PlayInFixedTime(string stateName, int layer, float fixedTime) => Ignored($"PlayInFixedTime({stateName})");
    public void PlayInFixedTime(int stateNameHash) => Ignored($"PlayInFixedTime({stateNameHash})");
    public void PlayInFixedTime(int stateNameHash, int layer) => Ignored($"PlayInFixedTime({stateNameHash})");
    public void PlayInFixedTime(int stateNameHash, int layer, float fixedTime) => Ignored($"PlayInFixedTime({stateNameHash})");

    public void CrossFade(string stateName, float normalizedTransitionDuration) => Ignored($"CrossFade({stateName})");
    public void CrossFade(string stateName, float normalizedTransitionDuration, int layer) => Ignored($"CrossFade({stateName})");
    public void CrossFade(string stateName, float normalizedTransitionDuration, int layer, float normalizedTimeOffset) => Ignored($"CrossFade({stateName})");
    public void CrossFade(string stateName, float normalizedTransitionDuration, int layer, float normalizedTimeOffset, float normalizedTransitionTime) => Ignored($"CrossFade({stateName})");
    public void CrossFade(int stateHashName, float normalizedTransitionDuration) => Ignored($"CrossFade({stateHashName})");
    public void CrossFade(int stateHashName, float normalizedTransitionDuration, int layer) => Ignored($"CrossFade({stateHashName})");
    public void CrossFade(int stateHashName, float normalizedTransitionDuration, int layer, float normalizedTimeOffset) => Ignored($"CrossFade({stateHashName})");
    public void CrossFade(int stateHashName, float normalizedTransitionDuration, int layer, float normalizedTimeOffset, float normalizedTransitionTime) => Ignored($"CrossFade({stateHashName})");
    public void CrossFadeInFixedTime(string stateName, float fixedTransitionDuration) => Ignored($"CrossFadeInFixedTime({stateName})");
    public void CrossFadeInFixedTime(string stateName, float fixedTransitionDuration, int layer) => Ignored($"CrossFadeInFixedTime({stateName})");
    public void CrossFadeInFixedTime(string stateName, float fixedTransitionDuration, int layer, float fixedTimeOffset) => Ignored($"CrossFadeInFixedTime({stateName})");
    public void CrossFadeInFixedTime(string stateName, float fixedTransitionDuration, int layer, float fixedTimeOffset, float normalizedTransitionTime) => Ignored($"CrossFadeInFixedTime({stateName})");
    public void CrossFadeInFixedTime(int stateHashName, float fixedTransitionDuration) => Ignored($"CrossFadeInFixedTime({stateHashName})");
    public void CrossFadeInFixedTime(int stateHashName, float fixedTransitionDuration, int layer) => Ignored($"CrossFadeInFixedTime({stateHashName})");
    public void CrossFadeInFixedTime(int stateHashName, float fixedTransitionDuration, int layer, float fixedTimeOffset) => Ignored($"CrossFadeInFixedTime({stateHashName})");
    public void CrossFadeInFixedTime(int stateHashName, float fixedTransitionDuration, int layer, float fixedTimeOffset, float normalizedTransitionTime) => Ignored($"CrossFadeInFixedTime({stateHashName})");
    public void ForceStateNormalizedTime(float normalizedTime) => Ignored("ForceStateNormalizedTime");

    public void Update(float deltaTime) => throw Unsupported(nameof(Update));
    public void Rebind() => throw Unsupported(nameof(Rebind));
    public void ApplyBuiltinRootMotion() => throw Unsupported(nameof(ApplyBuiltinRootMotion));
    public void WriteDefaultValues() => throw Unsupported(nameof(WriteDefaultValues));
    public void ResetControllerState(bool resetParameters) => throw Unsupported(nameof(ResetControllerState));
    public void StartPlayback() => throw Unsupported(nameof(StartPlayback));
    public void StopPlayback() => throw Unsupported(nameof(StopPlayback));
    public void StartRecording(int frameCount) => throw Unsupported(nameof(StartRecording));
    public void StopRecording() => throw Unsupported(nameof(StopRecording));
    public void SetTarget(AvatarTarget targetIndex, float targetNormalizedTime) => throw Unsupported(nameof(SetTarget));
    public void InterruptMatchTarget() => throw Unsupported(nameof(InterruptMatchTarget));
    public void InterruptMatchTarget(bool completeMatch) => throw Unsupported(nameof(InterruptMatchTarget));
    public void SetBoneLocalRotation(HumanBodyBones humanBoneId, Quaternion rotation) => throw Unsupported(nameof(SetBoneLocalRotation));

    public void MatchTarget(Vector3 matchPosition, Quaternion matchRotation, AvatarTarget targetBodyPart,
        MatchTargetWeightMask weightMask, float startNormalizedTime) => throw Unsupported(nameof(MatchTarget));

    public void MatchTarget(Vector3 matchPosition, Quaternion matchRotation, AvatarTarget targetBodyPart,
        MatchTargetWeightMask weightMask, float startNormalizedTime, float targetNormalizedTime) => throw Unsupported(nameof(MatchTarget));

    public void MatchTarget(Vector3 matchPosition, Quaternion matchRotation, AvatarTarget targetBodyPart,
        MatchTargetWeightMask weightMask, float startNormalizedTime, float targetNormalizedTime, bool completeMatch) => throw Unsupported(nameof(MatchTarget));

    public Vector3 GetIKPosition(AvatarIKGoal goal) => throw Unsupported(nameof(GetIKPosition));
    public void SetIKPosition(AvatarIKGoal goal, Vector3 goalPosition) => throw Unsupported(nameof(SetIKPosition));
    public Quaternion GetIKRotation(AvatarIKGoal goal) => throw Unsupported(nameof(GetIKRotation));
    public void SetIKRotation(AvatarIKGoal goal, Quaternion goalRotation) => throw Unsupported(nameof(SetIKRotation));
    public float GetIKPositionWeight(AvatarIKGoal goal) => throw Unsupported(nameof(GetIKPositionWeight));
    public void SetIKPositionWeight(AvatarIKGoal goal, float value) => throw Unsupported(nameof(SetIKPositionWeight));
    public float GetIKRotationWeight(AvatarIKGoal goal) => throw Unsupported(nameof(GetIKRotationWeight));
    public void SetIKRotationWeight(AvatarIKGoal goal, float value) => throw Unsupported(nameof(SetIKRotationWeight));
    public Vector3 GetIKHintPosition(AvatarIKHint hint) => throw Unsupported(nameof(GetIKHintPosition));
    public void SetIKHintPosition(AvatarIKHint hint, Vector3 hintPosition) => throw Unsupported(nameof(SetIKHintPosition));
    public float GetIKHintPositionWeight(AvatarIKHint hint) => throw Unsupported(nameof(GetIKHintPositionWeight));
    public void SetIKHintPositionWeight(AvatarIKHint hint, float value) => throw Unsupported(nameof(SetIKHintPositionWeight));
    public void SetLookAtPosition(Vector3 lookAtPosition) => throw Unsupported(nameof(SetLookAtPosition));
    public void SetLookAtWeight(float weight) => throw Unsupported(nameof(SetLookAtWeight));
    public void SetLookAtWeight(float weight, float bodyWeight) => throw Unsupported(nameof(SetLookAtWeight));
    public void SetLookAtWeight(float weight, float bodyWeight, float headWeight) => throw Unsupported(nameof(SetLookAtWeight));
    public void SetLookAtWeight(float weight, float bodyWeight, float headWeight, float eyesWeight) => throw Unsupported(nameof(SetLookAtWeight));
    public void SetLookAtWeight(float weight, float bodyWeight, float headWeight, float eyesWeight, float clampWeight) => throw Unsupported(nameof(SetLookAtWeight));
}
