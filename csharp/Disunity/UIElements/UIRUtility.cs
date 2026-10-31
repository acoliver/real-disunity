namespace UnityEngine.UIElements;

/// <summary>Helpers the uGUI bridge uses to tidy up objects it made for UI Toolkit panels.</summary>
public static class UIRUtility
{
    /// <summary>Destroys the object at the end of the frame. A missing object is ignored.</summary>
    public static void Destroy(Object? obj)
    {
        if (obj == null) return;
        Object.Destroy(obj);
    }
}
