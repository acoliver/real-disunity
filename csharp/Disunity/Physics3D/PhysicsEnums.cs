namespace UnityEngine;

public enum ForceMode { Force = 0, Acceleration = 5, Impulse = 1, VelocityChange = 2 }

public enum RigidbodyConstraints
{
    None = 0, FreezePositionX = 2, FreezePositionY = 4, FreezePositionZ = 8, FreezeRotationX = 16,
    FreezeRotationY = 32, FreezeRotationZ = 64, FreezePosition = 14, FreezeRotation = 112, FreezeAll = 126,
}

public enum CollisionDetectionMode { Discrete = 0, Continuous = 1, ContinuousDynamic = 2, ContinuousSpeculative = 3 }

public enum RigidbodyInterpolation { None = 0, Interpolate = 1, Extrapolate = 2 }

public enum SimulationMode { FixedUpdate = 0, Update = 1, Script = 2 }

public enum PhysicsMaterialCombine { Average = 0, Multiply = 1, Minimum = 2, Maximum = 3 }

[Flags]
public enum MeshColliderCookingOptions
{
    None = 0, InflateConvexMesh = 1, CookForFasterSimulation = 2, EnableMeshCleaning = 4,
    WeldColocatedVertices = 8, UseFastMidphase = 16,
}

public enum SimulationStage : ushort { None = 0, PrepareSimulation = 1, RunSimulation = 2, PublishSimulationResults = 4, All = 7 }

public enum SimulationOption : ushort { None = 0, SyncTransforms = 1, IgnoreEmptyScenes = 2, All = 3 }
