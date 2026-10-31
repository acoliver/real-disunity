namespace UnityEngine;

public enum ParticleSystemCurveMode
{
    Constant = 0, Curve = 1, TwoCurves = 2, TwoConstants = 3,
}

public enum ParticleSystemGradientMode
{
    Color = 0, Gradient = 1, TwoColors = 2, TwoGradients = 3, RandomColor = 4,
}

public enum ParticleSystemRenderMode
{
    Billboard = 0, Stretch = 1, HorizontalBillboard = 2, VerticalBillboard = 3, Mesh = 4, None = 5,
}

public enum ParticleSystemRenderSpace
{
    View = 0, World = 1, Local = 2, Facing = 3, Velocity = 4,
}

public enum ParticleSystemSortMode
{
    None = 0, Distance = 1, OldestInFront = 2, YoungestInFront = 3,
}

public enum ParticleSystemShapeType
{
    Sphere = 0, Hemisphere = 2, Cone = 4, Box = 5, Mesh = 6, ConeVolume = 8, Circle = 10, SingleSidedEdge = 12,
    MeshRenderer = 13, SkinnedMeshRenderer = 14, BoxShell = 15, BoxEdge = 16, Donut = 17, Rectangle = 18, Sprite = 19,
    SpriteRenderer = 20,
}

public enum ParticleSystemAnimationMode
{
    Grid = 0, Sprites = 1,
}

public enum ParticleSystemAnimationTimeMode
{
    Lifetime = 0, Speed = 1, FPS = 2,
}
