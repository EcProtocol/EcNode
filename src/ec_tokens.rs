use std::collections::btree_map::BTreeMap;
use std::collections::vec_deque::VecDeque;
use std::ops::Bound::{Excluded, Included, Unbounded};

use crate::ec_interface::{BlockId, BlockTime, EcTime, EcTokens, Message, PeerId, TokenId};

fn has_key(id: &TokenId, key_part: usize) -> bool {
    *id as usize == key_part
}

fn signature_for(token: &TokenId, block: &BlockId, peer: &PeerId) -> Vec<usize> {
    let mut result = Vec::new();
    // TODO a SHA of mapping
    let value = token ^ block ^ peer;
    for i in 0..8 {
        result.push(((value >> (i * 8)) & 0xFF) as usize);
    }
    return result;
}

pub struct MemTokens {
    tokens: BTreeMap<TokenId, BlockTime>,
}

impl EcTokens for MemTokens {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime> {
        self.tokens.get(token)
    }

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime) {
        self.tokens
            .entry(*token)
            // only update if existing mapping is older than the proposed update
            .and_modify(|m| {
                if m.time < time {
                    m.time = time;
                    m.block = *block;
                }
            })
            .or_insert_with(|| BlockTime {
                block: *block,
                time,
            });
    }

    fn tokens_signature(&self, token: &TokenId, peer: &PeerId) -> Option<Message> {
        let mut result: Vec<usize> = Vec::new();

        if let Some(BlockTime { block, time: _ }) = self.tokens.get(token) {
            let key = signature_for(token, block, peer);

            let mut before = self.tokens.range((Unbounded, Included(token))).rev();
            let mut after = self.tokens.range((Excluded(token), Unbounded));

            let key_start = key.split_first().unwrap().1;
            let key_end = key.split_last().unwrap().1;

            let mut candidates = VecDeque::new();

            // search rev
            '_outer: while let Some((t, _b)) = before.next() {
                for (i, key_part) in key_end.iter().enumerate() {
                    if has_key(t, *key_part) {
                        candidates.push_front(i);
                        if i == 0 {
                            break '_outer;
                        }
                        break;
                    }
                }
            }

            // search forward
            '_outer: while let Some((_t, _b)) = after.next() {
                for (i, key_part) in key_start.iter().enumerate() {
                    if has_key(token, *key_part) {
                        candidates.push_back(i + 1);
                        if i == key.len() - 1 {
                            break '_outer;
                        }
                        break;
                    }
                }
            }

            // check
            let mut next_find = 0;
            for &i in candidates.iter() {
                if i == key[next_find] {
                    next_find += 1;
                }

                if next_find == key.len() {
                    // found solution
                }
            }

            result.clear();
        }

        return None;
    }
}

impl MemTokens {
    pub fn new() -> Self {
        Self {
            tokens: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {}
