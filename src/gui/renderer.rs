use std::num::NonZeroU32;
use std::sync::Arc;

use egui::TextureId;
use egui_glow::{glow::HasContext, Painter, glow};

use crate::utils::name_tags::NameTagPreparedIcon;

use super::avatar_preview::{AvatarPreviewRenderer, AvatarPreviewRequest};

pub(crate) struct OverlayRenderer {
    pub(crate) painter: Painter,
    item_icons: Option<ItemIconRenderer>,
    menu_blur: Option<crate::gui_blur::MenuBlur>,
    avatar_preview: Option<AvatarPreviewRenderer>,
}

impl OverlayRenderer {
    pub(crate) fn new(gl: glow::Context) -> Option<Self> {
        let gl = Arc::new(gl);
        let painter = Painter::new(Arc::clone(&gl), "", None, false).ok()?;
        let menu_blur = unsafe { crate::gui_blur::MenuBlur::new(&gl) };
        Some(Self {
            painter,
            item_icons: None,
            menu_blur,
            avatar_preview: None,
        })
    }

    pub(crate) fn render(
        &mut self,
        screen_size: [u32; 2],
        pixels_per_point: f32,
        clipped_primitives: &[egui::ClippedPrimitive],
        textures_delta: &egui::TexturesDelta,
        blur_rects: &[crate::gui_blur::BlurRegion],
    ) {
        let gl = Arc::clone(self.painter.gl());
        let source_fbo = unsafe { gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING) };
        {
            let _gl_state = unsafe { EguiGlStateGuard::reset_for_egui(Arc::clone(&gl)) };

            if !blur_rects.is_empty() {
                if let Some(blur) = self.menu_blur.as_mut() {
                    unsafe {
                        blur.draw_regions(
                            &gl,
                            screen_size,
                            pixels_per_point,
                            blur_rects,
                            source_fbo,
                        );
                    }
                }
                unsafe {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    gl.disable(glow::SCISSOR_TEST);
                    gl.disable(glow::DEPTH_TEST);
                    gl.disable(glow::CULL_FACE);
                    gl.bind_vertex_array(None);
                    gl.use_program(None);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, None);
                }
            }

            unsafe {
                gl.disable(glow::DEPTH_TEST);
                gl.disable(glow::CULL_FACE);
                gl.disable(glow::STENCIL_TEST);
                gl.disable(glow::COLOR_LOGIC_OP);
                gl.disable(glow::SAMPLE_ALPHA_TO_COVERAGE);
                gl.disable(glow::SAMPLE_ALPHA_TO_ONE);
                gl.disable(GL_ALPHA_TEST_COMPAT);
                gl.disable(glow::SCISSOR_TEST);
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                drain_gl_errors(&gl);
            }
            self.painter.paint_and_update_textures(
                screen_size,
                pixels_per_point,
                clipped_primitives,
                textures_delta,
            );
        }

        unsafe {
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_vertex_array(None);
            drain_gl_errors(&gl);
        }
    }

    pub(crate) fn render_item_icons(&mut self, screen_size: [u32; 2], icons: &[NameTagPreparedIcon]) {
        if icons.is_empty() {
            return;
        }

        let gl = Arc::clone(self.painter.gl());
        if self.item_icons.is_none() {
            self.item_icons = unsafe { ItemIconRenderer::new(&gl) };
        }
        let Some(renderer) = self.item_icons.as_mut() else {
            return;
        };

        unsafe {
            let _gl_state = EguiGlStateGuard::reset_for_egui(Arc::clone(&gl));
            gl.viewport(0, 0, screen_size[0] as i32, screen_size[1] as i32);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::STENCIL_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.bind_sampler(0, None);
            renderer.render(&gl, screen_size, icons);
            drain_gl_errors(&gl);
        }
    }

    pub(crate) fn prepare_avatar_preview(&mut self, request: &AvatarPreviewRequest, pixels_per_point: f32) {
        let gl = Arc::clone(self.painter.gl());
        if self.avatar_preview.is_none() {
            self.avatar_preview = unsafe { AvatarPreviewRenderer::new(&gl) };
        }
        if let Some(preview) = self.avatar_preview.as_mut() {
            unsafe { preview.prepare(&mut self.painter, request, pixels_per_point) };
        }
    }

    pub(crate) fn avatar_preview_texture(&self) -> Option<TextureId> {
        self.avatar_preview
            .as_ref()
            .and_then(AvatarPreviewRenderer::texture_id)
    }
}

