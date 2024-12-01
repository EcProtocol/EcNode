use crate::ec_interface::{EcTime, PeerId, TokenId};
use rand::Rng;

struct MemPeer {
    id: PeerId,
    time: EcTime,
    // TODO network and shared secret
}

pub struct EcPeers {
    pub peer_id: PeerId,
    active: Vec<MemPeer>,
}

pub struct PeerRange {
    high: PeerId,
    low: PeerId,
}

impl PeerRange {
    pub fn in_range(&self, key: &TokenId) -> bool {
        if self.low < self.high {
            *key >= self.low && *key <= self.high
        } else {
            // wrapped case (or empty) TokenId == 0 means "no token"
            *key <= self.high || *key >= self.low
        }
    }
}

impl EcPeers {
    fn idx_adj(&self, idx: usize, adj: isize) -> usize {
        let tmp = idx as isize + adj;
        let len = self.active.len() as isize;

        let res = if tmp >= len {
            tmp - len
        } else if tmp < 0 {
            len + tmp
        } else {
            tmp
        };

        if res == len || res < 0 {
            panic!("adj {} {} -> {}", idx, adj, res);
        }

        res as usize
    }

    pub(crate) fn peers_for(&self, key: &TokenId, time: EcTime) -> [PeerId; 2] {
        let idx = match self.active.binary_search_by(|p| p.id.cmp(key)) {
            Ok(i) => i + 1,
            Err(i) => i,
        };

        let adj = (((key ^ self.peer_id) + time) & 0x3) as isize + 1;
        //let start = self.idx_sub(idx, 4);
        // rotating peers
        return [
            self.active.get(self.idx_adj(idx, -adj)).unwrap().id,
            self.active.get(self.idx_adj(idx, adj)).unwrap().id,
        ];
    }

    pub(crate) fn peer_for(&self, key: &TokenId, time: EcTime) -> PeerId {
        let idx = match self.active.binary_search_by(|p| p.id.cmp(key)) {
            Ok(i) => i + 1,
            Err(i) => i,
        };

        let adj = (((key ^ self.peer_id) + time) & 0x3) as isize + 1;
        //let start = self.idx_sub(idx, 4);
        // rotating peers
        return self.active.get(self.idx_adj(idx, -adj)).unwrap().id;
    }

    pub(crate) fn peers_idx_for(&self, key: &TokenId, time: EcTime) -> Vec<usize> {
        let idx = match self.active.binary_search_by(|p| p.id.cmp(key)) {
            Ok(i) => i + 1,
            Err(i) => i,
        };

        let adj = (((key ^ self.peer_id) + time) & 0x3) as isize + 1;
        //let start = self.idx_sub(idx, 4);
        // rotating peers
        return vec![self.idx_adj(idx, -adj), self.idx_adj(idx, adj)];
        //        return vec![self.idx_add(start, offset), self.idx_add(start, offset * 2), self.idx_add(start, offset * 3)];

        /*
        let start = self.idx_sub(idx, 4);
        return (0..8).into_iter().map(|i| self.idx_add(start, i)).choose_multiple(&mut rng, 3);
        // +/- 3 -> let the message packer collapse neighbour indices (effective range +/- 4)
        let from = self.idx_sub(idx, 4);
        let first = rng.gen_range(0..8);
        let second = (first + rng.gen_range(0..7)) % 8;

        [self.idx_add(from, first), self.idx_add(from, second)]
         */
    }

    pub fn for_index(&self, idx: usize) -> Option<PeerId> {
        self.active.get(idx).map(|p| p.id)
    }

    pub(crate) fn update_peer(&mut self, key: &PeerId, time: EcTime) {
        // never store self as a target
        if *key != self.peer_id {
            match self.active.binary_search_by(|p| p.id.cmp(key)) {
                Ok(idx) => {
                    self.active[idx].time = time;
                }
                Err(idx) => {
                    // TODO defer maybe to bulk updating?
                    self.active.insert(idx, MemPeer { time, id: *key });
                }
            }
        }
    }

    pub(crate) fn peer_range(&self, key: &PeerId) -> PeerRange {
        if self.active.len() <= 10 {
            return PeerRange {
                low: PeerId::MIN,
                high: PeerId::MAX,
            };
        }

        match self.active.binary_search_by(|p| p.id.cmp(key)) {
            Ok(idx) | Err(idx) => PeerRange {
                low: self.active[self.idx_adj(idx, -6)].id,
                high: self.active[self.idx_adj(idx, 6)].id,
            },
        }
    }
    pub(crate) fn trusted_peer(&self, key: &PeerId) -> Option<usize> {
        self.active
            .binary_search_by(|p| p.id.cmp(key))
            .map_or(None, |idx| Some(idx))
    }

    pub fn new(peer_id: PeerId) -> Self {
        Self {
            peer_id,
            active: Vec::new(),
        }
    }
    
    pub fn num_peers(&self) -> usize {
        self.active.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_peers() {
        let mut peers = EcPeers::new(1);

        assert!(peers.peers_for(&2).is_empty());

        peers.update_peer(2, 10);

        assert_eq!(*peers.peers_for(&1).first().unwrap(), 2);

        assert!(peers.peers_for(&1).is_empty());
        assert!(peers.peers_for(&1).is_empty());
    }

    #[test]
    fn finding_peers_around_key() {
        let mut peers = EcPeers::new(1);

        for i in 100..120 {
            peers.update_peer(i, 1);
        }

        let finding: Vec<PeerId> = peers.peers_for(&110);

        assert_eq!(finding, [109, 108, 107, 106, 111, 112, 113, 114]);

        let finding: Vec<PeerId> = peers.peers_for(&101);

        assert_eq!(finding, [100, 102, 103, 104, 105]);

        let finding: Vec<PeerId> = peers.peers_for(&120);

        assert_eq!(finding, [119, 118, 117, 116]);
    }
}
