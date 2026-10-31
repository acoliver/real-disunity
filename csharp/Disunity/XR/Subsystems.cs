namespace UnityEngine;

/// <summary>The XR family reports the missing device in one wording.</summary>
internal static class NoXrDevice
{
    internal static NotSupportedException Member(string member) =>
        new($"{member}: the engine has no XR device");
}

/// <summary>A subsystem the engine runs on behalf of a device or service.</summary>
public interface ISubsystem
{
    bool running { get; }
    void Start();
    void Stop();
    void Destroy();
}

public interface ISubsystem<TSubsystemDescriptor> : ISubsystem where TSubsystemDescriptor : ISubsystemDescriptor
{
    TSubsystemDescriptor SubsystemDescriptor { get; }
}

public interface ISubsystemDescriptor
{
    string id { get; }
    ISubsystem Create();
}

public interface ISubsystemDescriptor<TSubsystem> : ISubsystemDescriptor where TSubsystem : ISubsystem
{
    new TSubsystem Create();
}

/// <summary>
/// A subsystem a native runtime provides. The engine has no XR device and no native subsystem runtime, so no
/// instance is ever registered: <see cref="running"/> is false, <see cref="Start"/> throws
/// <see cref="NotSupportedException"/>, and <see cref="Stop"/> and <see cref="Destroy"/> do nothing on a
/// subsystem that never ran.
/// </summary>
public abstract class IntegratedSubsystem : ISubsystem
{
    public bool running => false;

    public void Start() => throw NoXrDevice.Member($"{GetType().Name}.{nameof(Start)}");

    public void Stop() { }

    public void Destroy() { }
}

public abstract class IntegratedSubsystem<TSubsystemDescriptor> : IntegratedSubsystem
    where TSubsystemDescriptor : ISubsystemDescriptor
{
    public TSubsystemDescriptor subsystemDescriptor =>
        throw NoXrDevice.Member($"{GetType().Name}.{nameof(subsystemDescriptor)}");
}

/// <summary>
/// Describes a subsystem a native runtime can create. A descriptor has an empty <see cref="id"/> and cannot
/// <see cref="Create"/> an instance: the engine has no XR device.
/// </summary>
public abstract class IntegratedSubsystemDescriptor : ISubsystemDescriptor
{
    public string id => string.Empty;

    ISubsystem ISubsystemDescriptor.Create() => throw NoXrDevice.Member($"{GetType().Name}.Create");
}

public abstract class IntegratedSubsystemDescriptor<TSubsystem> : IntegratedSubsystemDescriptor, ISubsystemDescriptor<TSubsystem>
    where TSubsystem : IntegratedSubsystem
{
    public TSubsystem Create() => throw NoXrDevice.Member($"{GetType().Name}.{nameof(Create)}");
}

/// <summary>
/// The registry of subsystems and their descriptors. The engine has no XR device and loads no subsystem
/// runtime, so every query clears the list it is given and leaves it empty (the engine's choice). The
/// reload events never fire, because nothing reloads.
/// </summary>
public static class SubsystemManager
{
    public static event Action? beforeReloadSubsystems { add { } remove { } }
    public static event Action? afterReloadSubsystems { add { } remove { } }
    public static event Action? reloadSubsytemsStarted { add { } remove { } }
    public static event Action? reloadSubsytemsCompleted { add { } remove { } }

    public static void GetAllSubsystemDescriptors(List<ISubsystemDescriptor> descriptors) => Clear(descriptors);

    public static void GetSubsystemDescriptors<T>(List<T> descriptors) where T : ISubsystemDescriptor => Clear(descriptors);

    public static void GetSubsystems<T>(List<T> subsystems) where T : ISubsystem => Clear(subsystems);

    public static void GetInstances<T>(List<T> subsystems) where T : ISubsystem => Clear(subsystems);

    private static void Clear<T>(List<T> list)
    {
        ArgumentNullException.ThrowIfNull(list);
        list.Clear();
    }
}
