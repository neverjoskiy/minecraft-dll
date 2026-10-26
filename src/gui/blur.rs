use egui::Rect;
use egui_glow::glow::{self, HasContext};


#[derive(Clone, Copy, Debug)]
pub struct BlurRegion {
    pub rect: Rect,
    pub radius: f32,
}

impl BlurRegion {
    pub fn sharp(rect: Rect) -> Self {
        Self { rect, radius: 0.0 }
    }

    pub fn rounded(rect: Rect, radius: f32) -> Self {
        Self { rect, radius }
    }
}






pub struct MenuBlur {
    copy_program: glow::Program,
    blur_program: glow::Program,
    vao: glow::VertexArray,
    tex_a: Option<glow::Texture>,
    tex_b: Option<glow::Texture>,
    fbo_a: Option<glow::Framebuffer>,
    fbo_b: Option<glow::Framebuffer>,
    size: [i32; 2],
    u_copy_tex: Option<glow::UniformLocation>,
    u_copy_bounds: Option<glow::UniformLocation>,
    u_copy_radius: Option<glow::UniformLocation>,
    u_copy_fb: Option<glow::UniformLocation>,
    u_blur_tex: Option<glow::UniformLocation>,
    u_blur_dir: Option<glow::UniformLocation>,
    u_blur_texel: Option<glow::UniformLocation>,
}

