//! Selective, real backdrop blur for small interface surfaces.
//!
//! The callback captures only its padded rectangle at quarter resolution, then
//! performs two five-tap Gaussian passes. Nothing is read back to the CPU and
//! no repaint is requested here. Egui must paint the backdrop before this shape.
use eframe::{egui, egui_glow, glow};
use glow::HasContext as _;
use std::sync::{Arc, Mutex};

const MAX_SIDE: i32 = 1024;
const MAX_AREA: i32 = 262_144;
const DOWNSAMPLE: f32 = 4.0;
const PADDING_POINTS: f32 = 24.0;

#[derive(Clone, Default)]
pub(super) struct GlassRenderer {
    state: Arc<Mutex<RendererState>>,
}

#[derive(Default)]
enum RendererState {
    #[default]
    Uninitialized,
    Ready(GpuBlur),
    Unavailable,
}

impl GlassRenderer {
    /// Paint after the background and before controls. `tint` should retain
    /// enough opacity for readable controls even if the driver cannot blur.
    pub(super) fn paint(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        radius: f32,
        tint: egui::Color32,
        strength: f32,
    ) {
        painter.add(self.shape(rect, radius, tint, strength));
    }

    /// A single reserved paint slot lets menus determine their final bounds
    /// while still drawing the blur before their text and controls.
    pub(super) fn shape(
        &self,
        rect: egui::Rect,
        radius: f32,
        tint: egui::Color32,
        strength: f32,
    ) -> egui::Shape {
        if !rect.is_positive() || !rect.is_finite() {
            return egui::Shape::Noop;
        }
        let mut shapes = Vec::with_capacity(3);
        let state = Arc::clone(&self.state);
        let strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            0.5
        };
        if strength > 0.0 {
            shapes.push(egui::Shape::Callback(egui::PaintCallback {
                rect,
                callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                    let Some(plan) = CapturePlan::new(&info, radius) else {
                        return;
                    };
                    let Ok(mut state) = state.lock() else {
                        return;
                    };
                    if matches!(*state, RendererState::Unavailable) {
                        return;
                    }
                    let gl = painter.gl();
                    let version = gl.version();
                    if version.major < 3
                        || (!version.is_embedded && version.major == 3 && version.minor < 3)
                    {
                        *state = RendererState::Unavailable;
                        return;
                    }
                    // All mutations, including failed lazy initialization, are
                    // enclosed by the guard. Egui restores its own rendering state
                    // too, but does not restore read/draw framebuffer bindings.
                    unsafe {
                        let _guard = GlState::capture(gl);
                        if matches!(*state, RendererState::Uninitialized) {
                            *state = match GpuBlur::new(gl) {
                                Ok(renderer) => RendererState::Ready(renderer),
                                Err(()) => RendererState::Unavailable,
                            };
                        }
                        if let RendererState::Ready(renderer) = &mut *state {
                            if renderer.paint(gl, &info, &plan, strength).is_err() {
                                renderer.destroy(gl);
                                *state = RendererState::Unavailable;
                            }
                        }
                    }
                })),
            }));
        }
        shapes.push(egui::Shape::rect_filled(rect, radius, tint));
        shapes.push(egui::Shape::rect_stroke(
            rect.shrink(0.5),
            radius,
            egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(26)),
            egui::StrokeKind::Inside,
        ));
        egui::Shape::Vec(shapes)
    }

    /// Call from eframe::App::on_exit while its GL context is current.
    pub(super) fn destroy(&self, gl: &glow::Context) {
        if let Ok(mut state) = self.state.lock() {
            if let RendererState::Ready(renderer) = &mut *state {
                unsafe { renderer.destroy(gl) };
            }
            *state = RendererState::Unavailable;
        }
    }
}

#[derive(Debug)]
struct CapturePlan {
    source: [i32; 4], // x, y from bottom, width, height
    small: [i32; 2],
    viewport: [i32; 4],
    clip: [i32; 4],
    uv_origin: [f32; 2],
    uv_extent: [f32; 2],
    radius_px: f32,
}

