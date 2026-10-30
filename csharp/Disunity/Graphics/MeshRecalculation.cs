namespace UnityEngine;

public partial class Mesh
{
    /// <summary>Sets <see cref="bounds"/> to the box around every position; an empty mesh gets zero bounds.</summary>
    public void RecalculateBounds()
    {
        RequireReadable();
        if (_positions.Length == 0)
        {
            _bounds = default;
            return;
        }
        var min = _positions[0];
        var max = _positions[0];
        foreach (var position in _positions)
        {
            min = Vector3.Min(min, position);
            max = Vector3.Max(max, position);
        }
        var box = new Bounds();
        box.SetMinMax(min, max);
        _bounds = box;
    }
}
