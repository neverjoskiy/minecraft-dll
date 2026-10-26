use std::collections::HashMap;

use egui_glow::{Painter, glow, glow::HasContext};

use crate::avatar::{model, runtime};

#[derive(Clone, Debug)]
pub struct AvatarPreviewRequest {
    pub avatar_id: String,
    pub size: [u32; 2],
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}

struct Batch {
    first: i32,
    count: i32,
    texture: Option<glow::Texture>,
}

pub struct AvatarPreviewRenderer {
    program: glow::Program,
    grid_program: glow::Program,
    vao: glow::VertexArray,
    grid_vao: glow::VertexArray,
    vbo: glow::Buffer,
    fbo: Option<glow::Framebuffer>,
    color: Option<glow::Texture>,
    depth: Option<glow::Renderbuffer>,
    texture_id: Option<egui::TextureId>,
    target_size: [i32; 2],
    avatar_id: String,
    generation: u64,
    batches: Vec<Batch>,
    textures: Vec<glow::Texture>,
    center: [f32; 3],
    radius: f32,
    bounds_min: [f32; 3],
    
    grid_extent: f32,
}

impl AvatarPreviewRenderer {
    pub unsafe fn new(gl: &glow::Context) -> Option<Self> {
        
        
        
        
        let program = report_err("avatar shader", compile_program(gl, VS, FS))?;
        let grid_program = report_err("grid shader", compile_program(gl, GRID_VS, GRID_FS))?;
        let vao = gl.create_vertex_array().ok()?;
        let grid_vao = gl.create_vertex_array().ok()?;
        let vbo = gl.create_buffer().ok()?;
        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let stride = (8 * std::mem::size_of::<f32>()) as i32;
        for (index, size, offset) in [(0, 3, 0), (1, 2, 3), (2, 3, 5)] {
            gl.enable_vertex_attrib_array(index);
            gl.vertex_attrib_pointer_f32(index, size, glow::FLOAT, false, stride, offset * 4);
        }
        gl.bind_vertex_array(None);
        Some(Self {
            program,
            grid_program,
            vao,
            grid_vao,
            vbo,
            fbo: None,
            color: None,
            depth: None,
            texture_id: None,
            target_size: [0, 0],
            avatar_id: String::new(),
            generation: 0,
            batches: Vec::new(),
            textures: Vec::new(),
            center: [0.0; 3],
            radius: 1.0,
            bounds_min: [0.0; 3],
            grid_extent: 2.0,
        })
    }

    pub fn texture_id(&self) -> Option<egui::TextureId> {
        self.texture_id
    }

    pub unsafe fn prepare(
        &mut self,
        painter: &mut Painter,
        request: &AvatarPreviewRequest,
        pixels_per_point: f32,
    ) {
        let gl = std::sync::Arc::clone(painter.gl());
        let width = ((request.size[0] as f32 * pixels_per_point).round() as i32).clamp(64, 1024);
        let height = ((request.size[1] as f32 * pixels_per_point).round() as i32).clamp(64, 1024);
        if !self.ensure_target(painter, &gl, width, height) {
            return;
        }
        let generation = runtime::avatar_content_generation();
        if self.avatar_id != request.avatar_id || self.generation != generation {
            if !self.load_avatar(&gl, &request.avatar_id) {
                return;
            }
            self.avatar_id = request.avatar_id.clone();
            self.generation = generation;
        }
        self.draw(&gl, request);
    }