impl MenuBlur {
    pub unsafe fn new(gl: &glow::Context) -> Option<Self> {
        let copy_program = compile_program(gl, COPY_VS, COPY_FS).ok()?;
        let blur_program = compile_program(gl, COPY_VS, BLUR_FS).ok()?;
        let vao = gl.create_vertex_array().ok()?;
        let u_copy_tex = gl.get_uniform_location(copy_program, "u_tex");
        let u_copy_bounds = gl.get_uniform_location(copy_program, "u_bounds");
        let u_copy_radius = gl.get_uniform_location(copy_program, "u_radius");
        let u_copy_fb = gl.get_uniform_location(copy_program, "u_fb_size");
        let u_blur_tex = gl.get_uniform_location(blur_program, "u_tex");
        let u_blur_dir = gl.get_uniform_location(blur_program, "u_dir");
        let u_blur_texel = gl.get_uniform_location(blur_program, "u_texel");

        Some(Self {
            copy_program,
            blur_program,
            vao,
            tex_a: None,
            tex_b: None,
            fbo_a: None,
            fbo_b: None,
            size: [0, 0],
            u_copy_tex,
            u_copy_bounds,
            u_copy_radius,
            u_copy_fb,
            u_blur_tex,
            u_blur_dir,
            u_blur_texel,
        })
    }

    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        self.destroy_targets(gl);
        gl.delete_vertex_array(self.vao);
        gl.delete_program(self.copy_program);
        gl.delete_program(self.blur_program);
    }

    unsafe fn ensure_targets(&mut self, gl: &glow::Context, w: i32, h: i32) -> bool {
        if w <= 0 || h <= 0 {
            return false;
        }
        if self.size == [w, h] && self.tex_a.is_some() && self.tex_b.is_some() {
            return true;
        }
        self.destroy_targets(gl);
        self.size = [w, h];

        let Some(tex_a) = create_texture(gl, w, h) else {
            return false;
        };
        let Some(tex_b) = create_texture(gl, w, h) else {
            gl.delete_texture(tex_a);
            return false;
        };
        let Some(fbo_a) = create_fbo(gl, tex_a) else {
            gl.delete_texture(tex_a);
            gl.delete_texture(tex_b);
            return false;
        };
        let Some(fbo_b) = create_fbo(gl, tex_b) else {
            gl.delete_framebuffer(fbo_a);
            gl.delete_texture(tex_a);
            gl.delete_texture(tex_b);
            return false;
        };

        self.tex_a = Some(tex_a);
        self.tex_b = Some(tex_b);
        self.fbo_a = Some(fbo_a);
        self.fbo_b = Some(fbo_b);
        true
    }

    unsafe fn destroy_targets(&mut self, gl: &glow::Context) {
        if let Some(tex) = self.tex_a.take() {
            gl.delete_texture(tex);
        }
        if let Some(tex) = self.tex_b.take() {
            gl.delete_texture(tex);
        }
        if let Some(fbo) = self.fbo_a.take() {
            gl.delete_framebuffer(fbo);
        }
        if let Some(fbo) = self.fbo_b.take() {
            gl.delete_framebuffer(fbo);
        }
        self.size = [0, 0];
    }

    
    pub unsafe fn draw_regions(
        &mut self,
        gl: &glow::Context,
        screen_size: [u32; 2],
        pixels_per_point: f32,
        regions: &[BlurRegion],
        source_fbo: Option<glow::Framebuffer>,
    ) {
        if regions.is_empty() {
            return;
        }

        let fb_w = screen_size[0] as i32;
        let fb_h = screen_size[1] as i32;
        let blur_w = (fb_w / 2).max(1);
        let blur_h = (fb_h / 2).max(1);
        if !self.ensure_targets(gl, blur_w, blur_h) {
            return;
        }
        let Some(tex_a) = self.tex_a else {
            return;
        };
        let Some(tex_b) = self.tex_b else {
            return;
        };
        let Some(fbo_a) = self.fbo_a else {
            return;
        };
        let Some(fbo_b) = self.fbo_b else {
            return;
        };

        let ppp = pixels_per_point.max(0.5);
        let mut gpu_regions = Vec::with_capacity(regions.len());
        for region in regions {
            let x0 = (region.rect.min.x * ppp).floor().max(0.0) as i32;
            let y0_ui = (region.rect.min.y * ppp).floor().max(0.0) as i32;
            let x1 = (region.rect.max.x * ppp).ceil().min(fb_w as f32) as i32;
            let y1_ui = (region.rect.max.y * ppp).ceil().min(fb_h as f32) as i32;
            if x1 <= x0 || y1_ui <= y0_ui {
                continue;
            }
            
            let y0 = fb_h - y1_ui;
            let y1 = fb_h - y0_ui;
            let radius = (region.radius * ppp).clamp(0.0, ((x1 - x0).min(y1 - y0) as f32) * 0.5);
            gpu_regions.push((x0 as f32, y0 as f32, x1 as f32, y1 as f32, radius));
        }
        if gpu_regions.is_empty() {
            return;
        }

        let dest_fbo = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
        let read_fbo = gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING);
        let mut prev_viewport = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut prev_viewport);
        let mut prev_scissor = [0i32; 4];
        gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut prev_scissor);
        let prev_scissor_on = gl.is_enabled(glow::SCISSOR_TEST);
        let prev_blend = gl.is_enabled(glow::BLEND);
        let prev_depth = gl.is_enabled(glow::DEPTH_TEST);
        let prev_cull = gl.is_enabled(glow::CULL_FACE);
        let prev_program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
        let prev_tex = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
        let prev_active = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
        let prev_vao = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);

        #[cfg(debug_assertions)]
        drain_errors(gl);

        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, source_fbo);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo_a));
        gl.blit_framebuffer(
            0,
            0,
            fb_w,
            fb_h,
            0,
            0,
            blur_w,
            blur_h,
            glow::COLOR_BUFFER_BIT,
            glow::NEAREST,
        );
        #[cfg(debug_assertions)]
        if drain_errors(gl) {
            restore_gl(
                gl,
                dest_fbo,
                read_fbo,
                prev_viewport,
                prev_scissor,
                prev_scissor_on,
                prev_blend,
                prev_depth,
                prev_cull,
                prev_program,
                prev_tex,
                prev_active,
                prev_vao,
            );
            return;
        }

        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.disable(glow::SCISSOR_TEST);
        gl.bind_vertex_array(Some(self.vao));
        gl.active_texture(glow::TEXTURE0);

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo_b));
        gl.viewport(0, 0, blur_w, blur_h);
        gl.use_program(Some(self.blur_program));
        if let Some(loc) = &self.u_blur_tex {
            gl.uniform_1_i32(Some(loc), 0);
        }
        if let Some(loc) = &self.u_blur_texel {
            gl.uniform_2_f32(Some(loc), 1.0 / blur_w as f32, 1.0 / blur_h as f32);
        }
        if let Some(loc) = &self.u_blur_dir {
            gl.uniform_2_f32(Some(loc), 1.0, 0.0);
        }
        gl.bind_texture(glow::TEXTURE_2D, Some(tex_a));
        gl.draw_arrays(glow::TRIANGLES, 0, 3);

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo_a));
        if let Some(loc) = &self.u_blur_dir {
            gl.uniform_2_f32(Some(loc), 0.0, 1.0);
        }
        gl.bind_texture(glow::TEXTURE_2D, Some(tex_b));
        gl.draw_arrays(glow::TRIANGLES, 0, 3);

        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, dest_fbo);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read_fbo);
        gl.viewport(0, 0, fb_w, fb_h);
        gl.use_program(Some(self.copy_program));
        if let Some(loc) = &self.u_copy_tex {
            gl.uniform_1_i32(Some(loc), 0);
        }
        if let Some(loc) = &self.u_copy_fb {
            gl.uniform_2_f32(Some(loc), fb_w as f32, fb_h as f32);
        }
        gl.bind_texture(glow::TEXTURE_2D, Some(tex_a));
        for (x0, y0, x1, y1, radius) in gpu_regions {
            if let Some(loc) = &self.u_copy_bounds {
                gl.uniform_4_f32(Some(loc), x0, y0, x1, y1);
            }
            if let Some(loc) = &self.u_copy_radius {
                gl.uniform_1_f32(Some(loc), radius);
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        #[cfg(debug_assertions)]
        let _ = drain_errors(gl);

        restore_gl(
            gl,
            dest_fbo,
            read_fbo,
            prev_viewport,
            prev_scissor,
            prev_scissor_on,
            prev_blend,
            prev_depth,
            prev_cull,
            prev_program,
            prev_tex,
            prev_active,
            prev_vao,
        );
    }
}

