using System.Runtime.CompilerServices;

namespace UnityEngine.Rendering;

/// <summary>
/// The command buffers attached to a camera or light, by event, in the order they were added.
/// A buffer can be attached to several events and more than once to the same one; removing it
/// from an event removes every attachment there. The engine records no commands into a buffer
/// (each recording member throws), so every attached buffer is empty and the renderer has
/// nothing to run at its event. Attachments belong to the component and are not copied by
/// <c>Instantiate</c>, which is the engine's choice: they are run-time additions, not settings.
/// </summary>
internal sealed class CommandBufferSlots<TEvent> where TEvent : struct, Enum
{
    private static readonly ConditionalWeakTable<Component, CommandBufferSlots<TEvent>> Owners = new();
    private readonly Dictionary<TEvent, List<CommandBuffer>> _slots = new();

    public static CommandBufferSlots<TEvent> Of(Component owner) => Owners.GetValue(owner, _ => new());

    public int Count
    {
        get
        {
            lock (_slots) return _slots.Values.Sum(list => list.Count);
        }
    }

    public void Add(TEvent evt, CommandBuffer buffer, string owner)
    {
        ArgumentNullException.ThrowIfNull(buffer);
        CheckEvent(evt, owner);
        lock (_slots)
        {
            if (!_slots.TryGetValue(evt, out var list)) _slots[evt] = list = new();
            list.Add(buffer);
        }
    }

    public void Remove(TEvent evt, CommandBuffer buffer, string owner)
    {
        ArgumentNullException.ThrowIfNull(buffer);
        CheckEvent(evt, owner);
        lock (_slots)
            if (_slots.TryGetValue(evt, out var list)) list.RemoveAll(b => ReferenceEquals(b, buffer));
    }

    public void RemoveAll(TEvent evt, string owner)
    {
        CheckEvent(evt, owner);
        lock (_slots) _slots.Remove(evt);
    }

    public void Clear()
    {
        lock (_slots) _slots.Clear();
    }

    public CommandBuffer[] Get(TEvent evt, string owner)
    {
        CheckEvent(evt, owner);
        lock (_slots) return _slots.TryGetValue(evt, out var list) ? list.ToArray() : [];
    }

    private static void CheckEvent(TEvent evt, string owner)
    {
        if (!Enum.IsDefined(evt))
            throw new ArgumentOutOfRangeException(nameof(evt), evt, $"{owner}: {Convert.ToInt64(evt)} is not a {typeof(TEvent).Name}");
    }
}
