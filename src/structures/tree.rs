use std::sync::{Arc, Mutex};

use crate::{
    blocks::{block::Block, block_type::BlockType},
    utils::{ChunkFromPosition, RelativeFromAbsolute},
};

use super::Structure;

pub struct Tree;

impl Structure for Tree {
    fn get_blocks(position: glam::Vec3) -> Vec<Arc<Mutex<Block>>> {
        let trunk_pos = [
            position + glam::vec3(0.0, 1.0, 0.0),
            position + glam::vec3(0.0, 2.0, 0.0),
            position + glam::vec3(0.0, 3.0, 0.0),
        ];

        #[rustfmt::skip]
        let leafs_pos = [
            position + glam::vec3(0.0, 3.0, 1.0),
            position + glam::vec3(0.0, 4.0, 1.0),
            position + glam::vec3(1.0, 3.0, 1.0),
            position + glam::vec3(1.0, 4.0, 1.0),
            position + glam::vec3(-1.0, 3.0, 1.0),
            position + glam::vec3(-1.0, 4.0, 1.0),

            position + glam::vec3(0.0, 3.0, -1.0),
            position + glam::vec3(0.0, 4.0, -1.0),
            position + glam::vec3(1.0, 3.0, -1.0),
            position + glam::vec3(1.0, 4.0, -1.0),
            position + glam::vec3(-1.0, 3.0, -1.0),
            position + glam::vec3(-1.0, 4.0, -1.0),

            position + glam::vec3(1.0, 3.0, 0.0),
            position + glam::vec3(1.0, 4.0, 0.0),
            position + glam::vec3(-1.0, 3.0, 0.0),
            position + glam::vec3(-1.0, 4.0, 0.0),

            position + glam::vec3(0.0, 5.0, 0.0),
        ];

        let blocks = trunk_pos.iter().map(|p| {
            Arc::new(Mutex::new(Block::new(
                p.relative_from_absolute(),
                p.get_chunk_from_position_absolute(),
                BlockType::wood(),
            )))
        });
        let leafs_iter = leafs_pos.iter().map(|p| {
            Arc::new(Mutex::new(Block::new(
                p.relative_from_absolute(),
                p.get_chunk_from_position_absolute(),
                BlockType::leaf(),
            )))
        });
        let blocks = blocks.chain(leafs_iter).collect::<Vec<_>>();
        return blocks;
    }
}