impl Drop for OverlayRenderer {
    fn drop(&mut self) {
        let gl = Arc::clone(self.painter.gl());
        if let Some(mut blur) = self.menu_blur.take() {
            unsafe {
                blur.destroy(&gl);
            }
        }
        if let Some(renderer) = self.item_icons.take() {
            unsafe {
                renderer.destroy(&gl);
            }
        }
        if let Some(mut preview) = self.avatar_preview.take() {
            unsafe {
                preview.destroy(&mut self.painter);
            }
        }
    }
}

struct ItemIconRenderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    u_screen: Option<glow::UniformLocation>,
    u_texture: Option<glow::UniformLocation>,
    u_use_texture: Option<glow::UniformLocation>,
}

impl ItemIconRenderer {
    unsafe fn new(gl: &glow::Context) -> Option<Self> {
        let program = unsafe { compile_icon_program(gl).ok()? };
        let vao = unsafe { gl.create_vertex_array().ok()? };
        let vbo = unsafe { gl.create_buffer().ok()? };
        let u_screen = unsafe { gl.get_uniform_location(program, "u_screen") };
        let u_texture = unsafe { gl.get_uniform_location(program, "u_texture") };
        let u_use_texture = unsafe { gl.get_uniform_location(program, "u_use_texture") };

        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 32, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, 32, 8);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, 32, 16);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);
        }

        Some(Self {
            program,
            vao,
            vbo,
            u_screen,
            u_texture,
            u_use_texture,
        })
    }

    unsafe fn render(
        &mut self,
        gl: &glow::Context,
        screen_size: [u32; 2],
        icons: &[NameTagPreparedIcon],
    ) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.uniform_2_f32(
                self.u_screen.as_ref(),
                screen_size[0].max(1) as f32,
                screen_size[1].max(1) as f32,
            );
            gl.uniform_1_i32(self.u_texture.as_ref(), 0);
            gl.active_texture(glow::TEXTURE0);
        }

        for icon in icons {
            let Some(texture) = NonZeroU32::new(icon.texture_id).map(glow::NativeTexture) else {
                continue;
            };
            let mut vertices = Vec::with_capacity(6 * 8);
            push_icon_quad(&mut vertices, icon);
            unsafe {
                gl.uniform_1_i32(self.u_use_texture.as_ref(), 1);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                if icon.nearest_filter {
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
                    gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                } else {
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
                    gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                }
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    f32_slice_as_u8(&vertices),
                    glow::STREAM_DRAW,
                );
                gl.draw_arrays(glow::TRIANGLES, 0, 6);
            }
        }

        let mut bar_vertices = Vec::new();
        for icon in icons {
            push_durability_bar_quads(&mut bar_vertices, icon);
        }
        if !bar_vertices.is_empty() {
            unsafe {
                gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                gl.uniform_1_i32(self.u_use_texture.as_ref(), 0);
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.buffer_data_u8_slice(
                    glow::ARRAY_BUFFER,
                    f32_slice_as_u8(&bar_vertices),
                    glow::STREAM_DRAW,
                );
                gl.draw_arrays(glow::TRIANGLES, 0, (bar_vertices.len() / 8) as i32);
            }
        }

        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }

    unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_buffer(self.vbo);
            gl.delete_vertex_array(self.vao);
            gl.delete_program(self.program);
        }
    }
}

