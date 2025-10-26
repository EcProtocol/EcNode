//! Event logging for simulator

use ec_rust::{EcTime, Event, EventSink, PeerId};

/// Logging event sink that outputs events to console
pub struct LoggingEventSink {
    enabled: bool,
}

impl LoggingEventSink {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

impl EventSink for LoggingEventSink {
    fn log(&mut self, round: EcTime, peer: PeerId, event: Event) {
        if !self.enabled {
            return;
        }

        match event {
            Event::BlockReceived {
                block_id,
                peer: from_peer,
                size,
            } => {
                println!(
                    "{} recv: p:{} b:{} from:{} size:{}",
                    round,
                    peer & 0xFFF,
                    block_id & 0xFF,
                    from_peer & 0xFFF,
                    size
                );
            }
            Event::VoteCast {
                block_id,
                token,
                vote,
                positive,
            } => {
                println!(
                    "{} vote: p:{} b:{} t:{} v:{} pos:{}",
                    round,
                    peer & 0xFFF,
                    block_id & 0xFF,
                    token & 0xFF,
                    vote,
                    positive
                );
            }
            Event::BlockCommitted {
                block_id,
                peer: committed_peer,
                votes,
            } => {
                println!(
                    "{} cmt: p:{} b:{} votes:{}",
                    round,
                    committed_peer & 0xFFF,
                    block_id & 0xFF,
                    votes
                );
            }
            Event::Reorg {
                block_id,
                peer: affected_peer,
                from,
                to,
            } => {
                println!(
                    "{} reorg b:{} p:{}: {} <-> {}",
                    round,
                    block_id & 0xFF,
                    affected_peer & 0xFFF,
                    from & 0xFF,
                    to & 0xFF
                );
            }
            Event::BlockNotFound {
                block_id,
                peer: local_peer,
                from_peer,
            } => {
                println!(
                    "{} not-found p:{} b:{} (from:{})",
                    round,
                    local_peer & 0xFFF,
                    block_id & 0xFF,
                    from_peer & 0xFFF
                );
            }
            Event::BlockStateChange {
                block_id,
                from_state,
                to_state,
            } => {
                println!(
                    "{} state: p:{} b:{} {} -> {}",
                    round,
                    peer & 0xFFF,
                    block_id & 0xFF,
                    from_state,
                    to_state
                );
            }
        }
    }
}