    unsafe fn ensure_target(
        &mut self,
        painter: &mut Painter,
        gl: &glow::Context,
        w: i32,
        h: i32,
    ) -> bool {
        if self.target_size == [w, h] && self.fbo.is_some() {
            return true;
        }
        self.destroy_target(painter, gl);
        let Ok(color) = gl.create_texture() else {
            return false;
        };
        gl.bind_texture(glow::TEXTURE_2D, Some(color));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        let Ok(depth) = gl.create_renderbuffer() else {
            gl.delete_texture(color);
            return false;
        };
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w, h);
        let Ok(fbo) = gl.create_framebuffer() else {
            gl.delete_renderbuffer(depth);
            gl.delete_texture(color);
            return false;
        };
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(color),
            0,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth),
        );
        let complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        if !complete {
            gl.delete_framebuffer(fbo);
            gl.delete_renderbuffer(depth);
            gl.delete_texture(color);
            return false;
        }
        self.texture_id = Some(painter.register_native_texture(color));
        self.color = Some(color);
        self.depth = Some(depth);
        self.fbo = Some(fbo);
        self.target_size = [w, h];
        true
    }

    unsafe fn load_avatar(&mut self, gl: &glow::Context, avatar_id: &str) -> bool {
        for texture in self.textures.drain(..) {
            gl.delete_texture(texture);
        }
        self.batches.clear();
        let mut vertices = Vec::<f32>::new();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut texture_map = HashMap::<(usize, usize), glow::Texture>::new();
        let loaded = runtime::with_avatar(avatar_id, |avatar| {
            for (model_index, bbmodel) in avatar.package.models.iter().enumerate() {
                let tree = model::build_model_tree(bbmodel);
                for textured in model::flatten_textured_meshes(&tree) {
                    let first = (vertices.len() / 8) as i32;
                    for quad in textured.mesh.vertices.chunks(4) {
                        let indices: &[usize] = if quad.len() == 4 {
                            &[0, 1, 2, 0, 2, 3]
                        } else if quad.len() == 3 {
                            &[0, 1, 2]
                        } else {
                            continue;
                        };
                        for &index in indices {
                            let v = quad[index];
                            min[0] = min[0].min(v.x);
                            min[1] = min[1].min(v.y);
                            min[2] = min[2].min(v.z);
                            max[0] = max[0].max(v.x);
                            max[1] = max[1].max(v.y);
                            max[2] = max[2].max(v.z);
                            vertices.extend_from_slice(&[v.x, v.y, v.z, v.u, v.v, 1.0, 1.0, 1.0]);
                        }
                    }
                    let texture = textured.texture_index.and_then(|index| {
                        let key = (model_index, index as usize);
                        if let Some(texture) = texture_map.get(&key) {
                            return Some(*texture);
                        }
                        let bytes = bbmodel.textures.get(index as usize)?.png_bytes.as_ref()?;
                        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
                        let texture = upload_texture(
                            gl,
                            image.width() as i32,
                            image.height() as i32,
                            image.as_raw(),
                        )?;
                        texture_map.insert(key, texture);
                        Some(texture)
                    });
                    self.batches.push(Batch {
                        first,
                        count: (vertices.len() / 8) as i32 - first,
                        texture,
                    });
                }
            }
        })
        .is_some();
        self.textures.extend(texture_map.into_values());
        if !loaded || vertices.is_empty() {
            return false;
        }
        self.center = [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ];
        self.bounds_min = min;
        self.radius =
            ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2) + (max[2] - min[2]).powi(2))
                .sqrt()
                .max(0.5)
                * 0.5;
        self.grid_extent = grid_extent_for(min, max);
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytemuck(&vertices), glow::STATIC_DRAW);
        true
    }

    unsafe fn draw(&self, gl: &glow::Context, request: &AvatarPreviewRequest) {
        let Some(fbo) = self.fbo else { return };
        let draw_fbo = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
        let read_fbo = gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING);
        let program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
        let vao = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
        let texture = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
        let renderbuffer = gl.get_parameter_renderbuffer(glow::RENDERBUFFER_BINDING);
        let active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
        let mut viewport = [0_i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
        let depth = gl.is_enabled(glow::DEPTH_TEST);
        let blend = gl.is_enabled(glow::BLEND);
        let cull = gl.is_enabled(glow::CULL_FACE);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        
        let depth_mask = gl.get_parameter_bool(glow::DEPTH_WRITEMASK);
        let depth_func = gl.get_parameter_i32(glow::DEPTH_FUNC);
        let depth_clear = gl.get_parameter_f32(glow::DEPTH_CLEAR_VALUE);
        let color_mask = gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.viewport(0, 0, self.target_size[0], self.target_size[1]);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::CULL_FACE);
        gl.enable(glow::DEPTH_TEST);
        gl.depth_mask(true);
        gl.depth_func(glow::LESS);
        
        
        gl.clear_depth_f32(1.0);
        gl.color_mask(true, true, true, true);
        gl.enable(glow::BLEND);
        
        
        
        
        
        gl.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ONE,
            glow::ONE_MINUS_SRC_ALPHA,
        );
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        let aspect = aspect_for(self.target_size);
        let mvp = preview_matrix(
            aspect,
            self.center,
            self.radius,
            request.yaw,
            request.pitch,
            request.zoom,
        );
        
        
        gl.use_program(Some(self.program));
        gl.bind_vertex_array(Some(self.vao));
        if let Some(location) = gl.get_uniform_location(self.program, "u_mvp") {
            gl.uniform_matrix_4_f32_slice(Some(&location), false, &mvp);
        }
        if let Some(location) = gl.get_uniform_location(self.program, "u_tex") {
            gl.uniform_1_i32(Some(&location), 0);
        }
        for batch in &self.batches {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, batch.texture);
            if let Some(location) = gl.get_uniform_location(self.program, "u_textured") {
                gl.uniform_1_i32(Some(&location), batch.texture.is_some() as i32);
            }
            gl.draw_arrays(glow::TRIANGLES, batch.first, batch.count);
        }
        
        
        gl.depth_mask(false);
        gl.use_program(Some(self.grid_program));
        gl.bind_vertex_array(Some(self.grid_vao));
        if let Some(location) = gl.get_uniform_location(self.grid_program, "u_mvp") {
            gl.uniform_matrix_4_f32_slice(Some(&location), false, &mvp);
        }
        if let Some(location) = gl.get_uniform_location(self.grid_program, "u_center") {
            
            
            gl.uniform_3_f32(Some(&location), 0.0, self.bounds_min[1] - 0.001, 0.0);
        }
        if let Some(location) = gl.get_uniform_location(self.grid_program, "u_radius") {
            gl.uniform_1_f32(Some(&location), self.grid_extent);
        }
        gl.draw_arrays(glow::TRIANGLES, 0, 6);
        gl.depth_mask(depth_mask);
        gl.depth_func(depth_func as u32);
        gl.clear_depth_f32(depth_clear);
        gl.color_mask(color_mask[0], color_mask[1], color_mask[2], color_mask[3]);
        gl.bind_vertex_array(vao);
        gl.use_program(program);
        gl.active_texture(active_texture as u32);
        gl.bind_texture(glow::TEXTURE_2D, texture);
        gl.bind_renderbuffer(glow::RENDERBUFFER, renderbuffer);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw_fbo);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read_fbo);
        gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        set_enabled(gl, glow::DEPTH_TEST, depth);
        set_enabled(gl, glow::BLEND, blend);
        set_enabled(gl, glow::CULL_FACE, cull);
        set_enabled(gl, glow::SCISSOR_TEST, scissor);
    }

    pub unsafe fn destroy(&mut self, painter: &mut Painter) {
        let gl = std::sync::Arc::clone(painter.gl());
        self.destroy_target(painter, &gl);
        for texture in self.textures.drain(..) {
            gl.delete_texture(texture);
        }
        gl.delete_buffer(self.vbo);
        gl.delete_vertex_array(self.vao);
        gl.delete_vertex_array(self.grid_vao);
        gl.delete_program(self.program);
        gl.delete_program(self.grid_program);
    }

    unsafe fn destroy_target(&mut self, painter: &mut Painter, gl: &glow::Context) {
        if let Some(id) = self.texture_id.take() {
            painter.free_texture(id);
        }
        if let Some(fbo) = self.fbo.take() {
            gl.delete_framebuffer(fbo);
        }
        if let Some(depth) = self.depth.take() {
            gl.delete_renderbuffer(depth);
        }
        if let Some(color) = self.color.take() {
            gl.delete_texture(color);
        }
        self.target_size = [0, 0];
    }
}