unsafe fn compile_icon_program(gl: &glow::Context) -> Result<glow::Program, String> {
    let vertex = unsafe {
        compile_shader(
            gl,
            glow::VERTEX_SHADER,
            r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
layout (location = 2) in vec4 a_color;
uniform vec2 u_screen;
out vec2 v_uv;
out vec4 v_color;
void main() {
    vec2 ndc = vec2((a_pos.x / u_screen.x) * 2.0 - 1.0,
                    1.0 - (a_pos.y / u_screen.y) * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
    v_uv = a_uv;
    v_color = a_color;
}"#,
        )?
    };
    let fragment = unsafe {
        compile_shader(
            gl,
            glow::FRAGMENT_SHADER,
            r#"#version 330 core
uniform sampler2D u_texture;
uniform int u_use_texture;
in vec2 v_uv;
in vec4 v_color;
out vec4 out_color;
void main() {
    vec4 tex = u_use_texture == 1 ? texture(u_texture, v_uv) : vec4(1.0);
    out_color = tex * v_color;
}"#,
        )?
    };
    let program = unsafe { gl.create_program().map_err(|err| err.to_string())? };
    unsafe {
        gl.attach_shader(program, vertex);
        gl.attach_shader(program, fragment);
        gl.link_program(program);
        gl.detach_shader(program, vertex);
        gl.detach_shader(program, fragment);
        gl.delete_shader(vertex);
        gl.delete_shader(fragment);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(log);
        }
    }
    Ok(program)
}

unsafe fn compile_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::Shader, String> {
    let shader = unsafe {
        gl.create_shader(shader_type)
            .map_err(|err| err.to_string())?
    };
    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(log);
        }
    }
    Ok(shader)
}

fn push_icon_quad(vertices: &mut Vec<f32>, icon: &NameTagPreparedIcon) {
    let x0 = if icon.nearest_filter {
        icon.x.round()
    } else {
        icon.x
    };
    let y0 = if icon.nearest_filter {
        icon.y.round()
    } else {
        icon.y
    };
    let size = if icon.nearest_filter {
        icon.size.round()
    } else {
        icon.size
    };
    let x1 = x0 + size;
    let y1 = y0 + size;
    push_quad(
        vertices,
        x0,
        y0,
        x1,
        y1,
        icon.u0,
        icon.v0,
        icon.u1,
        icon.v1,
        [1.0, 1.0, 1.0, 1.0],
    );
}

fn push_durability_bar_quads(vertices: &mut Vec<f32>, icon: &NameTagPreparedIcon) {
    let Some(bar) = icon.durability_bar else {
        return;
    };
    let unit = (icon.size / 16.0).max(0.25);
    let x = icon.x + 2.0 * unit;
    let y = icon.y + 13.0 * unit;
    let width = 13.0 * unit;
    let fill_width = width * (bar.width.clamp(0, 13) as f32 / 13.0);
    let height = (2.0 * unit).max(1.0);
    push_colored_rect(vertices, x, y, x + width, y + height, [0.0, 0.0, 0.0, 0.95]);
    if fill_width > 0.0 {
        push_colored_rect(
            vertices,
            x,
            y,
            x + fill_width,
            y + height * 0.5,
            [
                bar.color[0] as f32 / 255.0,
                bar.color[1] as f32 / 255.0,
                bar.color[2] as f32 / 255.0,
                1.0,
            ],
        );
    }
}

fn push_colored_rect(vertices: &mut Vec<f32>, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4]) {
    push_quad(vertices, x0, y0, x1, y1, 0.0, 0.0, 1.0, 1.0, color);
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    vertices: &mut Vec<f32>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    color: [f32; 4],
) {
    push_vertex(vertices, x0, y0, u0, v0, color);
    push_vertex(vertices, x0, y1, u0, v1, color);
    push_vertex(vertices, x1, y1, u1, v1, color);
    push_vertex(vertices, x0, y0, u0, v0, color);
    push_vertex(vertices, x1, y1, u1, v1, color);
    push_vertex(vertices, x1, y0, u1, v0, color);
}

fn push_vertex(vertices: &mut Vec<f32>, x: f32, y: f32, u: f32, v: f32, color: [f32; 4]) {
    vertices.extend_from_slice(&[x, y, u, v, color[0], color[1], color[2], color[3]]);
}

fn f32_slice_as_u8(values: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr() as *const u8, std::mem::size_of_val(values))
    }
}

const GL_ALPHA_TEST_COMPAT: u32 = 0x0BC0;

