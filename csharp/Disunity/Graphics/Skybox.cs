namespace UnityEngine;

/// <summary>A camera's own sky: a camera that has an enabled Skybox draws its sky with <see cref="material"/> in place of the
/// scene's. The world holds the enabled flag and the component's place on its object; the material stays in this component and
/// the world does not read it yet.</summary>
public sealed class Skybox : Behaviour
{
    private protected override bool EnabledState { get => NativeBehaviourState.Read(Handle); set => NativeBehaviourState.Write(Handle, value); }

    /// <summary>The sky material, or null for none. A new Skybox has none.</summary>
    public Material? material { get; set; }
}
