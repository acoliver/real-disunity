struct Frame {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    sun_direction: vec4<f32>,
    sun_radiance: vec4<f32>,
    ambient_sky: vec4<f32>,
    ambient_ground: vec4<f32>,
    grading: vec4<f32>,
    diffuse_sh: array<vec4<f32>,9>,
    lighting_options: vec4<f32>,
}
struct Sky {
    horizon: vec4<f32>,
    zenith: vec4<f32>,
    reserved3: vec4<f32>,
    reserved4: vec4<f32>,
    disc: vec4<f32>,
    disc_radiance: vec4<f32>,
    mode: vec4<f32>,
    reserved0: vec4<f32>,
    reserved1: vec4<f32>,
    reserved2: vec4<f32>,
}
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> sky: Sky;
@group(1) @binding(1) var disc_texture: texture_2d<f32>;
@group(1) @binding(2) var disc_sampler: sampler;
@group(1) @binding(3) var atmospheric_columns: texture_2d_array<f32>;
fn column_integral(ray:vec3<f32>,layer:i32)->vec3<f32> {
    let elevation=asin(clamp(ray.z,0.0,1.0));
    let position=(1.0-elevation/1.57079632679)*sky.reserved2.x-0.5;
    let row=i32(floor(position));
    let last=i32(sky.reserved2.x)-1;
    return mix(textureLoad(atmospheric_columns,vec2(0,clamp(row,0,last)),layer,0).rgb,
        textureLoad(atmospheric_columns,vec2(0,clamp(row+1,0,last)),layer,0).rgb,fract(position));
}
fn scattered_radiance(ray:vec3<f32>)->vec3<f32> {
    let mu=dot(ray,normalize(frame.sun_direction.xyz));
    let g=sky.reserved0.w;
    let g2=g*g;
    let molecular=3.0/(16.0*3.14159265359)*(1.0+mu*mu);
    let aerosol=3.0/(8.0*3.14159265359)*(1.0-g2)*(1.0+mu*mu)
        /((2.0+g2)*pow(1.0+g2-2.0*g*mu,1.5));
    let color=sky.reserved1.w*(column_integral(ray,0)*sky.reserved0.rgb*molecular
        +column_integral(ray,1)*sky.reserved1.rgb*aerosol);
    let luminance=dot(color,vec3(0.2126,0.7152,0.0722));
    return color*clamp(luminance,0.0,sky.reserved2.y)/max(0.0001,luminance);
}

fn atmospheric_fog(ray:vec3<f32>,origin:vec3<f32>)->f32 {
    if frame.sun_direction.w>=0.0 { return 1.0; }
    let falloff=-frame.sun_direction.w-1.0;
    let density=frame.camera_position.w-falloff*(origin.z-frame.camera_position.z);
    var distance=sky.mode.w;
    if sky.reserved3.x != 0.0 {
        distance=rd_far_plane_ray_distance(frame.view_projection,ray,frame.diffuse_sh[1].w);
    }
    if frame.diffuse_sh[0].w != 0.0 {
        return rd_full_ray_fog_transmittance(density,falloff,frame.sun_radiance.w,
            ray*distance,vec3(0.0,0.0,1.0));
    }
    return rd_height_fog_transmittance(density,falloff,frame.sun_radiance.w,
        ray*distance,vec3(0.0,0.0,1.0));
}

fn celestial_exposure()->f32 {
    if sky.reserved3.z != 0.0 { return sky.reserved3.w; }
    return frame.grading.x;
}

fn celestial_separate_surface(surface:vec3<f32>,background:vec3<f32>,transmission:f32)->vec3<f32> {
    let fogged=mix(vec3(frame.ambient_sky.w,frame.ambient_ground.w,frame.grading.w),background,transmission);
    return surface*celestial_exposure()+fogged*frame.grading.x;
}
fn background_radiance(ray:vec3<f32>)->vec3<f32> {
    var radiance=mix(sky.horizon.rgb,sky.zenith.rgb,pow(clamp(ray.z,0.0,1.0),sky.horizon.w));
    if sky.mode.y>0.5 {
        let uv=vec2(atan2(ray.y,ray.x)/6.28318530718+0.5,acos(clamp(ray.z,-1.0,1.0))/3.14159265359);
        let dx=dpdx(uv);
        let dy=dpdy(uv);
        let periodic_dx=vec2(dx.x-round(dx.x),dx.y);
        let periodic_dy=vec2(dy.x-round(dy.x),dy.y);
        radiance=textureSampleGrad(disc_texture,disc_sampler,uv,periodic_dx,periodic_dy).rgb;
    }
    if sky.mode.z>0.5 { radiance=scattered_radiance(ray); }
    if sky.mode.x<=0.5 && sky.zenith.w>0.0 && dot(ray,frame.sun_direction.xyz)>=cos(sky.zenith.w) {
        radiance=frame.sun_radiance.rgb;
        if sky.mode.z>0.5 && sky.reserved2.z>0.5 {
            radiance=radiance*column_integral(ray,2)+scattered_radiance(ray);
        }
    }
    return radiance;
}