pub(crate) struct EguiGlStateGuard {
    gl: Arc<glow::Context>,
    active_texture: i32,
    unpack_alignment: i32,
    unpack_row_length: i32,
    unpack_skip_rows: i32,
    unpack_skip_pixels: i32,
    unpack_lsb_first: i32,
    unpack_swap_bytes: i32,
    viewport: [i32; 4],
    scissor_box: [i32; 4],
    color_mask: [bool; 4],
    blend_enabled: bool,
    scissor_enabled: bool,
    depth_test_enabled: bool,
    cull_face_enabled: bool,
    stencil_test_enabled: bool,
    color_logic_op_enabled: bool,
    sample_alpha_to_coverage_enabled: bool,
    sample_alpha_to_one_enabled: bool,
    framebuffer_srgb_enabled: bool,
    blend_equation_rgb: i32,
    blend_equation_alpha: i32,
    blend_src_rgb: i32,
    blend_dst_rgb: i32,
    blend_src_alpha: i32,
    blend_dst_alpha: i32,
    texture_binding_2d: Option<glow::Texture>,
    current_program: Option<glow::Program>,
    pixel_unpack_buffer: Option<glow::Buffer>,
    pixel_pack_buffer: Option<glow::Buffer>,
    sampler: Option<glow::Sampler>,
    draw_framebuffer: Option<glow::Framebuffer>,
    read_framebuffer: Option<glow::Framebuffer>,
}

impl EguiGlStateGuard {
    pub(crate) unsafe fn reset_for_egui(gl: Arc<glow::Context>) -> Self {
        let active_texture = unsafe {
            let active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
            gl.active_texture(glow::TEXTURE0);
            active_texture
        };
        let mut viewport = [0; 4];
        let mut scissor_box = [0; 4];
        unsafe {
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);
        }
        let state = unsafe {
            Self {
                active_texture,
                texture_binding_2d: gl.get_parameter_texture(glow::TEXTURE_BINDING_2D),
                current_program: gl.get_parameter_program(glow::CURRENT_PROGRAM),
                unpack_alignment: gl.get_parameter_i32(glow::UNPACK_ALIGNMENT),
                unpack_row_length: gl.get_parameter_i32(glow::UNPACK_ROW_LENGTH),
                unpack_skip_rows: gl.get_parameter_i32(glow::UNPACK_SKIP_ROWS),
                unpack_skip_pixels: gl.get_parameter_i32(glow::UNPACK_SKIP_PIXELS),
                unpack_lsb_first: gl.get_parameter_i32(glow::UNPACK_LSB_FIRST),
                unpack_swap_bytes: gl.get_parameter_i32(glow::UNPACK_SWAP_BYTES),
                viewport,
                scissor_box,
                color_mask: gl.get_parameter_bool_array(glow::COLOR_WRITEMASK),
                blend_enabled: gl.is_enabled(glow::BLEND),
                scissor_enabled: gl.is_enabled(glow::SCISSOR_TEST),
                depth_test_enabled: gl.is_enabled(glow::DEPTH_TEST),
                cull_face_enabled: gl.is_enabled(glow::CULL_FACE),
                stencil_test_enabled: gl.is_enabled(glow::STENCIL_TEST),
                color_logic_op_enabled: gl.is_enabled(glow::COLOR_LOGIC_OP),
                sample_alpha_to_coverage_enabled: gl.is_enabled(glow::SAMPLE_ALPHA_TO_COVERAGE),
                sample_alpha_to_one_enabled: gl.is_enabled(glow::SAMPLE_ALPHA_TO_ONE),
                framebuffer_srgb_enabled: gl.is_enabled(glow::FRAMEBUFFER_SRGB),
                blend_equation_rgb: gl.get_parameter_i32(glow::BLEND_EQUATION_RGB),
                blend_equation_alpha: gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA),
                blend_src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
                blend_dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
                blend_src_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
                blend_dst_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
                pixel_unpack_buffer: gl.get_parameter_buffer(glow::PIXEL_UNPACK_BUFFER_BINDING),
                pixel_pack_buffer: gl.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING),
                sampler: gl.get_parameter_sampler(glow::SAMPLER_BINDING),
                draw_framebuffer: gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                read_framebuffer: gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                gl,
            }
        };

        unsafe {
            state.gl.active_texture(glow::TEXTURE0);
            state.gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            state.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            state.gl.bind_sampler(0, None);
            state.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            state.gl.disable(glow::SCISSOR_TEST);
            state.gl.disable(glow::FRAMEBUFFER_SRGB);
            state.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            state.gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            state.gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
            state.gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
            state.gl.pixel_store_i32(glow::UNPACK_LSB_FIRST, 0);
            state.gl.pixel_store_i32(glow::UNPACK_SWAP_BYTES, 0);
        }

        state
    }
}

