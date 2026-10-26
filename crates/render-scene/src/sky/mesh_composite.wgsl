@group(2) @binding(0) var radiance:texture_2d<f32>;
@vertex fn vertex(@builtin(vertex_index) index:u32)->@builtin(position) vec4<f32> {
    let xy=vec2(f32((index<<1u)&2u),f32(index&2u))*2.0-1.0;
    return vec4(xy,1.0,1.0);
}
@fragment fn fragment(@builtin(position) pixel:vec4<f32>,@builtin(sample_index) sample:u32)->@location(0) vec4<f32> {
    var surface=textureLoad(radiance,vec2<i32>(pixel.xy),i32(sample));
    if sky.mode.z>0.5 && sky.reserved2.z>0.5 {
        let size=vec2<f32>(textureDimensions(radiance));
        let xy=vec2(pixel.x/size.x*2.0-1.0,1.0-pixel.y/size.y*2.0);
        let matrix=transpose(frame.view_projection);
        var ray=cross(matrix[0].xyz-xy.x*matrix[3].xyz,matrix[1].xyz-xy.y*matrix[3].xyz);
        if dot(ray,matrix[2].xyz)<0.0 { ray=-ray; }
        ray=normalize(ray);
        let combined=surface.rgb*column_integral(ray,2)+scattered_radiance(ray);
        let fogged=mix(vec3(frame.ambient_sky.w,frame.ambient_ground.w,frame.grading.w),combined,atmospheric_fog(ray,scene_background_origin(xy)));
        surface=vec4(fogged*frame.grading.x,surface.a);
    }
    return surface;
}
