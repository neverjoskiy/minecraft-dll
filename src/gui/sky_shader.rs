use std::{
    ffi::{CString, c_void},
    ptr::null_mut,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use egui_glow::glow::{self, HasContext};
use windows::{
    Win32::{
        Graphics::OpenGL::wglGetProcAddress,
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
    },
    core::{PCSTR, s},
};

use crate::modules::{SkyShaderConfig, sky_shader_snapshot};

const GL_TRIANGLE_FAN: u32 = 6;
const GL_DEPTH_TEST: u32 = 0x0B71;
const GL_TRIANGLES: u32 = 4;

static GPU: OnceLock<Mutex<Option<SkyShaderPass>>> = OnceLock::new();
static IN_HIJACK: AtomicBool = AtomicBool::new(false);

static SUPPRESS_SKY_EXTRAS: AtomicBool = AtomicBool::new(false);

type GlIsEnabled = unsafe extern "system" fn(u32) -> u8;
type GlDrawArrays = unsafe extern "system" fn(u32, i32, i32);

fn gpu() -> &'static Mutex<Option<SkyShaderPass>> {
    GPU.get_or_init(|| Mutex::new(None))
}

pub unsafe fn ensure_initialized(gl: &glow::Context) {
    let mut slot = gpu().lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = SkyShaderPass::new(gl);
    }
}

pub unsafe fn destroy(gl: &glow::Context) {
    if let Ok(mut slot) = gpu().lock() {
        if let Some(mut pass) = slot.take() {
            pass.destroy(gl);
        }
    }
}


pub fn intercept_sky_draw_arrays(mode: u32, count: i32, instance_count: i32) -> bool {
    if IN_HIJACK.load(Ordering::Acquire) {
        return false;
    }

    let suppressing = SUPPRESS_SKY_EXTRAS.load(Ordering::Acquire);
    if suppressing {
        if depth_test_enabled() {
            SUPPRESS_SKY_EXTRAS.store(false, Ordering::Release);
            return false;
        }
        return true;
    }

    let is_sky_disc = mode == GL_TRIANGLE_FAN && count == 10 && instance_count >= 1;
    if !is_sky_disc || depth_test_enabled() {
        return false;
    }

    let Some(config) = sky_shader_snapshot() else {
        return false;
    };
    IN_HIJACK.store(true, Ordering::Release);
    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        draw_custom_sky(config)
    }))
    .unwrap_or(false);
    IN_HIJACK.store(false, Ordering::Release);
    if ok {
        SUPPRESS_SKY_EXTRAS.store(true, Ordering::Release);
    }
    ok
}

pub fn intercept_sky_draw_elements() -> bool {
    if IN_HIJACK.load(Ordering::Acquire) {
        return false;
    }
    end_suppress_if_depth_on();
    SUPPRESS_SKY_EXTRAS.load(Ordering::Acquire) && !depth_test_enabled()
}

pub fn clear_sky_suppress() {
    SUPPRESS_SKY_EXTRAS.store(false, Ordering::Release);
}

fn end_suppress_if_depth_on() {
    if SUPPRESS_SKY_EXTRAS.load(Ordering::Acquire) && depth_test_enabled() {
        SUPPRESS_SKY_EXTRAS.store(false, Ordering::Release);
    }
}


pub fn try_hijack_sky_disc(mode: u32, count: i32, instance_count: i32) -> bool {
    intercept_sky_draw_arrays(mode, count, instance_count)
}

fn depth_test_enabled() -> bool {
    static GL_IS_ENABLED: OnceLock<usize> = OnceLock::new();
    let ptr = *GL_IS_ENABLED.get_or_init(|| unsafe { load_gl_proc("glIsEnabled") as usize })
        as *mut c_void;
    if ptr.is_null() {
        return true;
    }
    let is_enabled: GlIsEnabled = unsafe { std::mem::transmute(ptr) };
    unsafe { is_enabled(GL_DEPTH_TEST) != 0 }
}

static GL_CONTEXT: OnceLock<Mutex<Option<glow::Context>>> = OnceLock::new();

unsafe fn draw_custom_sky(config: SkyShaderConfig) -> bool {
    let slot_lock = GL_CONTEXT.get_or_init(|| Mutex::new(None));
    {
        let mut guard = slot_lock.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(glow::Context::from_loader_function(|name| {
                load_gl_proc(name) as *const c_void
            }));
        }
    }
    let guard = slot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let gl = guard.as_ref().unwrap();
    ensure_initialized(gl);
    let mut gpu_slot = match gpu().lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let Some(pass) = gpu_slot.as_mut() else {
        return false;
    };
    pass.draw_sky_fullscreen(gl, config);
    true
}

