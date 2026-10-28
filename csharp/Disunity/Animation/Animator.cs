using System.Text;

namespace UnityEngine;

/// <summary>
/// Unity's animation component. The engine has no animation playback, so an Animator cannot be attached
/// to a GameObject (<c>AddComponent&lt;Animator&gt;</c> throws) and an instance made with <c>new</c> is an
/// animator with no controller state machine: it has no layers, parameters or states, reads zero, empty
/// or null for them, and ignores commands that need a controller with a warning when
/// <see cref="logWarnings"/> is set, as Unity does for a parameter or state that does not exist.
/// Members that would evaluate or record animation throw <see cref="NotSupportedException"/>.
/// The <c>playableGraph</c> property is not defined, because the playable types are not.
/// </summary>
public partial class Animator : Behaviour
{
    public Animator() { }

    public override bool enabled { get; set; } = true;
    public RuntimeAnimatorController? runtimeAnimatorController { get; set; }
    public Avatar? avatar { get; set; }
    public float speed { get; set; } = 1f;
    public AnimatorCullingMode cullingMode { get; set; }
    public AnimatorUpdateMode updateMode { get; set; }
    public bool applyRootMotion { get; set; }
    public bool linearVelocityBlending { get; set; }
    public bool stabilizeFeet { get; set; }
    public bool layersAffectMassCenter { get; set; }
    public float feetPivotActive { get; set; }
    public bool logWarnings { get; set; } = true;
    public bool fireEvents { get; set; } = true;
    public bool keepAnimatorStateOnDisable { get; set; }
    public bool writeDefaultValuesOnDisable { get; set; }
    public Vector3 rootPosition { get; set; }
    public Quaternion rootRotation { get; set; } = Quaternion.identity;
    public Vector3 bodyPosition { get; set; }
    public Quaternion bodyRotation { get; set; } = Quaternion.identity;
    public float playbackTime { get; set; }
    public float recorderStartTime { get; set; }
    public float recorderStopTime { get; set; }

    [Obsolete("Use keepAnimatorStateOnDisable instead.")]
    public bool keepAnimatorControllerStateOnDisable { get => keepAnimatorStateOnDisable; set => keepAnimatorStateOnDisable = value; }

    [Obsolete("Use updateMode instead.")]
    public bool animatePhysics
    {
        get => updateMode == AnimatorUpdateMode.Fixed;
        set => updateMode = value ? AnimatorUpdateMode.Fixed : AnimatorUpdateMode.Normal;
    }

    public bool isOptimizable => false;
    public bool isHuman => avatar is { isHuman: true };
    public bool hasRootMotion => false;
    public float humanScale => 1f;
    public bool isInitialized => false;
    public bool hasTransformHierarchy => true;
    public bool hasBoundPlayables => false;
    public bool isMatchingTarget => false;
    public float gravityWeight => 0f;
    public float pivotWeight => 0f;
    public Vector3 pivotPosition => Vector3.zero;
    public Vector3 deltaPosition => Vector3.zero;
    public Quaternion deltaRotation => Quaternion.identity;
    public Vector3 velocity => Vector3.zero;
    public Vector3 angularVelocity => Vector3.zero;
    public Vector3 targetPosition => Vector3.zero;
    public Quaternion targetRotation => Quaternion.identity;
    public float leftFeetBottomHeight => 0f;
    public float rightFeetBottomHeight => 0f;
    public Transform? avatarRoot => null;
    public AnimatorRecorderMode recorderMode => AnimatorRecorderMode.Offline;
    public int layerCount => 0;
    public int parameterCount => 0;
    public AnimatorControllerParameter[] parameters => [];

    public AnimatorControllerParameter GetParameter(int index) =>
        throw new IndexOutOfRangeException($"Animator.GetParameter: index {index} is out of range; the animator has no parameters");

    /// <summary>The hash Unity gives names of states, layers and parameters: the CRC-32 (IEEE 802.3, as in zlib) of the name's UTF-8 bytes, as a signed int.</summary>
    public static int StringToHash(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        var crc = 0xFFFFFFFFu;
        foreach (var value in Encoding.UTF8.GetBytes(name))
        {
            crc ^= value;
            for (var bit = 0; bit < 8; bit++) crc = (crc & 1) != 0 ? (crc >> 1) ^ 0xEDB88320u : crc >> 1;
        }
        return unchecked((int)~crc);
    }

    public string GetLayerName(int layerIndex) => "";
    public int GetLayerIndex(string layerName) => -1;
    public float GetLayerWeight(int layerIndex) => 0f;
    public bool HasState(int layerIndex, int stateID) => false;
    public bool IsInTransition(int layerIndex) => false;
    public bool IsControlled(Transform transform) => false;
    public Transform? GetBoneTransform(HumanBodyBones humanBoneId) => null;
    public bool IsParameterControlledByCurve(string name) => false;
    public bool IsParameterControlledByCurve(int id) => false;
    public AnimatorStateInfo GetCurrentAnimatorStateInfo(int layerIndex) => default;
    public AnimatorStateInfo GetNextAnimatorStateInfo(int layerIndex) => default;
    public AnimatorTransitionInfo GetAnimatorTransitionInfo(int layerIndex) => default;
    public int GetCurrentAnimatorClipInfoCount(int layerIndex) => 0;
    public int GetNextAnimatorClipInfoCount(int layerIndex) => 0;
    public AnimatorClipInfo[] GetCurrentAnimatorClipInfo(int layerIndex) => [];
    public AnimatorClipInfo[] GetNextAnimatorClipInfo(int layerIndex) => [];
    public void GetCurrentAnimatorClipInfo(int layerIndex, List<AnimatorClipInfo> clips) => Empty(clips);
    public void GetNextAnimatorClipInfo(int layerIndex, List<AnimatorClipInfo> clips) => Empty(clips);
    public T? GetBehaviour<T>() where T : StateMachineBehaviour => null;
    public T[] GetBehaviours<T>() where T : StateMachineBehaviour => [];
    public StateMachineBehaviour[] GetBehaviours(int fullPathHash, int layerIndex) => [];

    private static void Empty(List<AnimatorClipInfo> clips)
    {
        ArgumentNullException.ThrowIfNull(clips);
        clips.Clear();
    }

    private void Ignored(string what)
    {
        if (logWarnings) Debug.LogWarning($"Animator.{what} was ignored: the animator has no controller state machine", this);
    }

    private T Ignored<T>(string what, T result)
    {
        Ignored(what);
        return result;
    }

    private static NotSupportedException Unsupported(string member) =>
        new($"Animator.{member} is not supported yet: the engine plays no animation");
}
