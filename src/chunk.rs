use std::sync::{Arc, Mutex};

use wgpu::util::DeviceExt;

use crate::{
    blocks::{
        block::{Block, BlockFace, BlockVertexData, FaceDirections},
        block_type::BlockType,
    },
    world::{NoiseData, CHUNK_HEIGHT, CHUNK_SIZE, NOISE_CHUNK_PER_ROW, NOISE_SIZE},
};

pub type BlockVec = Vec<Vec<Arc<Mutex<Block>>>>;
pub struct Chunk {
    // probably there needs to be a cube type with more info ( regarding type, etc. )
    pub x: i32,
    pub y: i32,
    pub blocks: BlockVec,
    pub indices: u32,
    pub chunk_bind_group: wgpu::BindGroup,
    pub chunk_position_buffer: wgpu::Buffer,
    // pub chunk_vertex_buffer: wgpu::Buffer,
    pub chunk_index_buffer: wgpu::Buffer,
    pub chunk_vertex_buffer: wgpu::Buffer,
}

impl Chunk {
    pub fn exists_block_at(blocks: &BlockVec, position: &glam::Vec3) -> bool {
        if let Some(y_blocks) =
            blocks.get(((position.x as u32 * CHUNK_SIZE) + position.z as u32) as usize)
        {
            if let Some(_) = y_blocks.get(position.y as usize) {
                return true;
            } else {
                return false;
            }
        } else {
            return false;
        };
    }
    pub fn get_block_at_relative(&self, position: &glam::Vec3) -> Option<Arc<Mutex<Block>>> {
        if let Some(y_blocks) = self
            .blocks
            .get(((position.x * CHUNK_SIZE as f32) + position.z) as usize)
        {
            if let Some(block) = y_blocks.get(position.y as usize) {
                return Some(Arc::clone(block));
            }
        }
        return None;
    }
    pub fn is_outside_chunk(position: &glam::Vec3) -> bool {
        if position.x < 0.0
            || position.x >= CHUNK_SIZE as f32
            || position.z < 0.0
            || position.z >= CHUNK_SIZE as f32
        {
            true
        } else {
            false
        }
    }
    pub fn is_outside_bounds(position: &glam::Vec3) -> bool {
        if position.y < 0.0 {
            true
        } else {
            false
        }
    }
    // Returns the number of indices added to the chunk - it would've been better to be a mutable method but i can't do it because of borrow checker
    pub fn build_mesh(
        chunk_x: i32,
        chunk_y: i32,
        blocks: &BlockVec,
        noise_data: Arc<NoiseData>,
        device: Arc<wgpu::Device>,
    ) -> (u32, wgpu::Buffer, wgpu::Buffer) {
        let mut vertex: Vec<BlockVertexData> = vec![];
        let mut indices: Vec<u32> = vec![];
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let region = &blocks[(x * CHUNK_SIZE + z) as usize];
                for y in 0..region.len() {
                    let block = &region[y];
                    let block = block.lock().unwrap();
                    let position = block.position;
                    let faces = block.faces.as_ref().unwrap();

                    for face in faces.iter() {
                        let mut is_visible = true;
                        let face_position = face.face_direction.get_normal_vector() + position;

                        if Chunk::is_outside_bounds(&face_position) {
                            is_visible = false;
                        } else if Chunk::is_outside_chunk(&face_position) {
                            let target_chunk_x =
                                chunk_x + (f32::floor(face_position.x / CHUNK_SIZE as f32) as i32);
                            let target_chunk_y =
                                chunk_y + (f32::floor(face_position.z / CHUNK_SIZE as f32) as i32);

                            let target_block = glam::vec3(
                                (face_position.x + CHUNK_SIZE as f32) % CHUNK_SIZE as f32,
                                face_position.y,
                                (face_position.z + CHUNK_SIZE as f32) % CHUNK_SIZE as f32,
                            );

                            // This probably needs to be looked at again when the blocks can be placed/destroyed
                            if face_position.y as u32
                                <= Chunk::get_height_value(
                                    target_chunk_x,
                                    target_chunk_y,
                                    target_block.x as u32,
                                    target_block.z as u32,
                                    noise_data.clone(),
                                )
                            {
                                is_visible = false
                            };
                        } else if Chunk::exists_block_at(&blocks, &face_position) {
                            is_visible = false;
                        }

                        if is_visible {
                            let (mut vertex_data, index_data) = face.create_face_data(&block);
                            vertex.append(&mut vertex_data);
                            let indices_offset = vertex.len() as u32 - 4;
                            indices.append(
                                &mut index_data.iter().map(|i| i + indices_offset).collect(),
                            )
                        }
                    }
                }
            }
        }

        let chunk_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&vertex),
            label: Some(&format!("chunk-vertex-{chunk_x}-{chunk_y}")),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let chunk_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&indices),
            label: Some(&format!("chunk-index-{chunk_x}-{chunk_y}")),
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });
        return (
            indices.len() as u32,
            chunk_vertex_buffer,
            chunk_index_buffer,
        );
    }
    pub fn get_bind_group_layout() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: Some("chunk_bind_group"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        }
    }
    pub fn get_height_value(
        chunk_x: i32,
        chunk_y: i32,
        x: u32,
        z: u32,
        noise_data: Arc<NoiseData>,
    ) -> u32 {
        let mut x = (chunk_x * CHUNK_SIZE as i32) + x as i32 % NOISE_SIZE as i32;
        let mut z = (chunk_y * CHUNK_SIZE as i32) + z as i32 % NOISE_SIZE as i32;

        if x < 0 {
            x = NOISE_SIZE as i32 + (x % (NOISE_CHUNK_PER_ROW * CHUNK_SIZE) as i32);
        }
        if z < 0 {
            z = NOISE_SIZE as i32 + (z % (NOISE_CHUNK_PER_ROW * CHUNK_SIZE) as i32);
        }

        let y_top = (noise_data[((z * NOISE_SIZE as i32) + x) as usize] + 1.0) * 0.5;
        return (f32::powf(100.0, y_top) - 1.0) as u32;
    }

    pub fn create_blocks_data(chunk_x: i32, chunk_y: i32, noise_data: Arc<NoiseData>) -> BlockVec {
        let size = (CHUNK_SIZE * CHUNK_SIZE) as usize;
        let mut blocks: BlockVec = Vec::with_capacity(size);

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                blocks.push(vec![]);

                let y_top = Chunk::get_height_value(chunk_x, chunk_y, x, z, noise_data.clone());

                for y in 0..=y_top {
                    let block_type = match BlockType::from_y_position(y) {
                        BlockType::Dirt(..) if y == y_top => BlockType::grass(),
                        b => b,
                    };

                    let block = Arc::new(Mutex::new(Block {
                        faces: None,
                        position: glam::vec3(x as f32, y as f32, z as f32),
                        absolute_position: glam::vec3(
                            (chunk_x * CHUNK_SIZE as i32 + x as i32) as f32,
                            y as f32,
                            (chunk_y * CHUNK_SIZE as i32 + z as i32) as f32,
                        ),
                        block_type,
                        is_translucent: false,
                    }));

                    let face_directions = FaceDirections::all()
                        .iter()
                        .map(|face_dir| BlockFace {
                            block: Arc::downgrade(&block),
                            face_direction: *face_dir,
                        })
                        .collect::<Vec<_>>();

                    block.lock().unwrap().faces = Some(face_directions);
                    let curr = &mut blocks[((x * CHUNK_SIZE) + z) as usize];
                    curr.push(block.clone());
                }
            }
        }

        blocks
    }

    pub fn new(
        x: i32,
        y: i32,
        noise_data: Arc<NoiseData>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        chunk_data_layout: Arc<wgpu::BindGroupLayout>,
    ) -> Self {
        let blocks = Self::create_blocks_data(x, y, noise_data.clone());
        let (indices, chunk_vertex_buffer, chunk_index_buffer) =
            Self::build_mesh(x, y, &blocks, noise_data.clone(), device.clone());

        let chunk_position_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&[x, y]),
            label: Some(&format!("chunk-position-{x}-{y}")),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let chunk_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: chunk_data_layout.as_ref(),
            label: Some(&format!("chunk-bg-{x}-{y}")),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: chunk_position_buffer.as_entire_binding(),
            }],
        });

        return Chunk {
            x,
            y,
            blocks,
            chunk_bind_group,
            chunk_index_buffer,
            chunk_position_buffer,
            chunk_vertex_buffer,
            indices,
        };
    }
}
