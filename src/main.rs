extern crate core;

use std::cell::RefCell;
use std::cmp::min;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use log::info;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, RngCore, SeedableRng};
use simple_logger::SimpleLogger;

use crate::ec_blocks::MemBlocks;
use crate::ec_interface::{
    Block, BlockId, Message, MessageEnvelope, PeerId, PublicKeyReference, TokenBlock, TokenId,
    TOKENS_PER_BLOCK,
};
use crate::ec_node::EcNode;
use crate::ec_tokens::MemTokens;

mod ec_blocks;
mod ec_interface;
mod ec_mempool;
mod ec_node;
mod ec_peers;
mod ec_tokens;

fn main() {
    SimpleLogger::new().init().unwrap();

    info!("starting");

    let rounds = 5000;
    let num_of_peers = 2000;
    let mut seed = [0u8; 32];
    rand::thread_rng().fill(&mut seed);

    let mut rng = StdRng::from_seed(seed);

    // create starting peers
    let peers: Vec<PeerId> = (0..num_of_peers).map(|_| rng.next_u64()).collect();

    let mut tokens: Vec<(TokenId, BlockId, PublicKeyReference)> = Vec::new();
    for _ in 0..1 {
        tokens.push((rng.next_u64(), 0, 0));
    }

    // make nodes
    let mut nodes: BTreeMap<PeerId, EcNode> = BTreeMap::new();
    for peer_id in &peers {
        let tokens = Rc::new(RefCell::new(MemTokens::new()));
        let blocks = Rc::new(RefCell::new(MemBlocks::new()));

        let mut node = EcNode::new(tokens, blocks, *peer_id, 0);

        // select a random sample for each
        // for add_peer in peers.choose_multiple(&mut rng, 35 * num_of_peers / 100) {
        //     node.seed_peer(add_peer);
        // }
        for p in &peers {
            let d = peer_id.abs_diff(*p);
            let r = rng.next_u64();
            if r > d && d & 3 == 0 {
                node.seed_peer(p);
            }
        }

        nodes.insert(*peer_id, node);
    }

    let mut blocks: BTreeMap<BlockId, PeerId> = BTreeMap::new();

    // iterations
    let mut message_count = 0;
    let mut counters = (0, 0, 0, 0);
    let mut committed = 0;
    let mut messages: Vec<MessageEnvelope> = Vec::new();
    for i in 0..rounds {
        // check for commited
        let mut clear = HashSet::new();

        for (b, p) in &blocks {
            if let Some(block) = nodes.get(&p).unwrap().committed_block(&b) {
                for x in 0..block.used as usize {
                    tokens.push((block.parts[x].token, *b, block.parts[x].key));
                }

                let nodes = nodes
                    .iter()
                    .filter(|(_, n)| n.committed_block(&b).is_some())
                    .count();
                clear.insert(*b);
                info!(
                    "{}: commit B: {} p: {} (on {})",
                    i,
                    b & 0xFF,
                    p & 0xFF,
                    nodes
                );
                committed += 1;
            }
        }

        blocks.retain(|b, _| !clear.contains(b));

        // make new blocks
        tokens.shuffle(&mut rng);

        let mut x = 0;
        while x < tokens.len() {
            let used = min(rng.gen_range(1..4), tokens.len() - x);

            let mut new_block = Block {
                id: rng.next_u64(),
                time: i, // Must be larger than any prev mapping - but trivial here
                used: used as u8,
                parts: [TokenBlock {
                    token: 0,
                    last: 0,
                    key: 0,
                }; TOKENS_PER_BLOCK],
                signatures: [None; TOKENS_PER_BLOCK],
            };

            for (y, (t, b, k)) in tokens[x..(x + used)].iter().enumerate() {
                new_block.parts[y].token = *t;
                new_block.parts[y].last = *b;
                new_block.parts[y].key = rng.next_u64(); // TODO
                new_block.signatures[y] = Some(*k); // TODO
            }

            // choose a target node - to own the transaction
            let target = peers.choose(&mut rng).unwrap();
            let node = nodes.get_mut(target).unwrap();
            node.block(&new_block);
            blocks.insert(new_block.id, *target);

            x += used;
            info!(
                "{} block created B: {} size:{} - p: {}",
                i,
                new_block.id & 0xFF,
                used,
                node.get_peer_id() & 0xFF
            );
        }

        tokens.clear();

        let mut next: Vec<MessageEnvelope> = Vec::new();

        let number_of_messages = messages.len();
        if number_of_messages > 0 {
            messages.shuffle(&mut rng);
            // delay: push a fraction to next
            next.extend_from_slice(&mut messages[(number_of_messages / 2)..number_of_messages]);
            // drop a fraction (network loss)
            messages.truncate(number_of_messages / 2 - number_of_messages / 50);

            //info!("{}: next: {} msgs: {} number_of_messages: {}", i, next.len(), messages.len(), number_of_messages);
        }

        for m in &messages {
            if let Some(node) = nodes.get_mut(&m.receiver) {
                if false
                    && blocks
                        .iter()
                        .any(|p| *p.1 == m.receiver || *p.1 == m.sender)
                {
                    println!(
                        "{}> {}: {} -> {}",
                        i,
                        match m.message {
                            Message::Query { .. } => "Q",
                            Message::Vote { .. } => "S",
                            Message::Block { .. } => "B",
                            _ => "",
                        },
                        m.sender & 0xFF,
                        m.receiver & 0xFF
                    )
                }
                match m.message {
                    Message::Query { .. } => counters.0 += 1,
                    Message::Vote { .. } => counters.1 += 1,
                    Message::Block { .. } => counters.2 += 1,
                    Message::Answer { .. } => counters.3 += 1,
                };
                node.handle_message(&m, &mut next);
            }
        }

        // next round
        for (_, node) in &mut nodes {
            node.tick(&mut next, true); //rng.gen_bool(0.9));
        }

        //info!("{}: next round {} msgs {} blocks - {}", i, next.len(), blocks.len(), committed);

        message_count += messages.len();
        messages.clear();
        messages.append(&mut next);

        /*if messages.len() == 0 {
            info!("{}: next round EMPTY {}", i, committed);
            break;
        }*/
    }

    let stats = nodes
        .iter()
        .map(|(_, node)| node.num_peers())
        .fold((usize::MIN, usize::MAX, usize::MIN), |acc, e| {
            (usize::max(acc.0, e), usize::min(acc.1, e), acc.2 + e)
        });

    info!(
        "Peers ({}): max: {} min: {} avg: {}",
        nodes.len(),
        stats.0,
        stats.1,
        stats.2 / nodes.len()
    );

    info!("let seed = {:?};", seed);
    if committed > 0 {
        info!(
            "done. Messages {}. commit: {},  avg: {} rounds/commit, {} messeage/commit, {:?} dist",
            message_count,
            committed,
            rounds / committed,
            message_count as u64 / committed,
            counters
        );
    } else {
        info!(
            "done. Messages {}. commit: NONE, {:?} dist",
            message_count, counters
        );
    }
}
