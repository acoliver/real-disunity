// The Opaque pass's own depth attachment, bound read-only. With reused prepass depth, Opaque
// tests depth Equal against this value without writing it, so a fragment with a greater depth
// fails that test anyway. Discarding it early skips its shading and changes no output.
@group(2) @binding(5) var receiver_scene_depth: texture_depth_2d;

// True only when this fragment and its whole quad lie behind the stored depth, so a discard
// keeps every surviving lane's derivatives. The depth test sees the viewport-clamped depth, so
// the compare clamps too; a NaN depth compares false and is left to the depth test.
// Call it from uniform control flow.
fn scene_receiver_hidden(value: f32) -> bool {
    let stored = textureLoad(receiver_scene_depth, scene_receiver_pixel, 0);
    let behind = f32(clamp(value, 0.0, 1.0) > stored);
    let row = dpdxFine(behind);
    return behind == 1.0 && row == 0.0 && dpdyFine(behind) == 0.0 && dpdyFine(row) == 0.0;
}