impl CapturePlan {
    fn new(info: &egui::PaintCallbackInfo, radius: f32) -> Option<Self> {
        let ppp = info.pixels_per_point;
        if !ppp.is_finite() || ppp <= 0.0 || !info.viewport.is_finite() {
            return None;
        }
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(info.screen_size_px[0] as f32, info.screen_size_px[1] as f32) / ppp,
        );
        let visible = info.viewport.intersect(info.clip_rect).intersect(screen);
        if !visible.is_positive() {
            return None;
        }
        let viewport = info.viewport_in_pixels();
        if viewport.width_px <= 0 || viewport.height_px <= 0 {
            return None;
        }
        let padded = info.viewport.expand(PADDING_POINTS).intersect(screen);
        let source_info = egui::PaintCallbackInfo {
            viewport: padded,
            clip_rect: padded,
            pixels_per_point: ppp,
            screen_size_px: info.screen_size_px,
        };
        let source = source_info.viewport_in_pixels();
        let small = [
            (source.width_px as f32 / DOWNSAMPLE).ceil().max(1.0) as i32,
            (source.height_px as f32 / DOWNSAMPLE).ceil().max(1.0) as i32,
        ];
        // Bound driver work even with a giant window or unusually high DPI.
        if small[0] > MAX_SIDE || small[1] > MAX_SIDE || small[0] * small[1] > MAX_AREA {
            return None;
        }
        let clip_info = egui::PaintCallbackInfo {
            viewport: visible,
            clip_rect: visible,
            pixels_per_point: ppp,
            screen_size_px: info.screen_size_px,
        };
        let clip = clip_info.viewport_in_pixels();
        Some(Self {
            source: [
                source.left_px,
                source.from_bottom_px,
                source.width_px,
                source.height_px,
            ],
            small,
            viewport: [
                viewport.left_px,
                viewport.from_bottom_px,
                viewport.width_px,
                viewport.height_px,
            ],
            clip: [
                clip.left_px,
                clip.from_bottom_px,
                clip.width_px,
                clip.height_px,
            ],
            uv_origin: [
                (viewport.left_px - source.left_px) as f32 / source.width_px as f32,
                (viewport.from_bottom_px - source.from_bottom_px) as f32 / source.height_px as f32,
            ],
            uv_extent: [
                viewport.width_px as f32 / source.width_px as f32,
                viewport.height_px as f32 / source.height_px as f32,
            ],
            radius_px: (radius.max(0.0) * ppp)
                .min(viewport.width_px.min(viewport.height_px) as f32 * 0.5),
        })
    }
}

struct GpuBlur {
    program: glow::Program,
    vertex_array: glow::VertexArray,
    textures: [glow::Texture; 2],
    framebuffers: [glow::Framebuffer; 2],
    size: [i32; 2],
    sampler: Option<glow::UniformLocation>,
    step: Option<glow::UniformLocation>,
    origin: Option<glow::UniformLocation>,
    extent: Option<glow::UniformLocation>,
    panel: Option<glow::UniformLocation>,
    radius: Option<glow::UniformLocation>,
}

impl GpuBlur {
    unsafe fn new(gl: &glow::Context) -> Result<Self, ()> {
        // GL 3.3 / ES 3 provide framebuffer blitting and sampler objects. Older
        // contexts keep the translucent tint without risking unsupported calls.
        if gl.version().major < 3 {
            return Err(());
        }
        let prefix = if gl.version().is_embedded {
            "#version 300 es\nprecision highp float;\n"
        } else {
            "#version 130\n"
        };
        let program = unsafe { make_program(gl, prefix)? };
        let mut textures = Vec::new();
        let mut framebuffers = Vec::new();
        let mut vertex_array = None;
        let allocated = (|| unsafe {
            vertex_array = Some(gl.create_vertex_array().map_err(|_| ())?);
            for _ in 0..2 {
                textures.push(gl.create_texture().map_err(|_| ())?);
                framebuffers.push(gl.create_framebuffer().map_err(|_| ())?);
            }
            Ok::<_, ()>(())
        })();
        if allocated.is_err() {
            unsafe {
                for texture in textures {
                    gl.delete_texture(texture);
                }
                for framebuffer in framebuffers {
                    gl.delete_framebuffer(framebuffer);
                }
                if let Some(vertex_array) = vertex_array {
                    gl.delete_vertex_array(vertex_array);
                }
                gl.delete_program(program);
            }
            return Err(());
        }
        unsafe {
            Ok(Self {
                program,
                vertex_array: vertex_array.ok_or(())?,
                textures: [textures[0], textures[1]],
                framebuffers: [framebuffers[0], framebuffers[1]],
                size: [0, 0],
                sampler: gl.get_uniform_location(program, "u_texture"),
                step: gl.get_uniform_location(program, "u_step"),
                origin: gl.get_uniform_location(program, "u_origin"),
                extent: gl.get_uniform_location(program, "u_extent"),
                panel: gl.get_uniform_location(program, "u_panel"),
                radius: gl.get_uniform_location(program, "u_radius"),
            })
        }
    }

