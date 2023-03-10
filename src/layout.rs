use crate::{components::{self, Component, Point, Rect}, CustomEvents, framelimiter::FrameLimiter};
use async_trait::async_trait;
use std::sync::Arc;
use winit::{event::WindowEvent, event_loop::EventLoopWindowTarget, window::Window};

pub enum WindowLifeStatus {
	Alive,
	Dead,
}

#[derive(Default)]
pub struct LayoutContext {
	wgpu: Option<wgpu::Instance>,
}

#[allow(unused)]
pub struct InputHandler {
	mouse_position: Option<Point>,
}

#[allow(unused)]
impl InputHandler {
	fn get_mouse_absolute(&self) -> &Option<Point> {
		return &self.mouse_position;
	}

	fn get_mouse_relative(&self, r: Rect) -> Option<Point> {
		if let Some(p) = self.mouse_position {
			if r.inside(p) {
				return Some(p - r.pos);
			}
		}

		return None;
	}

	fn handle_event(&mut self, event: &WindowEvent) {
		match event {
			WindowEvent::CursorMoved { position, .. } => {
				self.mouse_position = Some(position.clone().into());
			}
			WindowEvent::CursorLeft { .. } => {
				self.mouse_position = None;
			}
			_ => (),
		}
	}
}

#[async_trait]
pub trait Layout {
	fn init() -> LayoutContext
	where
		Self: Sized;

	async fn new(_: LayoutContext, _: Arc<Window>) -> Box<Self>
	where
		Self: Sized;
	fn window(&self) -> Arc<Window>;
	fn render(&mut self);
	fn update(
		&mut self,
		_: &EventLoopWindowTarget<CustomEvents>,
	) -> (WindowLifeStatus, Option<Box<dyn Layout>>);

	/// Returns a life status and maybe another layout, notice that if the child layout uses the same window, the parent layout must pronounce itself as dead.
	fn event_handler(&mut self, _: winit::event::WindowEvent, _: &FrameLimiter);
}

pub struct DrawingWindow {
	window: Arc<Window>,
	surface: wgpu::Surface,
	queue: wgpu::Queue,
	config: wgpu::SurfaceConfiguration,
	size: winit::dpi::PhysicalSize<u32>,

	ctx: components::Context,

	canvas: Box<components::Canvas>,

	//Events:
	resized: bool,
	close: bool,
}

#[async_trait]
impl Layout for DrawingWindow {
	fn init() -> LayoutContext
	where
		Self: Sized,
	{
		LayoutContext {
			wgpu: Some(wgpu::Instance::new(wgpu::Backends::all())),
			..LayoutContext::default()
		}
	}

	async fn new(layout_ctx: LayoutContext, window: Arc<Window>) -> Box<Self> {
		let size = window.inner_size();

		let instance = layout_ctx.wgpu.expect("Generated with wrong context");
		let surface = unsafe { instance.create_surface(window.as_ref()) };

		let adapter = instance
			.request_adapter(&wgpu::RequestAdapterOptions {
				power_preference: wgpu::PowerPreference::default(),
				compatible_surface: Some(&surface),
				force_fallback_adapter: false,
			})
			.await
			.expect("Could not get adapter");

		let (device, queue) = adapter
			.request_device(
				&wgpu::DeviceDescriptor {
					features: wgpu::Features::PUSH_CONSTANTS
						| wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
					limits: wgpu::Limits {
						max_push_constant_size: 64,
						..wgpu::Limits::default()
					},
					label: None,
				},
				None,
			)
			.await
			.expect("Could not get device-queue pair");

		let config = wgpu::SurfaceConfiguration {
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
			format: surface.get_supported_formats(&adapter)[0],
			width: size.width,
			height: size.height,
			present_mode: wgpu::PresentMode::AutoNoVsync,
			alpha_mode: wgpu::CompositeAlphaMode::Auto,
		};

		surface.configure(&device, &config);

		let mut ctx = components::Context::new(device, config.format);

		let canvas = components::Canvas::new(&mut ctx);

		return Box::new(Self {
			window,
			surface,
			queue,
			config,
			size,

			ctx,
			canvas,

			resized: false,
			close: false,
		});
	}

	fn window(&self) -> Arc<Window> {
		self.window.clone()
	}

	fn render(&mut self) {
		match self.surface.get_current_texture() {
			Err(wgpu::SurfaceError::Lost) => self.resized = true,
			Err(wgpu::SurfaceError::OutOfMemory) => self.close = true,
			Err(e) => eprintln!("{:?}", e),
			Ok(output) => {
				let view = output
					.texture
					.create_view(&wgpu::TextureViewDescriptor::default());

				let mut encoder =
					self.ctx
						.device
						.create_command_encoder(&wgpu::CommandEncoderDescriptor {
							label: Some("Render Encoder"),
						});

				self.canvas.render(
					&mut encoder,
					&mut self.ctx,
					&view,
					components::Rect::new(0, 0, self.size.width, self.size.height),
					None,
				);

				self.ctx.staging_belt.finish();
				self.queue.submit(std::iter::once(encoder.finish()));
				self.ctx.staging_belt.recall();
				output.present();
			}
		}
	}

	fn update(
		&mut self,
		_: &EventLoopWindowTarget<CustomEvents>,
	) -> (WindowLifeStatus, Option<Box<dyn Layout>>) {
		use WindowLifeStatus::*;

		if self.resized {
			self.resized = false;
			let new_size = self.window().inner_size();
			if new_size.width <= 0 || new_size.height <= 0 {
				return (Alive, None);
			}

			self.size = new_size;
			self.config.width = new_size.width;
			self.config.height = new_size.height;
			self.surface.configure(&self.ctx.device, &self.config);
		}

		if self.close {
			self.close = false;
			return (Dead, None);
		}

		(Alive, None)
	}

	fn event_handler(&mut self, event: winit::event::WindowEvent, frame_limiter: &FrameLimiter) {
		use WindowEvent::*;

		match event {
			CloseRequested => self.close = true,

			Resized(_) => {
				self.resized = true;
			}

			KeyboardInput {
				input:
					winit::event::KeyboardInput {
						state: winit::event::ElementState::Released,
						virtual_keycode: Some(letter),
						..
					},
				..
			} => {
				let mut redraw = true;
				match letter {
					winit::event::VirtualKeyCode::C => self.canvas.clear(),
					_ => redraw = false,
				}
				if redraw {
					frame_limiter.schedule_redraw(self.window().id());
				}
			}

			MouseInput {
				state,
				button: winit::event::MouseButton::Left,
				..
			} => {
				use winit::event::ElementState;
				match state {
					ElementState::Pressed => self.canvas.mouse_down(),
					ElementState::Released => self.canvas.mouse_up(),
				}
				frame_limiter.schedule_redraw(self.window().id());
			}

			CursorMoved { position, .. } => {
				// TODO: Don't redraw window if no line was drawn
				self.canvas.mouse_pos(position.into());
				frame_limiter.schedule_redraw(self.window().id());
			}

			_ => (),
		}
	}
}
