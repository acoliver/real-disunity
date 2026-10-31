namespace UnityEngine;

/// <summary>
/// Adds the components a type's <see cref="RequireComponent"/> attributes name before <see cref="GameObject.AddComponent(Type)"/>
/// creates it. The attribute is read on the type and on every base type, base types first. A required type that is
/// <see cref="Transform"/> itself is skipped because every object has a transform. A required type that is abstract, an
/// interface or not a <see cref="Component"/> cannot be added, so the engine logs an error naming both types and carries on
/// with the requesting component. A cycle of requirements adds each type once.
/// </summary>
internal static class RequiredComponents
{
    public static void Satisfy(GameObject owner, Type requester) => Satisfy(owner, requester, [requester]);

    private static void Satisfy(GameObject owner, Type requester, HashSet<Type> inProgress)
    {
        foreach (var required in Required(requester))
        {
            if (required == typeof(Transform) || ComponentQueries.On(owner, required).Any()) continue;
            if (!typeof(Component).IsAssignableFrom(required) || required.IsAbstract)
            {
                Debug.LogError($"{requester} requires {required}, which cannot be added to a GameObject, so it was not added", owner);
                continue;
            }
            if (!inProgress.Add(required)) continue;
            Satisfy(owner, required, inProgress);
            owner.AttachNew(required);
            inProgress.Remove(required);
        }
    }

    private static IEnumerable<Type> Required(Type type)
    {
        var chain = new List<Type>();
        for (var current = type; current is not null; current = current.BaseType) chain.Insert(0, current);
        return chain
            .SelectMany(level => level.GetCustomAttributes(typeof(RequireComponent), inherit: false).Cast<RequireComponent>())
            .SelectMany(attribute => new[] { attribute.m_Type0, attribute.m_Type1, attribute.m_Type2 })
            .OfType<Type>();
    }
}