    unsafe fn allocate(&mut self, gl: &glow::Context, size: [i32; 2]) -> Result<(), ()> {
        if self.size[0] >= size[0] && self.size[1] >= size[1] {
            return Ok(());
        }
        // Reuse a grow-only pair when multiple small surfaces have different
        // dimensions. This avoids reallocating textures on every paint. The
        // area cap still applies if their combined extents would be excessive.
        let grown = [self.size[0].max(size[0]), self.size[1].max(size[1])];
        let size = if grown[0] * grown[1] <= MAX_AREA {
            grown
        } else {
            size
        };
        unsafe {
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            for i in 0..2 {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[i]));
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
                    size[0],
                    size[1],
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffers[i]));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(self.textures[i]),
                    0,
                );
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                    return Err(());
                }
            }
        }
        self.size = size;
        Ok(())
    }

    unsafe fn paint(
        &mut self,
        gl: &glow::Context,
        info: &egui::PaintCallbackInfo,
        plan: &CapturePlan,
        strength: f32,
    ) -> Result<(), ()> {
        unsafe {
            let target = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            // Scroll Studio explicitly requests multisampling=0. If a future
            // renderer enables MSAA, decline scaling a multisampled source:
            // OpenGL permits resolving only at identical dimensions.
            if gl.get_parameter_i32(glow::SAMPLES) > 0 {
                return Ok(());
            }
            gl.active_texture(glow::TEXTURE0);
            gl.bind_sampler(0, None);
            self.allocate(gl, plan.small)?;
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, target);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.framebuffers[0]));
            let [x, y, w, h] = plan.source;
            gl.blit_framebuffer(
                x,
                y,
                x + w,
                y + h,
                0,
                0,
                self.size[0],
                self.size[1],
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vertex_array));
            gl.uniform_1_i32(self.sampler.as_ref(), 0);
            gl.uniform_2_f32(self.origin.as_ref(), 0.0, 0.0);
            gl.uniform_2_f32(self.extent.as_ref(), 1.0, 1.0);
            gl.uniform_2_f32(self.panel.as_ref(), 0.0, 0.0);
            gl.uniform_1_f32(self.radius.as_ref(), 0.0);
            let step_px = (1.0 + strength * 5.0) * info.pixels_per_point;
            gl.uniform_2_f32(self.step.as_ref(), step_px / plan.source[2] as f32, 0.0);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.framebuffers[1]));
            gl.viewport(0, 0, self.size[0], self.size[1]);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[0]));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, target);
            gl.enable(glow::BLEND);
            gl.enable(glow::SCISSOR_TEST);
            let [x, y, w, h] = plan.clip;
            gl.scissor(x, y, w, h);
            let [x, y, w, h] = plan.viewport;
            gl.viewport(x, y, w, h);
            gl.uniform_2_f32(self.origin.as_ref(), plan.uv_origin[0], plan.uv_origin[1]);
            gl.uniform_2_f32(self.extent.as_ref(), plan.uv_extent[0], plan.uv_extent[1]);
            gl.uniform_2_f32(self.panel.as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(self.radius.as_ref(), plan.radius_px);
            gl.uniform_2_f32(self.step.as_ref(), 0.0, step_px / plan.source[3] as f32);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[1]));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            if gl.get_error() != glow::NO_ERROR {
                return Err(());
            }
        }
        Ok(())
    }

    unsafe fn destroy(&self, gl: &glow::Context) {
        unsafe {
            for texture in self.textures {
                gl.delete_texture(texture);
            }
            for framebuffer in self.framebuffers {
                gl.delete_framebuffer(framebuffer);
            }
            gl.delete_vertex_array(self.vertex_array);
            gl.delete_program(self.program);
        }
    }
}

