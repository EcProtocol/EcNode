//! Various event sinks for different use cases

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use ec_rust::{EcTime, Event, EventSink, PeerId};

// ============================================================================
// Console Logging Sink
// ============================================================================

/// Logging event sink that outputs events to console
pub struct ConsoleEventSink {
    enabled: bool,
}

impl ConsoleEventSink {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

impl EventSink for ConsoleEventSink {
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

// ============================================================================
// CSV Event Sink
// ============================================================================

/// CSV event sink for structured data export
pub struct CsvEventSink {
    writer: BufWriter<File>,
}

impl CsvEventSink {
    pub fn new<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        // Write CSV header
        writeln!(
            writer,
            "round,peer,event_type,block_id,related_peer,value1,value2,details"
        )?;

        Ok(Self { writer })
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

impl EventSink for CsvEventSink {
    fn log(&mut self, round: EcTime, peer: PeerId, event: Event) {
        let result = match event {
            Event::BlockReceived {
                block_id,
                peer: from_peer,
                size,
            } => writeln!(
                self.writer,
                "{},{},BlockReceived,{},{},{},{},size",
                round, peer, block_id, from_peer, size, 0
            ),
            Event::VoteCast {
                block_id,
                token,
                vote,
                positive,
            } => writeln!(
                self.writer,
                "{},{},VoteCast,{},{},{},{},pos={}",
                round, peer, block_id, token, vote, 0, positive
            ),
            Event::BlockCommitted {
                block_id,
                peer: committed_peer,
                votes,
            } => writeln!(
                self.writer,
                "{},{},BlockCommitted,{},{},{},{},votes",
                round, committed_peer, block_id, peer, votes, 0
            ),
            Event::Reorg {
                block_id,
                peer: affected_peer,
                from,
                to,
            } => writeln!(
                self.writer,
                "{},{},Reorg,{},{},{},{},from->to",
                round, affected_peer, block_id, peer, from, to
            ),
            Event::BlockNotFound {
                block_id,
                peer: local_peer,
                from_peer,
            } => writeln!(
                self.writer,
                "{},{},BlockNotFound,{},{},0,0,query_from",
                round, local_peer, block_id, from_peer
            ),
            Event::BlockStateChange {
                block_id,
                from_state,
                to_state,
            } => writeln!(
                self.writer,
                "{},{},StateChange,{},0,0,0,{}->{}",
                round, peer, block_id, from_state, to_state
            ),
        };

        if let Err(e) = result {
            eprintln!("Error writing to CSV: {}", e);
        }
    }
}

impl Drop for CsvEventSink {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}

// ============================================================================
// Collector Event Sink (In-Memory)
// ============================================================================

/// Collects events in memory for programmatic analysis
pub struct CollectorEventSink {
    pub events: Vec<EventRecord>,
}

#[derive(Debug, Clone)]
pub struct EventRecord {
    pub round: EcTime,
    pub peer: PeerId,
    pub event: Event,
}

impl CollectorEventSink {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    // Query helpers
    pub fn commits(&self) -> impl Iterator<Item = &EventRecord> {
        self.events
            .iter()
            .filter(|e| matches!(e.event, Event::BlockCommitted { .. }))
    }

    pub fn reorgs(&self) -> impl Iterator<Item = &EventRecord> {
        self.events
            .iter()
            .filter(|e| matches!(e.event, Event::Reorg { .. }))
    }

    pub fn for_peer(&self, peer_id: PeerId) -> impl Iterator<Item = &EventRecord> {
        self.events.iter().filter(move |e| e.peer == peer_id)
    }

    pub fn in_round_range(&self, start: EcTime, end: EcTime) -> impl Iterator<Item = &EventRecord> {
        self.events
            .iter()
            .filter(move |e| e.round >= start && e.round <= end)
    }

    pub fn count_by_type(&self) -> EventTypeCounts {
        let mut counts = EventTypeCounts::default();
        for record in &self.events {
            match record.event {
                Event::BlockReceived { .. } => counts.block_received += 1,
                Event::VoteCast { .. } => counts.vote_cast += 1,
                Event::BlockCommitted { .. } => counts.block_committed += 1,
                Event::Reorg { .. } => counts.reorg += 1,
                Event::BlockNotFound { .. } => counts.block_not_found += 1,
                Event::BlockStateChange { .. } => counts.state_change += 1,
            }
        }
        counts
    }

    pub fn export_to_csv<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let mut csv_sink = CsvEventSink::new(path)?;
        for record in &self.events {
            csv_sink.log(record.round, record.peer, record.event.clone());
        }
        csv_sink.flush()
    }
}

#[derive(Debug, Default)]
pub struct EventTypeCounts {
    pub block_received: usize,
    pub vote_cast: usize,
    pub block_committed: usize,
    pub reorg: usize,
    pub block_not_found: usize,
    pub state_change: usize,
}

impl EventSink for CollectorEventSink {
    fn log(&mut self, round: EcTime, peer: PeerId, event: Event) {
        self.events.push(EventRecord { round, peer, event });
    }
}

// ============================================================================
// Multi Sink (Combine Multiple Sinks)
// ============================================================================

/// Combines multiple event sinks
pub struct MultiEventSink {
    sinks: Vec<Box<dyn EventSink>>,
}

impl MultiEventSink {
    pub fn new() -> Self {
        Self { sinks: Vec::new() }
    }

    pub fn add_sink(&mut self, sink: Box<dyn EventSink>) {
        self.sinks.push(sink);
    }
}

impl EventSink for MultiEventSink {
    fn log(&mut self, round: EcTime, peer: PeerId, event: Event) {
        for sink in &mut self.sinks {
            sink.log(round, peer, event.clone());
        }
    }
}
