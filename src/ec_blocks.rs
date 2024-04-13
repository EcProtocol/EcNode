use std::collections::HashMap;

use crate::ec_interface::{Block, BlockId, EcBlocks};

pub struct MemBlocks {
    blocks: HashMap<BlockId, Block>,
}

impl MemBlocks {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }
}

impl EcBlocks for MemBlocks {
    fn lookup(&self, block: &BlockId) -> Option<Block> {
        self.blocks.get(block).map(|b| *b)
    }

    fn save(&mut self, block: &Block) {
        self.blocks.insert(block.id, *block);
    }

    fn remove(&mut self, block: &BlockId) {
        self.blocks.remove(block);
    }
}