unsafe fn restore_gl(
    gl: &glow::Context,
    dest_fbo: Option<glow::Framebuffer>,
    read_fbo: Option<glow::Framebuffer>,
    prev_viewport: [i32; 4],
    prev_scissor: [i32; 4],
    prev_scissor_on: bool,
    prev_blend: bool,
    prev_depth: bool,
    prev_cull: bool,
    prev_program: Option<glow::Program>,
    prev_tex: Option<glow::Texture>,
    prev_active: i32,
    prev_vao: Option<glow::VertexArray>,
) {
    gl.disable(glow::SCISSOR_TEST);
    gl.bind_vertex_array(prev_vao);
    gl.use_program(prev_program);
    gl.active_texture(glow::TEXTURE0);
    gl.bind_texture(glow::TEXTURE_2D, prev_tex);
    gl.active_texture(prev_active as u32);
    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, dest_fbo);
    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read_fbo);
    gl.viewport(
        prev_viewport[0],
        prev_viewport[1],
        prev_viewport[2],
        prev_viewport[3],
    );
    gl.scissor(
        prev_scissor[0],
        prev_scissor[1],
        prev_scissor[2],
        prev_scissor[3],
    );
    set_enabled(gl, glow::SCISSOR_TEST, prev_scissor_on);
    set_enabled(gl, glow::BLEND, prev_blend);
    set_enabled(gl, glow::DEPTH_TEST, prev_depth);
    set_enabled(gl, glow::CULL_FACE, prev_cull);
}

unsafe fn set_enabled(gl: &glow::Context, cap: u32, on: bool) {
    if on {
        gl.enable(cap);
    } else {
        gl.disable(cap);
    }
}