pub struct SkyShaderPass {
    program: glow::Program,
    vao: glow::VertexArray,
    u_resolution: Option<glow::UniformLocation>,
    u_time: Option<glow::UniformLocation>,
    u_mode: Option<glow::UniformLocation>,
    u_intensity: Option<glow::UniformLocation>,
    u_scale: Option<glow::UniformLocation>,
    u_ray_forward: Option<glow::UniformLocation>,
    u_ray_right: Option<glow::UniformLocation>,
    u_ray_up: Option<glow::UniformLocation>,
    u_shader_color: Option<glow::UniformLocation>,
    u_bg_color: Option<glow::UniformLocation>,
    started_at: Instant,
    draw_arrays: Option<GlDrawArrays>,
}

fn sky_ray_basis(
    width: f32,
    height: f32,
    config: SkyShaderConfig,
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let aspect = width / height.max(1.0);
    let tan_half = (config.camera.fov_y_degrees.clamp(1.0, 170.0).to_radians() * 0.5).tan();
    let yaw = config.camera.yaw_degrees.to_radians();
    let pitch = config.camera.pitch_degrees.to_radians();
    let cp = pitch.cos();

    let forward = [-yaw.sin() * cp, -pitch.sin(), yaw.cos() * cp];
    let right = [-yaw.cos(), 0.0, -yaw.sin()];
    let mut up = cross3(right, forward);
    let up_len = dot3(up, up).sqrt();
    if up_len > 1e-4 {
        up = [up[0] / up_len, up[1] / up_len, up[2] / up_len];
    } else {
        up = [0.0, 1.0, 0.0];
    }

    (
        forward,
        [
            right[0] * aspect * tan_half,
            right[1] * aspect * tan_half,
            right[2] * aspect * tan_half,
        ],
        [up[0] * tan_half, up[1] * tan_half, up[2] * tan_half],
    )
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
impl SkyShaderPass {
    pub unsafe fn new(gl: &glow::Context) -> Option<Self> {
        let program = compile_program(gl, VS, FS).ok()?;
        let vao = gl.create_vertex_array().ok()?;
        Some(Self {
            u_resolution: gl.get_uniform_location(program, "iResolution"),
            u_time: gl.get_uniform_location(program, "iTime"),
            u_mode: gl.get_uniform_location(program, "u_mode"),
            u_intensity: gl.get_uniform_location(program, "u_intensity"),
            u_scale: gl.get_uniform_location(program, "u_scale"),
            u_ray_forward: gl.get_uniform_location(program, "u_ray_forward"),
            u_ray_right: gl.get_uniform_location(program, "u_ray_right"),
            u_ray_up: gl.get_uniform_location(program, "u_ray_up"),
            u_shader_color: gl.get_uniform_location(program, "u_shader_color"),
            u_bg_color: gl.get_uniform_location(program, "u_bg_color"),
            program,
            vao,
            started_at: Instant::now(),
            draw_arrays: {
                let ptr = load_gl_proc("glDrawArrays");
                if ptr.is_null() {
                    None
                } else {
                    Some(std::mem::transmute(ptr))
                }
            },
        })
    }

    pub unsafe fn destroy(&mut self, gl: &glow::Context) {
        gl.delete_vertex_array(self.vao);
        gl.delete_program(self.program);
    }

    pub unsafe fn draw_sky_fullscreen(&mut self, gl: &glow::Context, config: SkyShaderConfig) {
        let mut viewport = [0i32; 4];
        gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
        let width = viewport[2].max(1) as f32;
        let height = viewport[3].max(1) as f32;
        let time = self.started_at.elapsed().as_secs_f32() * config.speed.max(0.01);

        let prev_program = gl.get_parameter_program(glow::CURRENT_PROGRAM);
        let prev_vao = gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING);
        let blend = gl.is_enabled(glow::BLEND);
        let depth = gl.is_enabled(glow::DEPTH_TEST);
        let cull = gl.is_enabled(glow::CULL_FACE);
        let scissor = gl.is_enabled(glow::SCISSOR_TEST);

        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.disable(glow::SCISSOR_TEST);
        gl.bind_vertex_array(Some(self.vao));
        gl.use_program(Some(self.program));

        if let Some(loc) = &self.u_resolution {
            gl.uniform_2_f32(Some(loc), width, height);
        }
        if let Some(loc) = &self.u_time {
            gl.uniform_1_f32(Some(loc), time);
        }
        if let Some(loc) = &self.u_mode {
            gl.uniform_1_i32(Some(loc), config.mode.as_i32());
        }
        if let Some(loc) = &self.u_intensity {
            gl.uniform_1_f32(Some(loc), config.intensity.clamp(0.05, 5.0));
        }
        if let Some(loc) = &self.u_scale {
            gl.uniform_1_f32(Some(loc), config.scale.clamp(0.1, 5.0));
        }
        let (ray_forward, ray_right, ray_up) = sky_ray_basis(width, height, config);
        if let Some(loc) = &self.u_ray_forward {
            gl.uniform_3_f32(Some(loc), ray_forward[0], ray_forward[1], ray_forward[2]);
        }
        if let Some(loc) = &self.u_ray_right {
            gl.uniform_3_f32(Some(loc), ray_right[0], ray_right[1], ray_right[2]);
        }
        if let Some(loc) = &self.u_ray_up {
            gl.uniform_3_f32(Some(loc), ray_up[0], ray_up[1], ray_up[2]);
        }
        if let Some(loc) = &self.u_shader_color {
            let c = config.shader_color;
            gl.uniform_3_f32(Some(loc), c[0], c[1], c[2]);
        }
        if let Some(loc) = &self.u_bg_color {
            let c = config.background_color;
            gl.uniform_3_f32(Some(loc), c[0], c[1], c[2]);
        }
        if let Some(draw) = self.draw_arrays {
            draw(GL_TRIANGLES, 0, 3);
        }

        gl.bind_vertex_array(prev_vao);
        gl.use_program(prev_program);
        set_enabled(gl, glow::BLEND, blend);
        set_enabled(gl, glow::DEPTH_TEST, depth);
        set_enabled(gl, glow::CULL_FACE, cull);
        set_enabled(gl, glow::SCISSOR_TEST, scissor);
        #[cfg(debug_assertions)]
        let _ = drain_errors(gl);
    }
}