unsafe fn upload_texture(
    gl: &glow::Context,
    width: i32,
    height: i32,
    rgba: &[u8],
) -> Option<glow::Texture> {
    let texture = gl.create_texture().ok()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::NEAREST as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_S,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_WRAP_T,
        glow::CLAMP_TO_EDGE as i32,
    );
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        width,
        height,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(rgba)),
    );
    Some(texture)
}

fn bytemuck(values: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), std::mem::size_of_val(values)) }
}

unsafe fn set_enabled(gl: &glow::Context, cap: u32, enabled: bool) {
    if enabled {
        gl.enable(cap)
    } else {
        gl.disable(cap)
    }
}

fn preview_matrix(
    aspect: f32,
    center: [f32; 3],
    radius: f32,
    yaw: f32,
    pitch: f32,
    zoom: f32,
) -> [f32; 16] {
    let distance = radius * (2.8 / zoom.clamp(0.45, 2.5));
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let eye = [
        center[0] + distance * sy * cp,
        center[1] + distance * sp,
        center[2] + distance * cy * cp,
    ];
    mul4(
        perspective(45_f32.to_radians(), aspect, 0.01, distance + radius * 6.0),
        look_at(eye, center, [0.0, 1.0, 0.0]),
    )
}







fn grid_extent_for(min: [f32; 3], max: [f32; 3]) -> f32 {
    let reach_x = min[0].abs().max(max[0].abs());
    let reach_z = min[2].abs().max(max[2].abs());
    (reach_x.hypot(reach_z) + 1.0).ceil().clamp(2.0, 8.0)
}
fn aspect_for(size: [i32; 2]) -> f32 {
    size[0] as f32 / size[1].max(1) as f32
}
fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov * 0.5).tan();
    [
        f / aspect,
        0.,
        0.,
        0.,
        0.,
        f,
        0.,
        0.,
        0.,
        0.,
        (far + near) / (near - far),
        -1.,
        0.,
        0.,
        (2. * far * near) / (near - far),
        0.,
    ]
}
fn look_at(e: [f32; 3], c: [f32; 3], u: [f32; 3]) -> [f32; 16] {
    let f = norm([c[0] - e[0], c[1] - e[1], c[2] - e[2]]);
    let s = norm(cross(f, u));
    let u = cross(s, f);
    [
        s[0],
        u[0],
        -f[0],
        0.,
        s[1],
        u[1],
        -f[1],
        0.,
        s[2],
        u[2],
        -f[2],
        0.,
        -dot(s, e),
        -dot(u, e),
        dot(f, e),
        1.,
    ]
}
fn mul4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut o = [0.; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}