unsafe fn drain_errors(gl: &glow::Context) -> bool {
    let mut had = false;
    loop {
        let err = gl.get_error();
        if err == glow::NO_ERROR {
            break;
        }
        had = true;
    }
    had
}

unsafe fn create_texture(gl: &glow::Context, w: i32, h: i32) -> Option<glow::Texture> {
    let tex = gl.create_texture().ok()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
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
        w,
        h,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
    );
    gl.bind_texture(glow::TEXTURE_2D, None);
    Some(tex)
}

unsafe fn create_fbo(gl: &glow::Context, tex: glow::Texture) -> Option<glow::Framebuffer> {
    let fbo = gl.create_framebuffer().ok()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(tex),
        0,
    );
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        return None;
    }
    Some(fbo)
}

unsafe fn compile_program(
    gl: &glow::Context,
    vs_src: &str,
    fs_src: &str,
) -> Result<glow::Program, String> {
    let program = gl.create_program().map_err(|e| e.to_string())?;
    let vs = gl
        .create_shader(glow::VERTEX_SHADER)
        .map_err(|e| e.to_string())?;
    gl.shader_source(vs, vs_src);
    gl.compile_shader(vs);
    if !gl.get_shader_compile_status(vs) {
        let log = gl.get_shader_info_log(vs);
        gl.delete_shader(vs);
        gl.delete_program(program);
        return Err(log);
    }
    let fs = gl
        .create_shader(glow::FRAGMENT_SHADER)
        .map_err(|e| e.to_string())?;
    gl.shader_source(fs, fs_src);
    gl.compile_shader(fs);
    if !gl.get_shader_compile_status(fs) {
        let log = gl.get_shader_info_log(fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        gl.delete_program(program);
        return Err(log);
    }
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);
    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(log);
    }
    Ok(program)
}

const COPY_VS: &str = r#"#version 330 core
out vec2 v_uv;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0)
    );
    vec2 uvs[3] = vec2[](
        vec2(0.0, 0.0),
        vec2(2.0, 0.0),
        vec2(0.0, 2.0)
    );
    gl_Position = vec4(positions[gl_VertexID], 0.0, 1.0);
    v_uv = uvs[gl_VertexID];
}
"#;

const COPY_FS: &str = r#"#version 330 core
in vec2 v_uv;
uniform sampler2D u_tex;
uniform vec4 u_bounds;   // x0, y0, x1, y1 in framebuffer pixels (GL Y-up)
uniform float u_radius;  // corner radius in pixels
uniform vec2 u_fb_size;
out vec4 frag_color;
void main() {
    vec2 p = v_uv * u_fb_size;
    vec2 half_size = 0.5 * (u_bounds.zw - u_bounds.xy);
    vec2 center = 0.5 * (u_bounds.xy + u_bounds.zw);
    float r = min(u_radius, min(half_size.x, half_size.y));
    vec2 q = abs(p - center) - half_size + vec2(r);
    float dist = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
    if (dist > 0.0) {
        discard;
    }
    frag_color = texture(u_tex, v_uv);
}
"#;

const BLUR_FS: &str = r#"#version 330 core
in vec2 v_uv;
uniform sampler2D u_tex;
uniform vec2 u_dir;
uniform vec2 u_texel;
out vec4 frag_color;
void main() {
    vec2 off = u_dir * u_texel * 1.75;
    vec4 c = texture(u_tex, v_uv) * 0.227027;
    c += texture(u_tex, v_uv + off) * 0.1945946;
    c += texture(u_tex, v_uv - off) * 0.1945946;
    c += texture(u_tex, v_uv + off * 2.0) * 0.1216216;
    c += texture(u_tex, v_uv - off * 2.0) * 0.1216216;
    c += texture(u_tex, v_uv + off * 3.0) * 0.054054;
    c += texture(u_tex, v_uv - off * 3.0) * 0.054054;
    frag_color = c;
}
"#;