unsafe fn set_enabled(gl: &glow::Context, cap: u32, on: bool) {
    if on {
        gl.enable(cap);
    } else {
        gl.disable(cap);
    }
}

fn drain_errors(gl: &glow::Context) -> bool {
    let mut had = false;
    unsafe {
        while gl.get_error() != glow::NO_ERROR {
            had = true;
        }
    }
    had
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

pub unsafe fn load_gl_proc(name: &str) -> *mut c_void {
    let Ok(cname) = CString::new(name) else {
        return null_mut();
    };
    let address = unsafe { wglGetProcAddress(PCSTR(cname.as_ptr().cast())) };
    if let Some(proc) = address {
        let ptr = proc as *mut c_void;
        if !matches!(ptr as usize, 0 | 1 | 2 | 3 | usize::MAX) {
            return ptr;
        }
    }
    let opengl32 = match unsafe { GetModuleHandleA(s!("opengl32.dll")) } {
        Ok(module) => module,
        Err(_) => return null_mut(),
    };
    unsafe { GetProcAddress(opengl32, PCSTR(cname.as_ptr().cast())) }
        .map(|proc| proc as *mut c_void)
        .unwrap_or(null_mut())
}

const VS: &str = r#"#version 330 core
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0)
    );
    gl_Position = vec4(positions[gl_VertexID], 0.0, 1.0);
}
"#;

const FS: &str = r#"#version 330 core
uniform vec2 iResolution;
uniform float iTime;
uniform int u_mode;
uniform float u_intensity;
uniform float u_scale;
uniform vec3 u_ray_forward;
uniform vec3 u_ray_right;
uniform vec3 u_ray_up;
uniform vec3 u_shader_color;
uniform vec3 u_bg_color;
out vec4 fragColor;

const mat2 WARP_M = mat2(0.80, 0.60, -0.60, 0.80);

float warp_noise(in vec2 p) {
    return sin(p.x) * sin(p.y);
}

float warp_fbm4(vec2 p) {
    float f = 0.0;
    f += 0.5000 * warp_noise(p); p = WARP_M * p * 2.02;
    f += 0.2500 * warp_noise(p); p = WARP_M * p * 2.02;
    f += 0.1250 * warp_noise(p); p = WARP_M * p * 2.02;
    f += 0.0625 * warp_noise(p);
    return f / 0.9375;
}

