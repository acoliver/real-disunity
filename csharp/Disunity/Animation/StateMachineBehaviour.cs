namespace UnityEngine;

/// <summary>
/// The base of scripts attached to an animator controller's states. The engine plays no controllers, so
/// none of the messages are sent; each does nothing unless a subclass overrides it. The overloads
/// that take an AnimatorControllerPlayable are not defined, because the playable types are not.
/// </summary>
public abstract class StateMachineBehaviour : ScriptableObject
{
    public virtual void OnStateEnter(Animator animator, AnimatorStateInfo stateInfo, int layerIndex) { }
    public virtual void OnStateUpdate(Animator animator, AnimatorStateInfo stateInfo, int layerIndex) { }
    public virtual void OnStateExit(Animator animator, AnimatorStateInfo stateInfo, int layerIndex) { }
    public virtual void OnStateMove(Animator animator, AnimatorStateInfo stateInfo, int layerIndex) { }
    public virtual void OnStateIK(Animator animator, AnimatorStateInfo stateInfo, int layerIndex) { }
    public virtual void OnStateMachineEnter(Animator animator, int stateMachinePathHash) { }
    public virtual void OnStateMachineExit(Animator animator, int stateMachinePathHash) { }
}