unsafe fn make_program(gl: &glow::Context, prefix: &str) -> Result<glow::Program, ()> {
    let program = unsafe { gl.create_program().map_err(|_| ())? };
    let mut shaders = Vec::new();
    let built = (|| unsafe {
        for (kind, source) in [
            (glow::VERTEX_SHADER, VERTEX),
            (glow::FRAGMENT_SHADER, FRAGMENT),
        ] {
            let shader = gl.create_shader(kind).map_err(|_| ())?;
            shaders.push(shader);
            gl.shader_source(shader, &format!("{prefix}{source}"));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                eprintln!(
                    "Scroll Studio glass shader unavailable: {}",
                    gl.get_shader_info_log(shader)
                );
                return Err(());
            }
            gl.attach_shader(program, shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            eprintln!(
                "Scroll Studio glass program unavailable: {}",
                gl.get_program_info_log(program)
            );
            return Err(());
        }
        Ok(())
    })();
    unsafe {
        for shader in shaders {
            gl.delete_shader(shader);
        }
        if built.is_err() {
            gl.delete_program(program);
        }
    }
    built.map(|()| program)
}

// Two bilinear pairs plus the center produce a nine-pixel Gaussian kernel with
// only five texture reads. Quarter-resolution capture broadens the soft blur.
const VERTEX: &str = r#"
out vec2 v_uv;
void main() {
    vec2 p = gl_VertexID == 0 ? vec2(-1.0, -1.0) :
             gl_VertexID == 1 ? vec2(3.0, -1.0) : vec2(-1.0, 3.0);
    v_uv = p * 0.5 + 0.5;
    gl_Position = vec4(p, 0.0, 1.0);
}
"#;

const FRAGMENT: &str = r#"
uniform sampler2D u_texture;
uniform vec2 u_step;
uniform vec2 u_origin;
uniform vec2 u_extent;
uniform vec2 u_panel;
uniform float u_radius;
in vec2 v_uv;
out vec4 color;
void main() {
    vec2 uv = u_origin + v_uv * u_extent;
    vec3 rgb = texture(u_texture, uv).rgb * 0.2270270270;
    rgb += texture(u_texture, uv + u_step * 1.3846153846).rgb * 0.3162162162;
    rgb += texture(u_texture, uv - u_step * 1.3846153846).rgb * 0.3162162162;
    rgb += texture(u_texture, uv + u_step * 3.2307692308).rgb * 0.0702702703;
    rgb += texture(u_texture, uv - u_step * 3.2307692308).rgb * 0.0702702703;
    float alpha = 1.0;
    if (u_panel.x > 0.0) {
        vec2 q = abs((v_uv - 0.5) * u_panel) - u_panel * 0.5 + vec2(u_radius);
        float d = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - u_radius;
        alpha = 1.0 - smoothstep(-0.75, 0.75, d);
    }
    color = vec4(rgb * alpha, alpha);
}
"#;

/// Save only state that this callback changes. In particular, egui's callback
/// reset does not repair FBO bindings, unpack buffers, or texture samplers.
struct GlState<'a> {
    gl: &'a glow::Context,
    read: Option<glow::Framebuffer>,
    draw: Option<glow::Framebuffer>,
    program: Option<glow::Program>,
    vao: Option<glow::VertexArray>,
    unpack: Option<glow::Buffer>,
    active_texture: u32,
    texture_zero: Option<glow::Texture>,
    sampler_zero: Option<glow::Sampler>,
    viewport: [i32; 4],
    scissor: [i32; 4],
    scissor_enabled: bool,
    blend_enabled: bool,
}

