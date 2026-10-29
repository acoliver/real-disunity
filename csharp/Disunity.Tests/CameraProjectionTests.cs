using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Camera lens state, projection and view matrices, rectangles and point conversions.</summary>
internal static class CameraProjectionTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            NativeSettings();
            PerspectiveMatrix();
            OrthographicMatrix();
            ViewMatrixOfMovedAndRotatedCamera();
            CustomMatrices();
            Console.WriteLine("PASS: Camera lens settings, projection and view matrices against hand-computed values");
            RectsAndAspect();
            Console.WriteLine("PASS: Camera rect, pixelRect, pixel sizes and aspect");
            PerspectiveConversions();
            OrthographicConversions();
            ViewportRect();
            EyeOverloads();
            Console.WriteLine("PASS: Camera WorldToScreenPoint, ScreenToWorldPoint, viewport conversions and rays");
        }
        finally
        {
            DisunityHost.SetScreen(800, 600, 96f, 60f);
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera(Vector3? position = null)
    {
        var owner = DisunityHost.CreateGameObject();
        if (position is { } at) owner.transform.position = at;
        return owner.AddComponent<Camera>();
    }

    private static void Near(Vector3 expected, Vector3 actual, string what, float tolerance = 1e-3f) =>
        Ensure((expected - actual).magnitude <= tolerance, $"{what}: expected {expected}, got {actual}");

    private static void Near(float expected, float actual, string what, float tolerance = 1e-4f) =>
        Ensure(MathF.Abs(expected - actual) <= tolerance, $"{what}: expected {expected}, got {actual}");

    private static void NativeSettings()
    {
        var camera = NewCamera();
        Ensure(camera.orthographic && camera.fieldOfView == 60f && camera.cullingMask == -1, "native defaults");
        Ensure(camera.allowHDR && camera.allowMSAA && camera.useOcclusionCulling && !camera.allowDynamicResolution, "native flag defaults");
        Ensure(camera.cameraType == CameraType.Game && camera.targetDisplay == 0 && camera.depthTextureMode == DepthTextureMode.None, "native type defaults");
        Ensure(camera.rect == new Rect(0, 0, 1, 1) && camera.targetTexture is null && camera.activeTexture is null, "native target defaults");
        camera.orthographic = false;
        camera.fieldOfView = 42.5f;
        camera.orthographicSize = 7f;
        camera.cullingMask = 0b1010;
        camera.depthTextureMode = DepthTextureMode.Depth | DepthTextureMode.MotionVectors;
        camera.allowHDR = false;
        camera.allowMSAA = false;
        camera.allowDynamicResolution = true;
        camera.useOcclusionCulling = false;
        camera.targetDisplay = 3;
        camera.cameraType = CameraType.Game | CameraType.Preview;
        Ensure(!camera.orthographic && camera.fieldOfView == 42.5f && camera.orthographicSize == 7f, "lens round trip");
        Ensure(camera.cullingMask == 0b1010, "culling mask round trip");
        Ensure(camera.depthTextureMode == (DepthTextureMode.Depth | DepthTextureMode.MotionVectors), "depth texture mode");
        Ensure(!camera.allowHDR && !camera.allowMSAA && camera.allowDynamicResolution && !camera.useOcclusionCulling, "flag round trip");
        Ensure(camera.targetDisplay == 3 && camera.cameraType == (CameraType.Game | CameraType.Preview), "display and type round trip");
        Throws<DisunityException>(() => camera.fieldOfView = 180f, "field of view of 180");
        Ensure(camera.fieldOfView == 42.5f, "a rejected field of view changes nothing");
        camera.cullingMask = -1;
        Ensure(camera.cullingMask == -1, "all layers");
    }

    private static void PerspectiveMatrix()
    {
        DisunityHost.SetScreen(1600, 800, 96f, 60f);
        var camera = NewCamera();
        camera.orthographic = false;
        camera.fieldOfView = 60f;
        camera.nearClipPlane = 0.3f;
        camera.farClipPlane = 1000f;
        var m = camera.projectionMatrix;
        Near(0.8660254f, m.m00, "perspective m00 = cot(30 degrees) / 2");
        Near(1.7320508f, m.m11, "perspective m11");
        Near(-1.0006002f, m.m22, "perspective m22 = -(f+n)/(f-n)");
        Near(-1f, m.m32, "perspective m32");
        Near(-0.60018f, m.m23, "perspective m23 = -2fn/(f-n)");
        Near(0f, m.m33, "perspective m33");
        Near(0f, m.m01 + m.m02 + m.m03 + m.m10 + m.m12 + m.m13 + m.m20 + m.m21 + m.m30 + m.m31, "perspective off-diagonal terms");
        Ensure(m == Matrix4x4.Perspective(60f, 2f, 0.3f, 1000f), "perspective matches Matrix4x4.Perspective");
    }

    private static void OrthographicMatrix()
    {
        DisunityHost.SetScreen(1600, 800, 96f, 60f);
        var camera = NewCamera();
        camera.orthographicSize = 5f;
        camera.nearClipPlane = 0.3f;
        camera.farClipPlane = 1000f;
        var m = camera.projectionMatrix;
        Near(0.1f, m.m00, "orthographic m00 = 1 / (size * aspect)");
        Near(0.2f, m.m11, "orthographic m11 = 1 / size");
        Near(-0.0020006f, m.m22, "orthographic m22 = -2/(f-n)", 1e-6f);
        Near(-1.0006002f, m.m23, "orthographic m23 = -(f+n)/(f-n)");
        Near(1f, m.m33, "orthographic m33");
        Near(0f, m.m03 + m.m13 + m.m32, "orthographic centred");
    }

    private static void ViewMatrixOfMovedAndRotatedCamera()
    {
        var camera = NewCamera();
        Ensure(camera.worldToCameraMatrix == Matrix4x4.Scale(new Vector3(1, 1, -1)), "an unmoved camera looks down negative Z");
        camera.transform.position = new Vector3(1, 2, 3);
        camera.transform.rotation = Quaternion.Euler(0, 90, 0);
        var v = camera.worldToCameraMatrix;
        Near(-1f, v.m02, "view m02");
        Near(3f, v.m03, "view m03");
        Near(1f, v.m11, "view m11");
        Near(-2f, v.m13, "view m13");
        Near(-1f, v.m20, "view m20");
        Near(1f, v.m23, "view m23");
        Near(1f, v.m33, "view m33");
        Near(0f, v.m00 + v.m01 + v.m10 + v.m12 + v.m21 + v.m22, "view zero terms");
        Near(-5f, v.MultiplyPoint3x4(new Vector3(6, 2, 3)).z, "a point 5 units along the camera's forward axis is at z = -5");
        Ensure(camera.cameraToWorldMatrix.MultiplyPoint3x4(new Vector3(0, 0, -5)) == new Vector3(6, 2, 3), "cameraToWorldMatrix inverts it");
        Ensure(camera.cameraToWorldMatrix.GetPosition() == new Vector3(1, 2, 3), "camera position from cameraToWorldMatrix");

        // The camera at (1, 2, 3) looks along +X; its near plane faces +X at x = 1 + near.
        var planes = GeometryUtility.CalculateFrustumPlanes(camera);
        Near(new Vector3(1, 0, 0), planes[4].normal, "the near plane faces the camera's forward axis");
        Near(-(1f + camera.nearClipPlane), planes[4].distance, "the near plane sits nearClipPlane ahead of the camera");
        Ensure(GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(6, 2, 3), Vector3.one)), "a box ahead of the camera is inside");
        Ensure(!GeometryUtility.TestPlanesAABB(planes, new Bounds(new Vector3(-4, 2, 3), Vector3.one)), "a box behind the camera is outside");
        var span = new Plane[6];
        GeometryUtility.CalculateFrustumPlanes(camera, span.AsSpan());
        Ensure(span.SequenceEqual(planes), "the Span overload gives the same planes");
    }

    private static void CustomMatrices()
    {
        var camera = NewCamera();
        var derived = camera.projectionMatrix;
        var custom = Matrix4x4.Perspective(90f, 1f, 0.1f, 10f);
        camera.projectionMatrix = custom;
        Ensure(camera.projectionMatrix == custom && camera.nonJitteredProjectionMatrix == custom, "custom projection is kept");
        camera.orthographicSize = 9f;
        Ensure(camera.projectionMatrix == custom, "a custom projection stays until reset");
        camera.ResetProjectionMatrix();
        Ensure(camera.projectionMatrix != custom && camera.projectionMatrix != derived, "reset derives the projection from the lens again");
        var view = Matrix4x4.TRS(new Vector3(4, 5, 6), Quaternion.identity, Vector3.one);
        camera.worldToCameraMatrix = view;
        Ensure(camera.worldToCameraMatrix == view, "custom view is kept");
        camera.transform.position = new Vector3(100, 0, 0);
        Ensure(camera.worldToCameraMatrix == view, "a custom view ignores the transform");
        camera.ResetWorldToCameraMatrix();
        Ensure(camera.worldToCameraMatrix != view, "reset derives the view from the transform again");
        Throws<DisunityException>(() => camera.projectionMatrix = new Matrix4x4 { m00 = float.NaN }, "a non-finite matrix");
    }

    private static void RectsAndAspect()
    {
        DisunityHost.SetScreen(1600, 800, 96f, 60f);
        var camera = NewCamera();
        Ensure(camera.pixelRect == new Rect(0, 0, 1600, 800), "full-screen pixel rectangle");
        Ensure(camera.pixelWidth == 1600 && camera.pixelHeight == 800, "full-screen pixel size");
        Ensure(camera.scaledPixelWidth == 1600 && camera.scaledPixelHeight == 800, "scaled size without dynamic resolution");
        camera.allowDynamicResolution = true;
        Ensure(camera.scaledPixelWidth == camera.pixelWidth, "dynamic resolution is not applied");
        Near(2f, camera.aspect, "aspect from the screen");
        camera.rect = new Rect(0.25f, 0.5f, 0.5f, 0.5f);
        Ensure(camera.pixelRect == new Rect(400, 400, 800, 400), "partial rectangle in pixels");
        Ensure(camera.pixelWidth == 800 && camera.pixelHeight == 400, "partial pixel size");
        Near(2f, camera.aspect, "aspect of the partial rectangle");
        camera.rect = new Rect(0, 0, 0.25f, 1f);
        Near(0.5f, camera.aspect, "aspect follows the rectangle");
        camera.aspect = 1.5f;
        Near(1.5f, camera.aspect, "aspect set by a script");
        Throws<ArgumentOutOfRangeException>(() => camera.aspect = 0f, "zero aspect", "aspect");
        camera.ResetAspect();
        Near(0.5f, camera.aspect, "ResetAspect returns to the computed aspect");
        camera.pixelRect = new Rect(0, 0, 800, 400);
        Ensure(camera.rect == new Rect(0, 0, 0.5f, 0.5f), "pixelRect setter writes the normalized rect");
        DisunityHost.SetScreen(1920, 1080, 96f, 60f);
        Ensure(camera.pixelRect == new Rect(0, 0, 960, 540), "pixel rectangle follows the screen size");
        Ensure(NewCamera().targetTexture is null, "no target texture by default");
        var texture = new RenderTexture(256, 128, 0);
        var drawn = NewCamera();
        drawn.targetTexture = texture;
        Ensure(drawn.targetTexture == texture && drawn.activeTexture == texture, "target texture round trip");
        Ensure(drawn.pixelWidth == 256 && drawn.pixelHeight == 128, "pixel size comes from the target texture");
        Near(2f, drawn.aspect, "aspect from the target texture");
        drawn.targetTexture = null;
        Ensure(drawn.targetTexture is null && drawn.pixelWidth == 1920, "clearing the target texture returns to the screen");
    }

    private static void PerspectiveConversions()
    {
        DisunityHost.SetScreen(1920, 1080, 96f, 60f);
        var camera = NewCamera(new Vector3(0, 0, -10));
        camera.orthographic = false;
        camera.fieldOfView = 60f;
        Near(new Vector3(960, 540, 10), camera.WorldToScreenPoint(Vector3.zero), "centre of the screen");
        var halfHeight = 10f * MathF.Tan(30f * Mathf.Deg2Rad);
        Near(1080f, camera.WorldToScreenPoint(new Vector3(0, halfHeight, 0)).y, "top edge of the frustum at distance 10", 0.01f);
        Near(1920f, camera.WorldToScreenPoint(new Vector3(halfHeight * 1920f / 1080f, 0, 0)).x, "right edge of the frustum", 0.01f);
        Near(-10f, camera.WorldToScreenPoint(new Vector3(0, 0, -20)).z, "a point behind the camera has negative z");
        Near(new Vector3(0.5f, 0.5f, 10), camera.WorldToViewportPoint(Vector3.zero), "viewport centre");
        foreach (var point in new[] { new Vector3(1, 2, 3), new Vector3(-4, 1, 20), new Vector3(0.5f, -3, 0) })
        {
            Near(point, camera.ScreenToWorldPoint(camera.WorldToScreenPoint(point)), $"screen round trip of {point}", 2e-3f);
            Near(point, camera.ViewportToWorldPoint(camera.WorldToViewportPoint(point)), $"viewport round trip of {point}", 2e-3f);
        }
        Near(new Vector3(0, 0, -10), camera.ScreenToWorldPoint(new Vector3(960, 540, 0)), "z of 0 is the camera position");
        var centre = camera.ScreenPointToRay(new Vector3(960, 540, 0));
        Near(new Vector3(0, 0, -10), centre.origin, "perspective ray starts at the camera");
        Near(new Vector3(0, 0, 1), centre.direction, "central ray looks forward");
        var corner = camera.ScreenPointToRay(new Vector3(1920, 1080, 5));
        var tan = MathF.Tan(30f * Mathf.Deg2Rad);
        Near(tan * 1920f / 1080f, corner.direction.x / corner.direction.z, "corner ray slope in x");
        Near(tan, corner.direction.y / corner.direction.z, "corner ray slope in y");
        var target = new Vector3(2, -1, 4);
        var hit = camera.ScreenPointToRay(camera.WorldToScreenPoint(target)).GetPoint((target - new Vector3(0, 0, -10)).magnitude);
        Near(target, hit, "a ray through a screen point passes the world point", 2e-3f);
        Near(camera.ViewportPointToRay(new Vector3(1, 0.5f, 0)).direction, camera.ScreenPointToRay(new Vector3(1920, 540, 0)).direction, "viewport and screen rays agree");
    }

    private static void OrthographicConversions()
    {
        DisunityHost.SetScreen(1600, 800, 96f, 60f);
        var camera = NewCamera();
        camera.orthographicSize = 5f;
        Near(new Vector3(1600, 800, 3), camera.WorldToScreenPoint(new Vector3(10, 5, 3)), "top right corner of the orthographic view");
        Near(new Vector3(800, 400, 7), camera.WorldToScreenPoint(new Vector3(0, 0, 7)), "orthographic centre");
        Near(new Vector3(0, 0, 0), camera.WorldToScreenPoint(new Vector3(-10, -5, 0)), "bottom left corner");
        var ray = camera.ScreenPointToRay(new Vector3(1600, 800, 0));
        Near(new Vector3(10, 5, 0), ray.origin, "orthographic ray starts on the camera plane at the point");
        Near(new Vector3(0, 0, 1), ray.direction, "orthographic rays are parallel to the view axis");
        Near(new Vector3(3, -2, 8), camera.ScreenToWorldPoint(camera.WorldToScreenPoint(new Vector3(3, -2, 8))), "orthographic round trip");
        camera.transform.position = new Vector3(5, 0, 0);
        Near(new Vector3(800, 400, 1), camera.WorldToScreenPoint(new Vector3(5, 0, 1)), "a moved camera sees its own position at the centre");
    }

    private static void ViewportRect()
    {
        DisunityHost.SetScreen(1600, 800, 96f, 60f);
        var camera = NewCamera();
        camera.orthographicSize = 5f;
        camera.rect = new Rect(0.5f, 0f, 0.5f, 1f);
        Near(new Vector3(1200, 400, 2), camera.WorldToScreenPoint(new Vector3(0, 0, 2)), "the view centre is the centre of the rectangle");
        Near(new Vector3(0.5f, 0.5f, 2), camera.ScreenToViewportPoint(new Vector3(1200, 400, 2)), "screen to viewport inside the rectangle");
        Near(new Vector3(1.0f, 0.5f, 2), camera.ScreenToViewportPoint(new Vector3(1600, 400, 2)), "right edge of the rectangle is viewport 1");
        Near(new Vector3(1200, 400, 2), camera.ViewportToScreenPoint(new Vector3(0.5f, 0.5f, 2)), "viewport to screen");
        Near(new Vector3(0, 0, 0), camera.ScreenToWorldPoint(new Vector3(1200, 400, 0)), "screen to world through the rectangle");
    }

    private static void EyeOverloads()
    {
        var camera = NewCamera();
        Ensure(camera.WorldToScreenPoint(Vector3.one, Camera.MonoOrStereoscopicEye.Mono) == camera.WorldToScreenPoint(Vector3.one), "Mono eye");
        Ensure(camera.ScreenPointToRay(Vector3.one, Camera.MonoOrStereoscopicEye.Mono).direction == camera.ScreenPointToRay(Vector3.one).direction, "Mono eye ray");
        Throws<NotSupportedException>(() => camera.WorldToScreenPoint(Vector3.one, Camera.MonoOrStereoscopicEye.Left), "left eye", "WorldToScreenPoint", "Left");
        Throws<NotSupportedException>(() => camera.WorldToViewportPoint(Vector3.one, Camera.MonoOrStereoscopicEye.Right), "right eye", "WorldToViewportPoint");
        Throws<NotSupportedException>(() => camera.ViewportToWorldPoint(Vector3.one, Camera.MonoOrStereoscopicEye.Left), "left eye", "ViewportToWorldPoint");
        Throws<NotSupportedException>(() => camera.ScreenToWorldPoint(Vector3.one, Camera.MonoOrStereoscopicEye.Left), "left eye", "ScreenToWorldPoint");
        Throws<NotSupportedException>(() => camera.ViewportPointToRay(Vector3.one, Camera.MonoOrStereoscopicEye.Left), "left eye", "ViewportPointToRay");
        Throws<NotSupportedException>(() => camera.ScreenPointToRay(Vector3.one, Camera.MonoOrStereoscopicEye.Right), "right eye", "ScreenPointToRay");
    }
}