float warp_fbm6(vec2 p) {
    float f = 0.0;
    f += 0.500000 * (0.5 + 0.5 * warp_noise(p)); p = WARP_M * p * 2.02;
    f += 0.500000 * (0.5 + 0.5 * warp_noise(p)); p = WARP_M * p * 2.02;
    f += 0.500000 * (0.5 + 0.5 * warp_noise(p)); p = WARP_M * p * 2.02;
    f += 0.250000 * (0.5 + 0.5 * warp_noise(p)); p = WARP_M * p * 2.02;
    return f / 0.96875;
}

vec2 warp_fbm4_2(vec2 p) {
    return vec2(warp_fbm4(p), warp_fbm4(p + vec2(7.8)));
}

vec2 warp_fbm6_2(vec2 p) {
    return vec2(warp_fbm6(p + vec2(16.8)), warp_fbm6(p + vec2(11.5)));
}

float warp_func(vec2 q, out vec4 ron) {
    q += 0.03 * sin(vec2(0.27, 0.23) * iTime + length(q) * vec2(4.1, 4.3));
    vec2 o = warp_fbm4_2(0.9 * q);
    o += 0.04 * sin(vec2(0.12, 0.14) * iTime + length(o));
    vec2 n = warp_fbm4_2(3.0 * o);
    ron = vec4(o, n);
    float f = 0.5 + 0.5 * warp_fbm4(1.8 * q + 6.0 * n);
    return mix(f, f * f * f * 3.5, f * abs(n.x));
}

vec3 world_ray() {
    vec2 ndc = (gl_FragCoord.xy / max(iResolution, vec2(1.0))) * 2.0 - 1.0;
    return normalize(u_ray_right * ndc.x + u_ray_up * ndc.y + u_ray_forward);
}

vec2 sky_uv(vec3 dir) {
    vec3 d = normalize(dir);
    float denom = max(1.0 + d.y, 0.2);
    return (d.xz / denom) * (1.6 * u_scale);
}

vec3 sky_marbling(vec2 uv) {
    for (float i = 1.0; i < 7.0; i++) {
        uv.x += 0.6 / i * cos(i * 2.5 * uv.y + iTime);
        uv.y += 0.6 / i * cos(i * 1.5 * uv.x + iTime);
    }
    float v = abs(sin(iTime - uv.y - uv.x));
    v = max(v, 0.08);
    return u_bg_color + u_shader_color * (0.12 / v);
}

vec3 sky_warp_fbm(vec2 p) {
    vec4 on = vec4(0.0);
    float f = warp_func(p, on);
    f = clamp(f, 0.001, 8.0);
    return mix(u_bg_color, u_shader_color, clamp(f, 0.0, 1.0)) * (0.45 + f * 0.55);
}

// --- Auroras by nimitz 2017 (https://www.shadertoy.com/view/XtGGRt) ---
mat2 aurora_mm2(float a) {
    float c = cos(a), s = sin(a);
    return mat2(c, s, -s, c);
}

const mat2 AURORA_M2 = mat2(0.95534, 0.29552, -0.29552, 0.95534);

float aurora_tri(float x) {
    return clamp(abs(fract(x) - 0.5), 0.01, 0.49);
}

vec2 aurora_tri2(vec2 p) {
    return vec2(aurora_tri(p.x) + aurora_tri(p.y), aurora_tri(p.y + aurora_tri(p.x)));
}

float aurora_tri_noise2d(vec2 p, float spd) {
    float z = 1.8;
    float z2 = 2.5;
    float rz = 0.0;
    p *= aurora_mm2(p.x * 0.06);
    vec2 bp = p;
    for (float i = 0.0; i < 5.0; i++) {
        vec2 dg = aurora_tri2(bp * 1.85) * 0.75;
        dg *= aurora_mm2(iTime * spd);
        p -= dg / z2;
        bp *= 1.3;
        z2 *= 0.45;
        z *= 0.42;
        p *= 1.21 + (rz - 1.0) * 0.02;
        rz += aurora_tri(p.x + aurora_tri(p.y)) * z;
        p *= -AURORA_M2;
    }
    return clamp(1.0 / pow(rz * 29.0, 1.3), 0.0, 0.55);
}

float aurora_hash21(vec2 n) {
    return fract(sin(dot(n, vec2(12.9898, 4.1414))) * 43758.5453);
}