impl<'a> GlState<'a> {
    unsafe fn capture(gl: &'a glow::Context) -> Self {
        unsafe {
            let active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
            gl.active_texture(glow::TEXTURE0);
            let mut viewport = [0; 4];
            let mut scissor = [0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
            Self {
                gl,
                read: gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                draw: gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                program: gl.get_parameter_program(glow::CURRENT_PROGRAM),
                vao: gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING),
                unpack: gl.get_parameter_buffer(glow::PIXEL_UNPACK_BUFFER_BINDING),
                active_texture,
                texture_zero: gl.get_parameter_texture(glow::TEXTURE_BINDING_2D),
                sampler_zero: gl.get_parameter_sampler(glow::SAMPLER_BINDING),
                viewport,
                scissor,
                scissor_enabled: gl.is_enabled(glow::SCISSOR_TEST),
                blend_enabled: gl.is_enabled(glow::BLEND),
            }
        }
    }
}

impl Drop for GlState<'_> {
    fn drop(&mut self) {
        unsafe {
            let gl = self.gl;
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, self.read);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.draw);
            gl.use_program(self.program);
            gl.bind_vertex_array(self.vao);
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, self.unpack);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, self.texture_zero);
            gl.bind_sampler(0, self.sampler_zero);
            gl.active_texture(self.active_texture);
            let [x, y, w, h] = self.viewport;
            gl.viewport(x, y, w, h);
            let [x, y, w, h] = self.scissor;
            gl.scissor(x, y, w, h);
            for (capability, enabled) in [
                (glow::SCISSOR_TEST, self.scissor_enabled),
                (glow::BLEND, self.blend_enabled),
            ] {
                if enabled {
                    gl.enable(capability)
                } else {
                    gl.disable(capability)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(rect: egui::Rect, ppp: f32) -> egui::PaintCallbackInfo {
        egui::PaintCallbackInfo {
            viewport: rect,
            clip_rect: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 760.0)),
            pixels_per_point: ppp,
            screen_size_px: [(1280.0 * ppp) as u32, (760.0 * ppp) as u32],
        }
    }

    #[test]
    fn capture_is_padded_and_quarter_resolution_with_correct_bottom_origin() {
        let input = info(
            egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 60.0)),
            1.0,
        );
        let plan = CapturePlan::new(&input, 14.0).unwrap();
        assert_eq!(plan.source, [76, 626, 448, 108]);
        assert_eq!(plan.small, [112, 27]);
        assert_eq!(plan.viewport, [100, 650, 400, 60]);
        assert_eq!(plan.clip, plan.viewport);
        assert_eq!(plan.radius_px, 14.0);
        assert!((plan.uv_origin[0] - 24.0 / 448.0).abs() < 0.0001);
        assert!((plan.uv_origin[1] - 24.0 / 108.0).abs() < 0.0001);
    }

    #[test]
    fn capture_clamps_to_screen_and_obeys_clip_at_high_dpi() {
        let mut input = info(
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 60.0)),
            2.0,
        );
        input.clip_rect = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(300.0, 50.0));
        let plan = CapturePlan::new(&input, 100.0).unwrap();
        assert_eq!(plan.source, [0, 1352, 848, 168]);
        assert_eq!(plan.clip, [20, 1420, 580, 80]);
        assert_eq!(plan.radius_px, 60.0);
        assert_eq!(plan.uv_origin[0], 0.0);
        assert_eq!(plan.uv_extent[0], 800.0 / 848.0);
    }

    #[test]
    fn invalid_offscreen_and_unbounded_work_are_declined() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 60.0));
        assert!(CapturePlan::new(&info(rect, f32::NAN), 10.0).is_none());
        let offscreen = rect.translate(egui::vec2(3000.0, 0.0));
        assert!(CapturePlan::new(&info(offscreen, 1.0), 10.0).is_none());
        let full = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 760.0));
        assert!(CapturePlan::new(&info(full, 4.0), 10.0).is_none());
    }
}
