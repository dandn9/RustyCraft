use std::sync::{Mutex, RwLock};
use std::{f32::consts, sync::Arc};

use crate::blocks::block::Block;
use crate::blocks::block_type::BlockType;
use crate::collision::CollisionBox;
use crate::persistence::Saveable;
use crate::pipeline::{Pipeline, PipelineTrait};
use crate::utils::{ChunkFromPosition, RelativeFromAbsolute};
use crate::{
    material::Texture,
    pipeline::{self, Uniforms},
    player::{Camera, CameraController, Player},
    ui::{UIPipeline, UI},
    world::World,
};
use winit::event::MouseButton;
use winit::window::CursorGrabMode;
use winit::{
    dpi::PhysicalSize,
    event::KeyEvent,
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

impl State {
    pub async fn new(window: Arc<Mutex<Window>>) -> Self {
        let windowbrw = window.lock().unwrap();
        let size = windowbrw.inner_size();
        let instance = wgpu::Instance::default();
        let surface = unsafe { instance.create_surface(&*windowbrw).unwrap() };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        // Create the logical device and command queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::POLYGON_MODE_LINE,
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
        };

        let camera = Camera {
            aspect_ratio: surface_config.width as f32 / surface_config.height as f32,
            eye: glam::vec3(-4.0, 50.0, 4.0),
            yaw: consts::FRAC_PI_2,
            pitch: 0.0,

            fovy: consts::FRAC_PI_4,
            znear: 0.1,
            zfar: 1000.,
            needs_update: false,
        };
        let player = Player {
            camera,
            current_chunk: (0, 0),
            is_jumping: false,
            on_ground: false,
            facing_block: None,
            facing_face: None,
            jump_action_start: None,
            is_ghost: false,
        };

        surface.configure(&device, &surface_config);
        let config = Config {
            polygon_mode: wgpu::PolygonMode::Fill,
        };

        let mut world = World::init_world(device.clone(), queue.clone());
        world.init_chunks();
        let ui = UI::new(device.clone(), queue.clone());

        let mut state = Self {
            config,
            player,
            ui,
            pipelines: vec![],
            surface_config,
            instance,
            window: window.clone(),
            device,
            world,
            queue,
            surface,
            adapter,
            camera_controller: CameraController::default(),
        };

        let world_pipeline = Box::new(Pipeline::new(&state));
        let ui_pipeline = Box::new(UIPipeline::new(&state));

        state.pipelines.push(world_pipeline);
        state.pipelines.push(ui_pipeline);

        state
    }
    pub fn save_state(&mut self) {
        self.player
            .camera
            .save()
            .expect("Failed to save camera state");
        self.world.save_state();
    }
    pub fn dispose(&mut self) {
        self.world.dispose();
    }
    pub fn handle_keypress(&mut self, event: KeyEvent, delta_time: f32) {
        let is_pressed: f32 = if event.state.is_pressed() { 1. } else { 0. };

        match event {
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyW),
                ..
            } => self.camera_controller.movement_vector.z = 1.0 * is_pressed,
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyS),
                ..
            } => self.camera_controller.movement_vector.z = -1.0 * is_pressed,
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyA),
                ..
            } => self.camera_controller.movement_vector.x = -1.0 * is_pressed,
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyD),
                ..
            } => self.camera_controller.movement_vector.x = 1.0 * is_pressed,
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyE),
                ..
            } => self.camera_controller.movement_vector.y = 1.0 * is_pressed,
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyQ),
                ..
            } => self.camera_controller.movement_vector.y = -1.0 * is_pressed,
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyK),
                ..
            } => self
                .window
                .lock()
                .unwrap()
                .set_cursor_grab(CursorGrabMode::Confined)
                .unwrap(),
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::Space),
                state: winit::event::ElementState::Pressed,
                ..
            } => {
                if self.player.on_ground {
                    self.player.is_jumping = true;
                    self.player.jump_action_start = Some(std::time::Instant::now());
                }
            }
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                state: winit::event::ElementState::Pressed,
                ..
            } => {
                self.player.is_ghost = !self.player.is_ghost;
            }
            KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyF),
                state: winit::event::ElementState::Pressed,
                ..
            } => {
                if self.config.polygon_mode == wgpu::PolygonMode::Line {
                    self.config.polygon_mode = wgpu::PolygonMode::Fill
                } else {
                    self.config.polygon_mode = wgpu::PolygonMode::Line
                }
            }
            _ => {}
        }
    }
    pub fn on_click(&mut self, button: MouseButton) {
        if let Some(facing_block) = self.player.facing_block.as_ref() {
            let facing_face = self
                .player
                .facing_face
                .expect("Cannot be not facing a face if it's facing a block");
            match button {
                MouseButton::Left => {
                    self.world.remove_block(facing_block.clone());
                }
                MouseButton::Right => {
                    let block_borrow = facing_block.read().unwrap();
                    let new_block_abs_position =
                        block_borrow.absolute_position + facing_face.get_normal_vector();

                    let chunk = new_block_abs_position.get_chunk_from_position_absolute();
                    let position = new_block_abs_position.relative_from_absolute();

                    let new_block =
                        Arc::new(RwLock::new(Block::new(position, chunk, BlockType::dirt())));

                    self.world.place_block(new_block);
                }
                _ => {}
            }
        }
    }
    pub fn handle_mouse(&mut self, delta: &glam::Vec2) {
        self.player.camera.move_target(delta)
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.surface_config.width = new_size.width.max(1);
            self.surface_config.height = new_size.height.max(1);
            self.surface.configure(&self.device, &self.surface_config);
            let new_depth = Texture::create_depth_texture(&self);
            self.pipelines[0].set_depth_texture(new_depth);
        }
    }
    pub fn update(&mut self, delta_time: f32, total_time: f32) {
        let mut collisions = vec![];
        if let Some(nearby_blocks) = self.world.get_blocks_nearby(&self.player) {
            for block in nearby_blocks.iter() {
                let block = block.read().unwrap();
                let collision = CollisionBox::from_block_position(
                    block.absolute_position.x,
                    block.position.y,
                    block.absolute_position.z,
                );
                collisions.push(collision);
            }
        };
        self.player.move_camera(
            &self.camera_controller.movement_vector,
            delta_time,
            &collisions,
        );
        if let Some((block, face_dir)) = self.player.get_facing_block(&collisions) {
            let block = self.world.get_blocks_absolute(&block.to_block_position());

            self.player.facing_block = block;
            self.player.facing_face = Some(face_dir);
        } else {
            self.player.facing_block = None;
            self.player.facing_face = None;
        }

        let uniforms = Uniforms::from(&self.player.camera);

        for pipeline in self.pipelines.iter() {
            self.queue.write_buffer(
                pipeline.view_buffer(),
                0,
                bytemuck::cast_slice(&[uniforms.view]),
            )
        }

        self.world.update(
            &mut self.player,
            Arc::clone(&self.queue),
            Arc::clone(&self.device),
        );
        self.ui.update(
            &mut self.player,
            Arc::clone(&self.queue),
            Arc::clone(&self.device),
        );
    }
    pub fn draw(&mut self) {
        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swapchain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("command_encoder"),
            });

        let chunks = self
            .world
            .chunks
            .iter()
            .map(|f| f.read().unwrap())
            .collect::<Vec<_>>();

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.pipelines[0].depth_texture().view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let pipeline = &self.pipelines[0];

            rpass.set_pipeline(pipeline.pipeline());

            rpass.set_bind_group(0, pipeline.bind_group_0(), &[]);
            rpass.set_bind_group(1, pipeline.bind_group_1(), &[]);

            for chunk in chunks.iter() {
                rpass.set_bind_group(2, &chunk.chunk_bind_group, &[]);
                rpass.set_vertex_buffer(
                    0,
                    chunk
                        .chunk_vertex_buffer
                        .as_ref()
                        .expect("Vertex buffer not initiated")
                        .slice(..),
                );
                rpass.set_index_buffer(
                    chunk
                        .chunk_index_buffer
                        .as_ref()
                        .expect("Index buffer not initiated")
                        .slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                rpass.draw_indexed(0..chunk.indices, 0, 0..1);
            }
        }
        {
            let mut ui_renderpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.pipelines[0].depth_texture().view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let pipeline = &self.pipelines[1];
            ui_renderpass.set_pipeline(pipeline.pipeline());

            ui_renderpass.set_bind_group(0, pipeline.bind_group_0(), &[]);
            ui_renderpass.set_bind_group(1, pipeline.bind_group_1(), &[]);

            ui_renderpass.set_vertex_buffer(0, self.ui.vertex_buffer.slice(..));
            ui_renderpass
                .set_index_buffer(self.ui.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            ui_renderpass.draw_indexed(0..self.ui.indices as u32, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
    }
}

pub struct Config {
    pub polygon_mode: wgpu::PolygonMode,
}

pub struct State {
    pub surface: wgpu::Surface,
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub window: Arc<Mutex<Window>>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub pipelines: Vec<Box<dyn PipelineTrait>>,
    pub player: Player,
    pub world: World,
    pub ui: UI,
    pub config: Config,
    pub camera_controller: CameraController,
    // pub model: Rc<RefCell<Model>>,
}
