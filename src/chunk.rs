use crate::persistence::{Loadable, Saveable};
use crate::world::WorldChunk;
use crate::{
    blocks::{
        block::{Block, BlockVertexData, FaceDirections},
        block_type::BlockType,
    },
    structures::Structure,
    world::{NoiseData, CHUNK_SIZE, MAX_TREES_PER_CHUNK, NOISE_CHUNK_PER_ROW, NOISE_SIZE},
};
use glam::Vec3;
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use wgpu::util::DeviceExt;

pub type BlockVec = Arc<RwLock<Vec<Vec<Option<Arc<RwLock<Block>>>>>>>;

#[derive(Debug)]
pub struct Chunk {
    pub x: i32,
    pub y: i32,
    pub blocks: BlockVec,
    pub indices: u32,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub noise_data: Arc<NoiseData>,
    pub chunk_bind_group: wgpu::BindGroup,
    pub chunk_position_buffer: wgpu::Buffer,
    pub chunk_index_buffer: Option<wgpu::Buffer>,
    pub chunk_vertex_buffer: Option<wgpu::Buffer>,
    pub outside_blocks: Vec<Arc<RwLock<Block>>>,
}

impl Chunk {
    pub fn add_block(&mut self, block: Arc<RwLock<Block>>) {
        let block_borrow = block.read().unwrap();
        let block_position = block_borrow.position;
        std::mem::drop(block_borrow);
        let mut blocks_borrow = self.blocks.write().unwrap();

        let y_blocks = blocks_borrow
            .get_mut(((block_position.x * CHUNK_SIZE as f32) + block_position.z) as usize)
            .expect("Cannot add oob block");

        let start_len = y_blocks.len();

        /* Make sure we don't have enough space in the vector */
        for i in start_len..=block_position.y as usize {
            if i >= y_blocks.len() {
                y_blocks.push(None);
            }
        }

        y_blocks[block_position.y as usize] = Some(block);
    }
    pub fn remove_block(&mut self, block_r_position: &Vec3) {
        let mut blocks_borrow = self.blocks.write().unwrap();
        let y_blocks = blocks_borrow
            .get_mut(((block_r_position.x * CHUNK_SIZE as f32) + block_r_position.z) as usize)
            .expect("Cannot delete oob block");
        y_blocks[block_r_position.y as usize] = None;
    }
    pub fn exists_block_at(&self, position: &glam::Vec3) -> bool {
        if let Some(y_blocks) = self
            .blocks
            .read()
            .unwrap()
            .get(((position.x as u32 * CHUNK_SIZE) + position.z as u32) as usize)
        {
            if let Some(block_opt) = y_blocks.get(position.y as usize) {
                if let Some(_) = block_opt {
                    return true;
                }
            }
        }
        return false;
    }
    pub fn get_block_at_relative(&self, position: &glam::Vec3) -> Option<Arc<RwLock<Block>>> {
        if let Some(y_blocks) = self
            .blocks
            .read()
            .unwrap()
            .get(((position.x * CHUNK_SIZE as f32) + position.z) as usize)
        {
            if let Some(block) = y_blocks.get(position.y as usize)? {
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
    pub fn build_mesh(&self, other_chunks: Vec<WorldChunk>) -> (u32, wgpu::Buffer, wgpu::Buffer) {
        let mut vertex: Vec<BlockVertexData> = vec![];
        let mut indices: Vec<u32> = vec![];
        let mut adjacent_chunks: Vec<((i32, i32), BlockVec)> = vec![((self.x, self.y), self.blocks.clone())];

        for chunk in &other_chunks {
            let chunk_read = chunk.read().unwrap();
            if chunk_read.x == self.x + 1 || chunk_read.x == self.x - 1 {
                adjacent_chunks.push(((chunk_read.x, chunk_read.y), chunk_read.blocks.clone()));
            } else if chunk_read.y == self.y + 1 || chunk_read.y == self.y - 1 {
                adjacent_chunks.push(((chunk_read.x, chunk_read.y), chunk_read.blocks.clone()));
            }
        }

        for region in self.blocks.read().unwrap().iter() {
            for y in 0..region.len() {
                if let Some(block_ptr) = &region[y] {
                    let block = block_ptr.read().unwrap();
                    let position = block.position;
                    let faces = FaceDirections::all();

                    for face in faces.iter() {
                        let mut is_visible = true;
                        let face_position = face.get_normal_vector() + position;

                        if Chunk::is_outside_bounds(&face_position) {
                            is_visible = false;
                        } else if Chunk::is_outside_chunk(&face_position) {
                            let target_chunk_x =
                                self.x + (f32::floor(face_position.x / CHUNK_SIZE as f32) as i32);
                            let target_chunk_y =
                                self.y + (f32::floor(face_position.z / CHUNK_SIZE as f32) as i32);

                            let target_block = glam::vec3(
                                (face_position.x + CHUNK_SIZE as f32) % CHUNK_SIZE as f32,
                                face_position.y,
                                (face_position.z + CHUNK_SIZE as f32) % CHUNK_SIZE as f32,
                            );

                            let target_chunk = other_chunks.iter().find(|c| {
                                let c = c.read().unwrap();
                                c.x == target_chunk_x && c.y == target_chunk_y
                            });
                            // If there's a chunk loaded in memory then check that, else it means we're on a edge and we can
                            // Calculate the block's height when the chunk gets generated
                            // TODO: Check for saved file chunk
                            match target_chunk {
                                Some(chunk) => {
                                    let chunk = chunk.read().unwrap();
                                    if chunk.exists_block_at(&target_block) {
                                        is_visible = false;
                                    }
                                }
                                None => {
                                    if face_position.y as u32
                                        <= Chunk::get_height_value(
                                            target_chunk_x,
                                            target_chunk_y,
                                            target_block.x as u32,
                                            target_block.z as u32,
                                            self.noise_data.clone(),
                                        )
                                    {
                                        is_visible = false
                                    };
                                }
                            }
                        } else if self.exists_block_at(&face_position) {
                            is_visible = false;
                        }

                        if is_visible {
                            let (mut vertex_data, index_data) =
                                face.create_face_data(block_ptr.clone(), &adjacent_chunks);
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

        let chunk_vertex_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    contents: bytemuck::cast_slice(&vertex),
                    label: Some(&format!("chunk-vertex-{}-{}", self.x, self.y)),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
        let chunk_index_buffer =
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    contents: bytemuck::cast_slice(&indices),
                    label: Some(&format!("chunk-vertex-{}-{}", self.x, self.y)),
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                });

        (
            indices.len() as u32,
            chunk_vertex_buffer,
            chunk_index_buffer,
        )
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
        let mut blocks: BlockVec = Arc::new(RwLock::new(vec![vec![]; size]));

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let y_top = Chunk::get_height_value(chunk_x, chunk_y, x, z, noise_data.clone());

                for y in 0..=y_top {
                    let block_type = match BlockType::from_y_position(y) {
                        BlockType::Dirt(..) if y == y_top => BlockType::grass(),
                        b => b,
                    };

                    let block = Arc::new(RwLock::new(Block::new(
                        glam::vec3(x as f32, y as f32, z as f32),
                        (chunk_x, chunk_y),
                        block_type,
                    )));

                    let curr = &mut blocks.write().unwrap()[((x * CHUNK_SIZE) + z) as usize];
                    curr.push(Some(block.clone()));
                }
            }
        }

        blocks
    }
    pub fn place_trees(&mut self) {
        let number_of_trees = rand::random::<f32>();
        let number_of_trees = f32::floor(number_of_trees * MAX_TREES_PER_CHUNK as f32) as u32;

        for _ in 0..number_of_trees {
            let x = f32::floor(rand::random::<f32>() * CHUNK_SIZE as f32) as usize;
            let z = f32::floor(rand::random::<f32>() * CHUNK_SIZE as f32) as usize;

            let blocks_read = self.blocks.read().unwrap();
            let block_column = blocks_read
                .get((x * CHUNK_SIZE as usize) + z)
                .expect("TODO: fix this case");
            let highest_block = block_column
                .last()
                .expect("TODO: Fix this case -h")
                .as_ref()
                .unwrap()
                .read()
                .unwrap()
                .absolute_position;
            std::mem::drop(blocks_read);

            let tree_blocks = crate::structures::Tree::get_blocks(highest_block);

            for block in tree_blocks.iter() {
                let block_brw = block.read().unwrap();
                let block_chunk = block_brw.get_chunk_coords();
                if block_chunk == (self.x, self.y) {
                    self.add_block(block.clone());
                } else {
                    self.outside_blocks.push(block.clone())
                }
            }
        }
    }

    pub fn new(
        x: i32,
        y: i32,
        noise_data: Arc<NoiseData>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        chunk_data_layout: Arc<wgpu::BindGroupLayout>,
    ) -> Chunk {
        let mut was_loaded = false;
        let blocks = if let Ok(blocks) = Self::load(Box::new((x, y))) {
            was_loaded = true;
            blocks
        } else {
            Self::create_blocks_data(x, y, noise_data.clone())
        };

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

        let mut chunk = Chunk {
            blocks,
            x,
            y,
            device,
            queue,
            noise_data,
            chunk_vertex_buffer: None,
            chunk_index_buffer: None,
            chunk_bind_group,
            chunk_position_buffer,
            indices: 0,
            outside_blocks: vec![],
        };

        if !was_loaded {
            chunk.place_trees();
        }
        return chunk;
    }
}

impl Saveable<Chunk> for Chunk {
    fn save(&self) -> Result<(), Box<dyn Error>> {
        if let Ok(_) = std::fs::create_dir("data") {
            println!("Created dir");
        }
        let mut data = String::new();

        for col in self.blocks.read().unwrap().iter() {
            for block in col.iter() {
                if let Some(block_ptr) = block {
                    let blockbrw = block_ptr.read().unwrap();
                    data += &format!(
                        "{},{},{},{}\n",
                        blockbrw.position.x,
                        blockbrw.position.y,
                        blockbrw.position.z,
                        blockbrw.block_type.to_id()
                    );
                }
            }
        }

        let chunk_file_name = format!("data/chunk{}_{}", self.x, self.y);
        std::fs::write(chunk_file_name.clone(), data.as_bytes())?;

        Ok(())
    }
}

impl Loadable<BlockVec> for Chunk {
    fn load(args: Box<dyn Any>) -> Result<BlockVec, Box<dyn Error>> {
        if let Ok(chunk_position) = args.downcast::<(i32, i32)>() {
            for entry in std::fs::read_dir("data")? {
                let file = entry?;
                let filename_chunk = file.file_name();
                let mut coords = filename_chunk
                    .to_str()
                    .unwrap()
                    .split("k")
                    .last()
                    .expect("Invalid filename")
                    .split("_");
                let x = coords.next().unwrap().parse::<i32>()?;
                let y = coords.next().unwrap().parse::<i32>()?;

                let size = (CHUNK_SIZE * CHUNK_SIZE) as usize;
                let mut blocks: BlockVec = Arc::new(RwLock::new(vec![vec![]; size]));
                if *chunk_position == (x, y) {
                    let file_contents = std::fs::read_to_string(format!("data/chunk{}_{}", x, y))?;
                    for line in file_contents.lines() {
                        let mut i = line.split(",");
                        let bx = i.next().unwrap().parse::<u32>()?;
                        let by = i.next().unwrap().parse::<u32>()?;
                        let bz = i.next().unwrap().parse::<u32>()?;
                        let block_type = i.next().unwrap().parse::<u32>()?;
                        let block_type = BlockType::from_id(block_type);

                        let block = Block::new(
                            glam::vec3(bx as f32, by as f32, bz as f32),
                            (x, y),
                            block_type,
                        );
                        let y_blocks =
                            &mut blocks.write().unwrap()[((bx * CHUNK_SIZE) + bz) as usize];
                        let start_len = y_blocks.len();

                        for i in start_len..=by as usize {
                            if i >= y_blocks.len() {
                                y_blocks.push(None);
                            }
                        }
                        y_blocks[by as usize] = Some(Arc::new(RwLock::new(block)));
                    }
                    return Ok(blocks);
                }
            }
        }
        return Err("Not valid args".into());
    }
}
