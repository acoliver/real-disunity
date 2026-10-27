fn scene_disk_direction(reflected: vec3<f32>, center: vec3<f32>, radius: f32) -> vec3<f32> {
    let cosine = dot(reflected, center);
    let radius_cosine = cos(0.0027);
    if cosine >= radius_cosine { return reflected; }
    let radial = reflected - cosine * center;
    var tangent = cross(center, select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(center.x) > 0.9));
    if dot(radial, radial) > 0.0 { tangent = radial; }
    return normalize(center * radius_cosine + normalize(tangent) * sin(0.0027));
}