vec4 aurora_volume(vec3 ro, vec3 rd) {
    vec4 col = vec4(0.0);
    vec4 avg_col = vec4(0.0);
    for (float i = 0.0; i < 24.0; i++) {
        float of = 0.006 * aurora_hash21(gl_FragCoord.xy) * smoothstep(0.0, 10.0, i);
        float pt = ((0.8 + pow(i, 1.4) * 0.002) - ro.y) / (rd.y * 2.0 + 0.4);
        pt -= of;
        vec3 bpos = ro + pt * rd;
        vec2 p = bpos.zx * u_scale;
        float rzt = aurora_tri_noise2d(p, 0.06);
        vec4 col2 = vec4(0.0, 0.0, 0.0, rzt);
        col2.rgb = (sin(1.0 - vec3(2.15, -0.5, 1.2) + i * 0.043) * 0.5 + 0.5) * rzt;
        col2.rgb = mix(col2.rgb, u_shader_color * rzt, 0.35);
        avg_col = mix(avg_col, col2, 0.5);
        col += avg_col * exp2(-i * 0.065 - 2.5) * smoothstep(0.0, 5.0, i);
    }
    col *= clamp(rd.y * 15.0 + 0.4, 0.0, 1.0);
    return col * 1.8;
}

vec3 aurora_hash33(vec3 p) {
    p = fract(p * vec3(443.8975, 397.2973, 491.1871));
    p += dot(p.zxy, p.yxz + 19.27);
    return fract(vec3(p.x * p.y, p.z * p.x, p.y * p.z));
}

vec3 aurora_stars(vec3 p) {
    vec3 c = vec3(0.0);
    float res = iResolution.x * 1.0;
    for (float i = 0.0; i < 2.0; i++) {
        vec3 q = fract(p * (0.15 * res)) - 0.5;
        vec3 id = floor(p * (0.15 * res));
        vec2 rn = aurora_hash33(id).xy;
        float c2 = 1.0 - smoothstep(0.0, 0.6, length(q));
        c2 *= step(rn.x, 0.0005 + i * i * 0.001);
        c += c2 * (mix(vec3(1.0, 0.49, 0.1), vec3(0.75, 0.9, 1.0), rn.y) * 0.1 + 0.9);
        p *= 1.3;
    }
    return c * c * 0.8;
}

vec3 aurora_bg(vec3 rd) {
    float sd = dot(normalize(vec3(-0.5, -0.6, 0.9)), rd) * 0.5 + 0.5;
    sd = pow(sd, 5.0);
    vec3 col = mix(u_bg_color, u_shader_color * 0.35, sd);
    return col * 0.63;
}

vec3 sky_auroras(vec3 rd) {
    vec3 ro = vec3(0.0, 0.0, -6.7);
    float fade = smoothstep(0.0, 0.01, abs(rd.y)) * 0.1 + 0.9;
    vec3 col = vec3(0.0);

    if (rd.y > 0.0) {
        col = aurora_bg(rd) * fade;
        vec4 aur = smoothstep(0.0, 1.5, aurora_volume(ro, rd)) * fade;
        col += aurora_stars(rd);
        col = col * (1.0 - aur.a) + aur.rgb;
    } else {
        rd.y = abs(rd.y);
        col = aurora_bg(rd) * fade * 0.6;
        vec4 aur = smoothstep(0.0, 2.5, aurora_volume(ro, rd));
        col += aurora_stars(rd) * 0.1;
        col = col * (1.0 - aur.a) + aur.rgb;
        vec3 pos = ro + ((0.5 - ro.y) / rd.y) * rd;
        float nz2 = aurora_tri_noise2d(pos.xz * vec2(0.5, 0.7) * u_scale, 0.0);
        col += mix(vec3(0.2, 0.25, 0.5) * 0.08, vec3(0.3, 0.3, 0.5) * 0.7, nz2 * 0.4);
    }
    return col;
}

void main() {
    vec3 rd = world_ray();
    if (rd.y < -0.50) {
        discard;
    }
    float horizon_fade = smoothstep(-0.50, 0.05, rd.y);
    vec3 sky;
    if (u_mode == 2) {
        sky = sky_auroras(rd);
    } else if (u_mode == 1) {
        sky = sky_warp_fbm(sky_uv(rd));
    } else {
        sky = sky_marbling(sky_uv(rd));
    }
    fragColor = vec4(sky * u_intensity * horizon_fade, horizon_fade);
}
"#;
