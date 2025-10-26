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

        // Format: round peer_id event_type event_details
        let peer_fmt = format!("{:x}", peer & 0xFFFF);

        match event {
            Event::BlockReceived {
                block_id,
                peer: from_peer,
                size,
            } => {
                println!(
                    "{:>5} {:>6} BlockReceived    block:{:x} from:{:x} size:{}",
                    round,
                    peer_fmt,
                    block_id & 0xFFFF,
                    from_peer & 0xFFFF,
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
                    "{:>5} {:>6} VoteCast         block:{:x} token:{:x} vote:{} {}",
                    round,
                    peer_fmt,
                    block_id & 0xFFFF,
                    token & 0xFFFF,
                    vote,
                    if positive { "✓" } else { "✗" }
                );
            }
            Event::BlockCommitted {
                block_id,
                peer: committed_peer,
                votes,
            } => {
                println!(
                    "{:>5} {:>6} BlockCommitted   block:{:x} votes:{}",
                    round,
                    format!("{:x}", committed_peer & 0xFFFF),
                    block_id & 0xFFFF,
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
                    "{:>5} {:>6} Reorg            block:{:x} from:{:x} to:{:x}",
                    round,
                    format!("{:x}", affected_peer & 0xFFFF),
                    block_id & 0xFFFF,
                    from & 0xFFFF,
                    to & 0xFFFF
                );
            }
            Event::BlockNotFound {
                block_id,
                peer: local_peer,
                from_peer,
            } => {
                println!(
                    "{:>5} {:>6} BlockNotFound    block:{:x} from:{:x}",
                    round,
                    format!("{:x}", local_peer & 0xFFFF),
                    block_id & 0xFFFF,
                    from_peer & 0xFFFF
                );
            }
            Event::BlockStateChange {
                block_id,
                from_state,
                to_state,
            } => {
                println!(
                    "{:>5} {:>6} StateChange      block:{:x} {} -> {}",
                    round,
                    peer_fmt,
                    block_id & 0xFFFF,
                    from_state,
                    to_state
                );
            }
        }
    }
}
