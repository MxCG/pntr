use bytemuck::{Pod, Zeroable};
use core::ops;

use std::{
	any::TypeId,
	collections::HashMap,
	sync::{Arc, Weak},
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Point {
	pub x: i32,
	pub y: i32,
}

impl<P: Into<f64>> From<winit::dpi::PhysicalPosition<P>> for Point {
	fn from(value: winit::dpi::PhysicalPosition<P>) -> Self {
		return Point {
			x: value.x.into() as i32,
			y: value.y.into() as i32,
		};
	}
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Size {
	pub w: u32,
	pub h: u32,
}

impl TryFrom<Point> for Size {
	type Error = core::num::TryFromIntError;
	fn try_from(value: Point) -> Result<Self, Self::Error> {
		return Ok(Size {
			w: value.x.try_into()?,
			h: value.y.try_into()?,
		});
	}
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Rect {
	pub pos: Point,
	pub size: Size,
}

impl Rect {
	pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
		Self {
			pos: Point { x, y },
			size: Size { w, h },
		}
	}

	pub fn inside(&self, point: Point) -> bool {
		macro_rules! inside_dim {
			($dimP:ident, $dimS:ident) => {
				self.pos.$dimP <= point.$dimP
					&& point.$dimP <= self.pos.$dimP + self.size.$dimS as i32
			};
		}
		inside_dim!(x, w) && inside_dim!(y, h)
	}
}

impl ops::AddAssign for Point {
	fn add_assign(&mut self, other: Self) {
		self.x += other.x;
		self.y += other.y;
	}
}

impl ops::Add for Point {
	type Output = Self;
	fn add(self, other: Self) -> Self {
		let mut r = self.clone();
		r += other;
		r
	}
}

impl ops::Sub for Point {
	type Output = Self;
	fn sub(self, rhs: Self) -> Self::Output {
		return Point {
			x: self.x - rhs.x,
			y: self.y - rhs.y,
		};
	}
}

impl ops::AddAssign<Point> for Rect {
	fn add_assign(&mut self, other: Point) {
		self.pos += other;
	}
}

impl ops::Add<Point> for Rect {
	type Output = Self;

	fn add(self, other: Point) -> Self::Output {
		let mut r = self.clone();
		r += other;
		r
	}
}

pub trait RectViewportClipSpace {
	fn set_viewport_rect(&mut self, _: Rect);
	fn set_clipspace_rect(&mut self, _: Option<Rect>);
}

impl RectViewportClipSpace for wgpu::RenderPass<'_> {
	fn set_viewport_rect(&mut self, r: Rect) {
		self.set_viewport(
			r.pos.x as f32,
			r.pos.y as f32,
			r.size.w as f32,
			r.size.h as f32,
			0.,
			1.,
		);
	}

	fn set_clipspace_rect(&mut self, or: Option<Rect>) {
		if let Some(r) = or {
			use std::cmp::{max, min};
			let x = max(r.pos.x as u32, 0);
			let y = max(r.pos.y as u32, 0);
			let w = r.size.w + min(r.pos.x as u32, 0);
			let h = r.size.h + min(r.pos.y as u32, 0);
			self.set_scissor_rect(x, y, w, h);
		}
	}
}

pub struct Pipelines {
	pub render: Vec<wgpu::RenderPipeline>,
	pub compute: Vec<wgpu::ComputePipeline>,
}

pub trait Component {
	fn generate_pipelines(_: &Context) -> Pipelines;
	fn new(_: &mut Context) -> Box<Self>;
	fn min_size() -> Option<Size>;
	fn render(
		&mut self,
		_: &mut wgpu::CommandEncoder,
		_: &mut Context,
		output_texture: &wgpu::TextureView,
		view_port: Rect,
		clip_space: Option<Rect>,
	);
}

const STAGING_BUFFER_BYTES: u64 = 10;

pub struct Context {
	pub device: wgpu::Device,
	pub surface_format: wgpu::TextureFormat,
	pipeline_map: HashMap<TypeId, Weak<Pipelines>>,
	pub staging_belt: wgpu::util::StagingBelt,
}

impl Context {
	pub fn new(device: wgpu::Device, surface_format: wgpu::TextureFormat) -> Context {
		Context {
			device,
			surface_format,
			pipeline_map: HashMap::new(),
			staging_belt: wgpu::util::StagingBelt::new(4 * STAGING_BUFFER_BYTES),
		}
	}

	pub fn get_pipelines<T: Component + 'static>(&mut self) -> Arc<Pipelines> {
		if let Some(weak) = self.pipeline_map.get(&TypeId::of::<T>()) {
			if let Some(arc) = weak.upgrade() {
				return arc;
			}
		}

		let arc = Arc::new(T::generate_pipelines(self));
		self.pipeline_map
			.insert(TypeId::of::<T>(), Arc::downgrade(&arc));
		return arc;
	}
}

macro_rules! add_component {
	($x:ident) => {
		mod $x;
		pub use crate::components::$x::*;
	};
}

add_component!(canvas);
add_component!(image);