fn report_err<T>(what: &str, result: Result<T, String>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!("[XZ] avatar preview: {what} failed: {error}");
            None
        }
    }
}

unsafe fn compile_program(gl: &glow::Context, vs: &str, fs: &str) -> Result<glow::Program, String> {
    let p = gl.create_program().map_err(|e| e.to_string())?;
    for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
        let s = gl.create_shader(kind).map_err(|e| e.to_string())?;
        gl.shader_source(s, src);
        gl.compile_shader(s);
        if !gl.get_shader_compile_status(s) {
            return Err(gl.get_shader_info_log(s));
        }
        gl.attach_shader(p, s);
        gl.delete_shader(s);
    }
    gl.link_program(p);
    if gl.get_program_link_status(p) {
        Ok(p)
    } else {
        Err(gl.get_program_info_log(p))
    }
}
const VS: &str = r#"#version 330 core
layout(location=0) in vec3 a_pos; layout(location=1) in vec2 a_uv; layout(location=2) in vec3 a_color;
uniform mat4 u_mvp; out vec2 v_uv; out vec3 v_color; void main(){gl_Position=u_mvp*vec4(a_pos,1.0);v_uv=a_uv;v_color=a_color;}"#;
const FS: &str = r#"#version 330 core
in vec2 v_uv; in vec3 v_color; uniform sampler2D u_tex; uniform int u_textured; out vec4 color;
void main(){vec4 t=u_textured!=0?texture(u_tex,v_uv):vec4(1.0);color=vec4(v_color,1.0)*t;if(color.a<0.02)discard;}"#;











const GRID_VS: &str = r#"#version 330 core
uniform mat4 u_mvp; uniform vec3 u_center; uniform float u_radius; out vec2 v_grid; out vec2 v_unit;
const vec2 P[6]=vec2[6](vec2(-1,-1),vec2(1,-1),vec2(1,1),vec2(-1,-1),vec2(1,1),vec2(-1,1));
void main(){vec2 p=P[gl_VertexID];vec2 xz=p*u_radius;v_unit=p;v_grid=xz+u_center.xz;
gl_Position=u_mvp*vec4(u_center+vec3(xz.x,0.0,xz.y),1.0);}"#;











