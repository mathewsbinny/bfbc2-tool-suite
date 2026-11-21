//! The editor without a window: the whole interface run frame by frame on a GPU device of its own, with input
//! events injected, and drawn into an image (the tests' proofs and the screenshots).

use std::sync::Arc;

use eframe::egui::{self, Pos2, Rect, Vec2};
use eframe::egui_wgpu::{self, wgpu};
use eframe::epaint::mutex::RwLock;

use crate::app::{EditorApp, GpuCtx};
use crate::error::EditorError;
use crate::render::{self, Gpu};

/// A headless window.
pub struct Headless {
    /// The GPU and egui's renderer.
    pub gpu: GpuCtx,
    /// egui.
    pub ctx: egui::Context,
    /// Size in pixels (= points: 1 pixel per point).
    pub size: [u32; 2],
    time: f64,
    last: Option<egui::FullOutput>,
    /// The modifier keys held in every frame (Alt for the sink wheel ...).
    pub modifiers: egui::Modifiers,
    sent_modifiers: egui::Modifiers,
}

impl Headless {
    /// A headless window of `size` pixels.
    ///
    /// # Errors
    ///
    /// No graphics device.
    pub fn new(size: [u32; 2]) -> Result<Self, EditorError> {
        let (gpu, adapter) = Gpu::headless()?;
        let egui = egui_wgpu::Renderer::new(&gpu.device, render::COLOR_FORMAT, egui_wgpu::RendererOptions::default());
        let ctx = egui::Context::default();
        Ok(Self {
            gpu: GpuCtx {
                gpu,
                egui: Arc::new(RwLock::new(egui)),
                adapter,
            },
            ctx,
            size,
            time: 0.0,
            last: None,
            modifiers: egui::Modifiers::NONE,
            sent_modifiers: egui::Modifiers::NONE,
        })
    }

    /// Runs one frame of the app with these input events.
    pub fn frame(&mut self, app: &mut EditorApp, mut events: Vec<egui::Event>) {
        self.time += 1.0 / 60.0;
        if self.modifiers != self.sent_modifiers {
            events.insert(0, egui::Event::ModifiersChanged(self.modifiers));
            self.sent_modifiers = self.modifiers;
        }
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(self.size[0] as f32, self.size[1] as f32),
            )),
            time: Some(self.time),
            predicted_dt: 1.0 / 60.0,
            events,
            focused: true,
            ..Default::default()
        };
        let gpu = self.gpu.clone();
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui, &gpu));
        // egui's texture changes go to its renderer at once; the image is drawn from the last frame's shapes
        {
            let mut r = self.gpu.egui.write();
            for (id, deltas) in &out.textures_delta.set {
                for delta in deltas {
                    r.update_texture(&self.gpu.gpu.device, &self.gpu.gpu.queue, *id, delta);
                }
            }
            for id in &out.textures_delta.free {
                r.free_texture(id);
            }
        }
        out.textures_delta.clear();
        self.last = Some(out);
    }

    /// Runs `n` frames without input (layout and animations settle).
    pub fn settle(&mut self, app: &mut EditorApp, n: usize) {
        for _ in 0..n {
            self.frame(app, Vec::new());
        }
    }

    /// A left click at a point.
    pub fn click(&mut self, app: &mut EditorApp, at: Pos2) {
        self.frame(app, vec![egui::Event::PointerMoved(at)]);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(app, vec![button(true)]);
        self.frame(app, vec![button(false)]);
        self.frame(app, Vec::new());
    }

    /// A right click at a point (a context menu).
    pub fn right_click(&mut self, app: &mut EditorApp, at: Pos2) {
        self.frame(app, vec![egui::Event::PointerMoved(at)]);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(app, vec![button(true)]);
        self.frame(app, vec![button(false)]);
        self.frame(app, Vec::new());
    }

    /// Wheel notches over a point, one a frame (negative: towards the user), with the held modifiers.
    pub fn wheel(&mut self, app: &mut EditorApp, at: Pos2, notches: i32) {
        self.frame(app, vec![egui::Event::PointerMoved(at)]);
        for _ in 0..notches.unsigned_abs() {
            let ev = egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: Vec2::new(0.0, notches.signum() as f32),
                phase: egui::TouchPhase::Move,
                modifiers: self.modifiers,
            };
            self.frame(app, vec![ev]);
        }
        self.frame(app, Vec::new());
    }

    /// The pointer moved to a point.
    pub fn hover(&mut self, app: &mut EditorApp, at: Pos2) {
        self.frame(app, vec![egui::Event::PointerMoved(at)]);
        self.frame(app, Vec::new());
    }

    /// A left-button drag from one point to another in `steps` moves, with these modifiers held.
    pub fn drag(&mut self, app: &mut EditorApp, from: Pos2, to: Pos2, steps: usize, modifiers: egui::Modifiers) {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        self.frame(app, vec![egui::Event::PointerMoved(from)]);
        self.frame(app, vec![button(from, true)]);
        let n = steps.max(1);
        for k in 1..=n {
            let t = k as f32 / n as f32;
            self.frame(app, vec![egui::Event::PointerMoved(from + (to - from) * t)]);
        }
        self.frame(app, vec![button(to, false)]);
        self.frame(app, Vec::new());
    }

    /// A key pressed and released with these modifiers (shortcuts).
    pub fn key(&mut self, app: &mut EditorApp, key: egui::Key, modifiers: egui::Modifiers) {
        let ev = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(app, vec![ev(true)]);
        self.frame(app, vec![ev(false)]);
        self.frame(app, Vec::new());
    }

    /// The last frame drawn, as RGBA rows.
    #[must_use]
    pub fn image(&mut self) -> Option<Vec<u8>> {
        let out = self.last.take()?;
        let device = &self.gpu.gpu.device;
        let queue = &self.gpu.gpu.queue;
        let jobs = self.ctx.tessellate(out.shapes.clone(), out.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: self.size,
            pixels_per_point: out.pixels_per_point,
        };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless window"),
            size: wgpu::Extent3d {
                width: self.size[0],
                height: self.size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render::COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless"),
        });
        {
            let mut r = self.gpu.egui.write();
            let _ = r.update_buffers(device, queue, &mut enc, &jobs, &screen);
            let pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            r.render(&mut pass, &jobs, &screen);
        }
        queue.submit([enc.finish()]);

        let rgba = render::read_rgba(&self.gpu.gpu, &tex, self.size)?;
        self.last = Some(out);
        Some(rgba)
    }

    /// The last frame as a PNG file.
    ///
    /// # Errors
    ///
    /// Nothing drawn yet, or the file cannot be written.
    pub fn save(&mut self, path: &std::path::Path) -> Result<(), EditorError> {
        let rgba = self.image().ok_or(EditorError::Gpu("nothing drawn".into()))?;
        render::write_png(path, self.size, &rgba)
    }
}