impl Drop for EguiGlStateGuard {
    fn drop(&mut self) {
        unsafe {
            self.gl
                .pixel_store_i32(glow::UNPACK_ALIGNMENT, self.unpack_alignment);
            self.gl
                .pixel_store_i32(glow::UNPACK_ROW_LENGTH, self.unpack_row_length);
            self.gl
                .pixel_store_i32(glow::UNPACK_SKIP_ROWS, self.unpack_skip_rows);
            self.gl
                .pixel_store_i32(glow::UNPACK_SKIP_PIXELS, self.unpack_skip_pixels);
            self.gl
                .pixel_store_i32(glow::UNPACK_LSB_FIRST, self.unpack_lsb_first);
            self.gl
                .pixel_store_i32(glow::UNPACK_SWAP_BYTES, self.unpack_swap_bytes);
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, self.texture_binding_2d);
            self.gl.use_program(self.current_program);
            self.gl
                .bind_buffer(glow::PIXEL_UNPACK_BUFFER, self.pixel_unpack_buffer);
            self.gl
                .bind_buffer(glow::PIXEL_PACK_BUFFER, self.pixel_pack_buffer);
            self.gl.bind_sampler(0, self.sampler);
            self.gl
                .bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.draw_framebuffer);
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, self.read_framebuffer);
            self.gl.viewport(
                self.viewport[0],
                self.viewport[1],
                self.viewport[2],
                self.viewport[3],
            );
            self.gl.scissor(
                self.scissor_box[0],
                self.scissor_box[1],
                self.scissor_box[2],
                self.scissor_box[3],
            );
            self.gl.color_mask(
                self.color_mask[0],
                self.color_mask[1],
                self.color_mask[2],
                self.color_mask[3],
            );
            self.gl.blend_equation_separate(
                self.blend_equation_rgb as u32,
                self.blend_equation_alpha as u32,
            );
            self.gl.blend_func_separate(
                self.blend_src_rgb as u32,
                self.blend_dst_rgb as u32,
                self.blend_src_alpha as u32,
                self.blend_dst_alpha as u32,
            );
            set_gl_enabled(&self.gl, glow::BLEND, self.blend_enabled);
            set_gl_enabled(&self.gl, glow::SCISSOR_TEST, self.scissor_enabled);
            set_gl_enabled(&self.gl, glow::DEPTH_TEST, self.depth_test_enabled);
            set_gl_enabled(&self.gl, glow::CULL_FACE, self.cull_face_enabled);
            set_gl_enabled(&self.gl, glow::STENCIL_TEST, self.stencil_test_enabled);
            set_gl_enabled(&self.gl, glow::COLOR_LOGIC_OP, self.color_logic_op_enabled);
            set_gl_enabled(
                &self.gl,
                glow::SAMPLE_ALPHA_TO_COVERAGE,
                self.sample_alpha_to_coverage_enabled,
            );
            set_gl_enabled(
                &self.gl,
                glow::SAMPLE_ALPHA_TO_ONE,
                self.sample_alpha_to_one_enabled,
            );
            set_gl_enabled(
                &self.gl,
                glow::FRAMEBUFFER_SRGB,
                self.framebuffer_srgb_enabled,
            );
            self.gl.active_texture(self.active_texture as u32);
        }
    }
}

unsafe fn set_gl_enabled(gl: &glow::Context, cap: u32, enabled: bool) {
    unsafe {
        if enabled {
            gl.enable(cap);
        } else {
            gl.disable(cap);
        }
    }
}

pub(crate) fn drain_gl_errors(gl: &glow::Context) {
    unsafe { while gl.get_error() != glow::NO_ERROR {} }
}
