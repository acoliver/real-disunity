using Phases = UnityEngine.PlayerLoop;

namespace UnityEngine.PlayerLoop
{
    /// <summary>The phase before anything else in a frame.</summary>
    public struct Initialization { }

    /// <summary>The phase that reads input and the frame's events, before the fixed steps.</summary>
    public struct EarlyUpdate { }

    /// <summary>The fixed steps: physics and the scripts' <c>FixedUpdate</c>.</summary>
    public struct FixedUpdate { }

    /// <summary>The phase after the fixed steps and before the scripts' <c>Update</c>.</summary>
    public struct PreUpdate { }

    /// <summary>The scripts' <c>Update</c>, their coroutines and the main thread's posted work.</summary>
    public struct Update { }

    /// <summary>The scripts' <c>LateUpdate</c>.</summary>
    public struct PreLateUpdate { }

    /// <summary>The end of the frame, before it is drawn.</summary>
    public struct PostLateUpdate { }
}

namespace UnityEngine.LowLevel
{
    /// <summary>The engine's phases and the delegates a loop adds under them.</summary>
    internal static class PlayerLoopPhases
    {
        internal static readonly Type[] Order =
        [
            typeof(Phases.Initialization), typeof(Phases.EarlyUpdate), typeof(Phases.FixedUpdate), typeof(Phases.PreUpdate),
            typeof(Phases.Update), typeof(Phases.PreLateUpdate), typeof(Phases.PostLateUpdate),
        ];

        /// <summary>A root with the seven phases in order and nothing under them.</summary>
        internal static PlayerLoopSystem Default() => new() { subSystemList = Order.Select(phase => new PlayerLoopSystem { type = phase }).ToArray() };

        /// <summary>The systems and their child lists copied, so later edits to <paramref name="system"/>'s arrays do not reach it.</summary>
        internal static PlayerLoopSystem Copy(PlayerLoopSystem system)
        {
            var copy = system;
            copy.subSystemList = system.subSystemList?.Select(Copy).ToArray();
            return copy;
        }

        /// <summary>
        /// The delegates <paramref name="loop"/> adds, by phase in tree order; throws for a loop the engine cannot run. The seven
        /// phases must appear at the top level in their order. A system a game puts at the top level between them runs with the
        /// phase before it, after that phase's own systems; one before <c>Initialization</c> runs first, with <c>Initialization</c>.
        /// </summary>
        internal static Dictionary<Type, PlayerLoopSystem.UpdateFunction[]> Added(PlayerLoopSystem loop)
        {
            const string member = "PlayerLoop.SetPlayerLoop";
            if (loop.updateDelegate is not null) throw new NotSupportedException($"{member}: the root system cannot run a delegate; add it under a phase");
            var systems = loop.subSystemList ?? [];
            if (!systems.Select(system => system.type).Where(Order.Contains).SequenceEqual(Order))
                throw new NotSupportedException($"{member}: the top level must hold the engine's seven phases once each in order ({string.Join(", ", Order.Select(type => type.Name))}), not {string.Join(", ", systems.Select(Name))}; the engine cannot drop or move a phase");
            var added = Order.ToDictionary(phase => phase, _ => new List<PlayerLoopSystem.UpdateFunction>());
            var leading = new List<PlayerLoopSystem.UpdateFunction>();
            Type? current = null;
            foreach (var system in systems)
            {
                if (system.type is { } type && Order.Contains(type)) current = type;
                Collect(system, current is null ? leading : added[current]);
            }
            added[Order[0]].InsertRange(0, leading);
            if (added[typeof(Phases.FixedUpdate)].Count != 0)
                throw new NotSupportedException($"{member}: a system with a delegate under or after FixedUpdate and before PreUpdate is not supported: the engine runs fixed steps natively, with no managed point per step");
            return added.Where(pair => pair.Value.Count != 0).ToDictionary(pair => pair.Key, pair => pair.Value.ToArray());
        }

        private static string Name(PlayerLoopSystem system) => system.type?.FullName ?? "a system without a type";

        private static void Collect(PlayerLoopSystem system, List<PlayerLoopSystem.UpdateFunction> functions)
        {
            if (system.updateDelegate is { } function) functions.Add(function);
            foreach (var child in system.subSystemList ?? []) Collect(child, functions);
        }
    }
}