const GRID_FS: &str = r#"#version 330 core
in vec2 v_grid; in vec2 v_unit; out vec4 color;
// Coverage of one line level, antialiased and faded out once its cells crowd into
// less than a pixel or so -- past that the lines merge into solid fill and would
// shimmer, so the finer level drops away and leaves the coarser one behind.
// fwidth is taken once in main and passed in: derivatives are undefined in
// non-uniform control flow, and this keeps both levels on the same footing.
float level(vec2 w,float cell){vec2 f=max(w/cell,vec2(1e-5));
vec2 g=abs(fract(v_grid/cell)-0.5)/f;
return (1.0-min(min(g.x,g.y),1.0))*(1.0-smoothstep(0.60,1.40,max(f.x,f.y)));}
void main(){vec2 w=fwidth(v_grid);
float line=min(level(w,0.25)*0.55+level(w,1.0),1.0);
// Reaches 0 exactly at the quad edge so the floor dissolves instead of ending on a
// straight cut. It starts late (0.55) because the grazing camera only ever shows the
// outer part of the quad: fading from 0.35 put the whole visible band in the falloff.
float radial=1.0-smoothstep(0.55,1.0,length(v_unit));
// Keeps a fully-covered line at ~2.4x the panel background, just under the old
// design's peak, since the two levels together now supply the density.
float alpha=line*radial*0.18;if(alpha<0.004)discard;color=vec4(0.62,0.57,0.72,alpha);}"#;

#[cfg(test)]
mod tests {
    use super::*;

    
    
    #[test]
    fn grid_extent_is_a_whole_number_of_blocks() {
        for (min, max) in [
            ([-0.25, 0.0, -0.15], [0.25, 1.85, 0.15]),
            ([-0.55, 0.0, -0.30], [1.30, 2.05, 0.30]),
            ([-2.0, 0.0, -0.2], [2.0, 2.0, 0.2]),
        ] {
            let extent = grid_extent_for(min, max);
            assert_eq!(extent, extent.round(), "extent {extent} is not whole");
        }
    }

    
    
    
    
    
    #[test]
    fn avatar_stands_well_inside_its_floor() {
        for (min, max) in [
            ([-0.25, 0.0, -0.15], [0.25, 1.85, 0.15]),
            ([-0.55, 0.0, -0.30], [1.30, 2.05, 0.30]),
            ([-2.0, 0.0, -0.2], [2.0, 2.0, 0.2]),
            ([-0.1, 0.0, -0.1], [0.1, 0.3, 0.1]),
        ] {
            let extent = grid_extent_for(min, max);
            let reach_x = min[0].abs().max(max[0].abs());
            let reach_z = min[2].abs().max(max[2].abs());
            
            assert!(
                extent >= reach_x.max(reach_z) + 0.5,
                "extent {extent} leaves no margin around a reach of                  ({reach_x}, {reach_z})",
            );
            
            assert!(
                extent <= reach_x.hypot(reach_z) + 2.0,
                "extent {extent} dwarfs a model reaching ({reach_x}, {reach_z})",
            );
        }
    }

    
    #[test]
    fn grid_extent_stays_within_sane_bounds() {
        let tiny = grid_extent_for([-0.01, 0.0, -0.01], [0.01, 0.05, 0.01]);
        assert_eq!(tiny, 2.0, "a tiny model should still get a 2-block floor");
        let huge = grid_extent_for([-40.0, 0.0, -40.0], [40.0, 60.0, 40.0]);
        assert_eq!(
            huge, 8.0,
            "an enormous model should clamp, not tile forever"
        );
    }

    
    
    
    #[test]
    fn grid_fade_length_is_computed_per_fragment() {
        assert!(
            GRID_VS.contains("v_unit=p"),
            "vertex shader should pass the raw quad-local offset through"
        );
        assert!(
            !GRID_VS.contains("length("),
            "vertex shader must not collapse the fade to a per-vertex length"
        );
        assert!(
            GRID_FS.contains("length(v_unit)"),
            "fragment shader should take the radial distance itself"
        );
    }

    
    
    
    #[test]
    fn grid_does_not_fade_by_screen_position() {
        for shader in [GRID_VS, GRID_FS] {
            assert!(
                !shader.contains("v_screen_y"),
                "grid opacity should derive from world space, not NDC y"
            );
        }
    }

    
    
    
    
    
    
    
    #[test]
    fn draw_pairs_its_depth_comparison_with_a_matching_clear_value() {
        let source = include_str!("avatar_preview.rs");
        let draw = source
            .split("unsafe fn draw(")
            .nth(1)
            .and_then(|rest| {
                rest.split(
                    "
    unsafe fn ",
                )
                .next()
            })
            .expect("draw should be findable in the source");
        let clears_depth = draw.contains("glow::DEPTH_BUFFER_BIT");
        assert!(clears_depth, "draw is expected to clear the depth buffer");
        
        
        assert!(
            draw.contains("gl.depth_func(glow::LESS)") && draw.contains("gl.clear_depth_f32(1.0)"),
            "a LESS depth test must clear depth to 1.0 explicitly, never inherit it"
        );
        
        let clear_call = draw
            .find("gl.clear(glow::COLOR_BUFFER_BIT")
            .expect("the combined clear should be findable");
        assert!(
            draw.find("gl.clear_depth_f32(1.0)").expect("set") < clear_call,
            "the clear value has to be set before glClear runs, not after"
        );
    }

    
    
    
    
    
    
