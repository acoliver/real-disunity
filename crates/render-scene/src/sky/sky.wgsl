struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) direction: vec3<f32>,
    @location(1) origin: vec3<f32>,
}
@vertex fn vertex(@builtin(vertex_index) index:u32)->Vertex {
    let xy=vec2(f32((index<<1u)&2u),f32(index&2u))*2.0-1.0;
    let matrix=transpose(frame.view_projection);
    // Intersect the two clip-coordinate planes in world-direction space.
    // Their cross product is affine in xy and interpolates perspective rays exactly.
    var ray=cross(matrix[0].xyz-xy.x*matrix[3].xyz,matrix[1].xyz-xy.y*matrix[3].xyz);
    if dot(ray,matrix[2].xyz)<0.0 { ray=-ray; }
    var out:Vertex;
    out.position=vec4(xy,1.0,1.0);
    out.direction=ray;
    out.origin=scene_background_origin(xy);
    return out;
}
@fragment fn fragment(input:Vertex)->@location(0) vec4<f32> {
    let ray=normalize(input.direction);
    var radiance=mix(sky.horizon.rgb,sky.zenith.rgb,pow(clamp(ray.z,0.0,1.0),sky.horizon.w));
    var coverage=1.0;
    if sky.mode.y>0.5 {
        // Equirectangular Z-up dome: +X is u=0.5, +Y is u=0.75, zenith is v=0.
        let uv=vec2(atan2(ray.y,ray.x)/6.28318530718+0.5,acos(clamp(ray.z,-1.0,1.0))/3.14159265359);
        let dx=dpdx(uv);
        let dy=dpdy(uv);
        let periodic_dx=vec2(dx.x-round(dx.x),dx.y);
        let periodic_dy=vec2(dy.x-round(dy.x),dy.y);
        radiance=textureSampleGrad(disc_texture,disc_sampler,uv,periodic_dx,periodic_dy).rgb;
    }
    if sky.mode.z>0.5 { radiance=scattered_radiance(ray); }
    if sky.mode.x>0.5 {
        let cosine=dot(ray,sky.disc.xyz);
        if cosine<cos(sky.disc.w) { discard; }
        var seed=vec3(0.0,0.0,1.0);
        if abs(sky.disc.z)>=0.999 { seed=vec3(1.0,0.0,0.0); }
        let right=normalize(cross(seed,sky.disc.xyz));
        let up=cross(sky.disc.xyz,right);
        let p=vec2(dot(ray,right),dot(ray,up))/(cosine*tan(sky.disc.w));
        let c=cos(sky.disc_radiance.w);
        let s=sin(sky.disc_radiance.w);
        let uv=vec2(c*p.x+s*p.y,c*p.y-s*p.x)*0.5+0.5;
        let texel=textureSample(disc_texture,disc_sampler,uv);
        radiance=texel.rgb*sky.disc_radiance.rgb;
        if sky.mode.z>0.5 && sky.reserved2.z>0.5 {
            radiance=radiance*column_integral(ray,2)+scattered_radiance(ray);
        }
        coverage=texel.a;
    } else if sky.zenith.w>0.0 && dot(ray,frame.sun_direction.xyz)>=cos(sky.zenith.w) {
        radiance=frame.sun_radiance.rgb;
        if sky.mode.z>0.5 && sky.reserved2.z>0.5 {
            radiance=radiance*column_integral(ray,2)+scattered_radiance(ray);
        }
    }
    radiance=mix(vec3(frame.ambient_sky.w,frame.ambient_ground.w,frame.grading.w),radiance,atmospheric_fog(ray,input.origin));
    return vec4(radiance * frame.grading.x,coverage);
}