    #[test]
    fn draw_claims_and_restores_the_global_write_masks() {
        let source = include_str!("avatar_preview.rs");
        let draw = source
            .split("unsafe fn draw(")
            .nth(1)
            .and_then(|rest| rest.split("\n    unsafe fn ").next())
            .expect("draw should be findable in the source");
        for (setter, getter) in [
            (
                "gl.color_mask(true, true, true, true)",
                "glow::COLOR_WRITEMASK",
            ),
            ("gl.depth_mask(true)", "glow::DEPTH_WRITEMASK"),
            ("gl.depth_func(glow::LESS)", "glow::DEPTH_FUNC"),
            ("gl.clear_depth_f32(1.0)", "glow::DEPTH_CLEAR_VALUE"),
        ] {
            assert!(
                draw.contains(setter),
                "draw should force known-good state with {setter}"
            );
            assert!(
                draw.contains(getter),
                "draw should save {getter} before overwriting it"
            );
        }
        
        for restore in [
            "gl.color_mask(color_mask[0]",
            "gl.depth_mask(depth_mask)",
            "gl.depth_func(depth_func as u32)",
            "gl.clear_depth_f32(depth_clear)",
        ] {
            assert!(
                draw.contains(restore),
                "draw should restore the saved state via {restore}"
            );
        }
    }

    
    
    
    
    #[test]
    fn grid_has_a_finer_level_than_whole_blocks() {
        let levels = GRID_FS.matches("level(w,").count();
        assert!(
            levels >= 2,
            "expected at least two line levels in the grid shader, found {levels}"
        );
        assert!(
            GRID_FS.contains("level(w,1.0)"),
            "one level should be whole blocks, so the floor carries Minecraft's scale"
        );
        assert!(
            GRID_FS.contains("level(w,0.25)"),
            "a sub-block level should keep the floor legible when the view flattens"
        );
    }

    
    
    
    
    #[test]
    fn grid_fade_covers_the_visible_band_and_still_reaches_the_edge() {
        let fade = GRID_FS
            .split("smoothstep(")
            .find(|piece| piece.contains("length(v_unit)"))
            .expect("radial fade should be a smoothstep of the quad-local length");
        let start: f32 = fade
            .split(',')
            .next()
            .and_then(|value| value.trim().parse().ok())
            .expect("fade should start at a literal radius");
        assert!(
            (0.5..0.9).contains(&start),
            "fade should start outside the visible band, got {start}"
        );
        let end: f32 = fade
            .split(',')
            .nth(1)
            .and_then(|value| value.trim().parse().ok())
            .expect("fade should end at a literal radius");
        assert_eq!(
            end, 1.0,
            "the fade must reach zero exactly at the quad edge"
        );
    }
}
