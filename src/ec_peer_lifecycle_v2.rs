use crate::ec_interface::{EcTime, PeerId, TokenId, TokenMapping, TOKENS_SIGNATURE_SIZE};

const RING_SIZE: u128 = u64::MAX as u128 + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LifecyclePeerState {
    Identified,
    Pending,
    Connected,
}

impl LifecyclePeerState {
    fn is_at_least(self, minimum: LifecyclePeerState) -> bool {
        self >= minimum
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerEntry {
    pub peer_id: PeerId,
    pub state: LifecyclePeerState,
    pub last_heard_from: Option<EcTime>,
}

impl PeerEntry {
    pub fn new(peer_id: PeerId, state: LifecyclePeerState) -> Self {
        Self {
            peer_id,
            state,
            last_heard_from: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerSpectrum {
    local_peer_id: PeerId,
    peers: Vec<PeerEntry>,
}

impl PeerSpectrum {
    pub fn new(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            peers: Vec::new(),
        }
    }

    pub fn from_entries(
        local_peer_id: PeerId,
        entries: impl IntoIterator<Item = PeerEntry>,
    ) -> Self {
        let mut spectrum = Self::new(local_peer_id);
        for entry in entries {
            spectrum.upsert_entry(entry);
        }
        spectrum
    }

    pub fn upsert(&mut self, peer_id: PeerId, state: LifecyclePeerState) {
        self.upsert_entry(PeerEntry::new(peer_id, state));
    }

    pub fn record_signal(
        &mut self,
        peer_id: PeerId,
        state: LifecyclePeerState,
        time: EcTime,
    ) -> bool {
        if peer_id == self.local_peer_id {
            return false;
        }

        let mut entry = self
            .get(peer_id)
            .unwrap_or_else(|| PeerEntry::new(peer_id, state));
        entry.state = state;
        entry.last_heard_from = Some(time);
        self.upsert_entry(entry);
        true
    }

    pub fn remove(&mut self, peer_id: PeerId) -> Option<PeerEntry> {
        let index = self
            .peers
            .binary_search_by_key(&peer_id, |peer| peer.peer_id)
            .ok()?;
        Some(self.peers.remove(index))
    }

    pub fn get(&self, peer_id: PeerId) -> Option<PeerEntry> {
        let index = self
            .peers
            .binary_search_by_key(&peer_id, |peer| peer.peer_id)
            .ok()?;
        Some(self.peers[index])
    }

    pub fn peers_around(
        &self,
        target: TokenId,
        count: usize,
        minimum_state: LifecyclePeerState,
    ) -> Vec<PeerId> {
        if self.peers.is_empty() || count == 0 {
            return Vec::new();
        }

        let idx = self.insertion_index(target);
        let mut selected = Vec::with_capacity(count);

        for step in 0..self.peers.len() {
            let left = self.peers[idx_adj(self.peers.len(), idx, -((step as isize) + 1))];
            let right = self.peers[idx_adj(self.peers.len(), idx, step as isize)];

            if ring_distance(left.peer_id, target) <= ring_distance(right.peer_id, target) {
                push_matching(&mut selected, left, minimum_state, count);
                push_matching(&mut selected, right, minimum_state, count);
            } else {
                push_matching(&mut selected, right, minimum_state, count);
                push_matching(&mut selected, left, minimum_state, count);
            }

            if selected.len() == count {
                break;
            }
        }

        selected
    }

    pub fn span_around(
        &self,
        target: TokenId,
        width: usize,
        minimum_state: LifecyclePeerState,
    ) -> Option<RingSpan> {
        if self.peers.is_empty() {
            return None;
        }

        let width = width.max(1);
        let idx = self.insertion_index(target);
        let target_is_peer = idx < self.peers.len() && self.peers[idx].peer_id == target;
        let mut low = None;
        let mut high = None;
        let mut low_seen = 0;
        let mut high_seen = 0;

        for step in 0..self.peers.len() {
            let left = self.peers[idx_adj(self.peers.len(), idx, -((step as isize) + 1))];
            if left.state.is_at_least(minimum_state) {
                low_seen += 1;
                if low_seen == width {
                    low = Some(left.peer_id);
                }
            }

            let right_index = if target_is_peer {
                idx_adj(self.peers.len(), idx, (step as isize) + 1)
            } else {
                idx_adj(self.peers.len(), idx, step as isize)
            };
            let right = self.peers[right_index];
            if right.state.is_at_least(minimum_state) {
                high_seen += 1;
                if high_seen == width {
                    high = Some(right.peer_id);
                }
            }

            if low.is_some() && high.is_some() {
                break;
            }
        }

        match (low, high) {
            (Some(low), Some(high)) if low != high => Some(RingSpan::new(low, high)),
            (Some(_), _) | (_, Some(_)) => Some(RingSpan::new(PeerId::MIN, PeerId::MAX)),
            _ => None,
        }
    }

    pub fn scan_gaps(&self, minimum_state: LifecyclePeerState) -> Vec<SpectrumGap> {
        scan_gaps_in_entries(&self.peers, minimum_state)
    }

    pub fn known_gaps(&self) -> Vec<SpectrumGap> {
        self.scan_gaps(LifecyclePeerState::Identified)
    }

    pub fn connected_gaps(&self) -> Vec<SpectrumGap> {
        self.scan_gaps(LifecyclePeerState::Connected)
    }

    pub fn count_in_span(&self, span: RingSpan, minimum_state: LifecyclePeerState) -> usize {
        self.peers
            .iter()
            .filter(|peer| peer.state.is_at_least(minimum_state) && span.contains(peer.peer_id))
            .count()
    }

    fn upsert_entry(&mut self, entry: PeerEntry) {
        if entry.peer_id == self.local_peer_id {
            return;
        }

        match self
            .peers
            .binary_search_by_key(&entry.peer_id, |peer| peer.peer_id)
        {
            Ok(index) => self.peers[index] = entry,
            Err(index) => self.peers.insert(index, entry),
        }
    }

    fn insertion_index(&self, token: TokenId) -> usize {
        match self.peers.binary_search_by_key(&token, |peer| peer.peer_id) {
            Ok(i) | Err(i) => i,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnswerRepairConfig {
    pub min_connected_per_span: usize,
}

impl Default for AnswerRepairConfig {
    fn default() -> Self {
        Self {
            min_connected_per_span: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LivenessConfig {
    pub stale_after: EcTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingSpan {
    pub low: TokenId,
    pub high: TokenId,
}

impl RingSpan {
    pub fn new(low: TokenId, high: TokenId) -> Self {
        Self { low, high }
    }

    pub fn contains(&self, token: TokenId) -> bool {
        if self.low <= self.high {
            token >= self.low && token <= self.high
        } else {
            token >= self.low || token <= self.high
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerOrigin {
    Invite,
    DiscoveryProbe { token: TokenId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerRepairDecision {
    StartElection { span: RingSpan },
    Stop(AnswerStopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerStopReason {
    Filled,
    InvalidSpan,
    WrongProbeToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerLiveness {
    pub peer_id: PeerId,
    pub last_heard: EcTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectrumGap {
    pub left: PeerId,
    pub right: PeerId,
    pub midpoint: TokenId,
    pub width: u128,
}

pub fn answer_span(signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE]) -> Option<RingSpan> {
    if TOKENS_SIGNATURE_SIZE < 10 {
        return None;
    }

    let high = signature[4].id;
    let low = signature[9].id;
    if high == 0 && low == 0 {
        return None;
    }

    Some(RingSpan::new(low, high))
}

pub fn decide_answer_repair(
    answer: &TokenMapping,
    signature: &[TokenMapping; TOKENS_SIGNATURE_SIZE],
    connected_peers: impl IntoIterator<Item = PeerId>,
    origin: AnswerOrigin,
    config: AnswerRepairConfig,
) -> AnswerRepairDecision {
    if let AnswerOrigin::DiscoveryProbe { token } = origin {
        if answer.id != token {
            return AnswerRepairDecision::Stop(AnswerStopReason::WrongProbeToken);
        }
    }

    let Some(span) = answer_span(signature) else {
        return AnswerRepairDecision::Stop(AnswerStopReason::InvalidSpan);
    };

    let min_connected = config.min_connected_per_span.max(1);
    let count = connected_peers
        .into_iter()
        .filter(|peer_id| span.contains(*peer_id))
        .count();
    if count >= min_connected {
        return AnswerRepairDecision::Stop(AnswerStopReason::Filled);
    }

    AnswerRepairDecision::StartElection { span }
}

pub fn stale_connected_peers(
    peers: impl IntoIterator<Item = PeerLiveness>,
    now: EcTime,
    config: LivenessConfig,
) -> Vec<PeerId> {
    peers
        .into_iter()
        .filter(|peer| now.saturating_sub(peer.last_heard) > config.stale_after)
        .map(|peer| peer.peer_id)
        .collect()
}

fn scan_gaps_in_entries(
    peers: &[PeerEntry],
    minimum_state: LifecyclePeerState,
) -> Vec<SpectrumGap> {
    if peers.is_empty() {
        return Vec::new();
    }

    let mut gaps = Vec::new();
    let mut first = None;
    let mut previous = None;

    for peer in peers.iter().copied() {
        if !peer.state.is_at_least(minimum_state) {
            continue;
        }

        if let Some(left) = previous {
            gaps.push(spectrum_gap(left, peer.peer_id));
        } else {
            first = Some(peer.peer_id);
        }

        previous = Some(peer.peer_id);
    }

    if let (Some(left), Some(right)) = (previous, first) {
        gaps.push(spectrum_gap(left, right));
    }

    gaps
}

fn spectrum_gap(left: PeerId, right: PeerId) -> SpectrumGap {
    let width = if left == right {
        RING_SIZE
    } else {
        clockwise_distance(left, right)
    };
    SpectrumGap {
        left,
        right,
        midpoint: left.wrapping_add((width / 2) as u64),
        width,
    }
}

fn idx_adj(len: usize, idx: usize, adj: isize) -> usize {
    (idx as isize + adj).rem_euclid(len as isize) as usize
}

fn push_matching(
    selected: &mut Vec<PeerId>,
    peer: PeerEntry,
    minimum_state: LifecyclePeerState,
    count: usize,
) {
    if selected.len() < count
        && peer.state.is_at_least(minimum_state)
        && !selected.contains(&peer.peer_id)
    {
        selected.push(peer.peer_id);
    }
}

fn ring_distance(a: PeerId, b: TokenId) -> u128 {
    let forward = clockwise_distance(a, b);
    let backward = clockwise_distance(b, a);
    forward.min(backward)
}

fn clockwise_distance(left: PeerId, right: PeerId) -> u128 {
    if right >= left {
        (right - left) as u128
    } else {
        RING_SIZE - left as u128 + right as u128
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: LifecyclePeerState = LifecyclePeerState::Identified;
    const ACTIVE: LifecyclePeerState = LifecyclePeerState::Pending;
    const CONNECTED: LifecyclePeerState = LifecyclePeerState::Connected;

    fn signature_with_span(low: TokenId, high: TokenId) -> [TokenMapping; TOKENS_SIGNATURE_SIZE] {
        let mut signature = [TokenMapping { id: 0, block: 0 }; TOKENS_SIGNATURE_SIZE];
        signature[0].id = high.saturating_sub(4);
        signature[1].id = high.saturating_sub(3);
        signature[2].id = high.saturating_sub(2);
        signature[3].id = high.saturating_sub(1);
        signature[4].id = high;
        signature[5].id = low.saturating_add(4);
        signature[6].id = low.saturating_add(3);
        signature[7].id = low.saturating_add(2);
        signature[8].id = low.saturating_add(1);
        signature[9].id = low;
        signature
    }

    #[test]
    fn bootstrap_keeps_sorted_entries_and_skips_self() {
        let spectrum = PeerSpectrum::from_entries(
            1,
            [
                PeerEntry::new(30, LifecyclePeerState::Identified),
                PeerEntry::new(1, LifecyclePeerState::Connected),
                PeerEntry::new(10, LifecyclePeerState::Pending),
                PeerEntry::new(30, LifecyclePeerState::Connected),
            ],
        );

        assert_eq!(spectrum.peers_around(0, 4, ACTIVE), vec![10, 30]);
        assert_eq!(spectrum.get(1), None);
        assert_eq!(
            spectrum.get(30),
            Some(PeerEntry::new(30, LifecyclePeerState::Connected))
        );
    }

    #[test]
    fn record_signal_updates_state_and_last_heard_from() {
        let mut spectrum = PeerSpectrum::new(1);

        assert!(!spectrum.record_signal(1, LifecyclePeerState::Connected, 10));
        assert!(spectrum.record_signal(20, LifecyclePeerState::Identified, 10));
        assert_eq!(
            spectrum.get(20),
            Some(PeerEntry {
                peer_id: 20,
                state: LifecyclePeerState::Identified,
                last_heard_from: Some(10),
            })
        );

        assert!(spectrum.record_signal(20, LifecyclePeerState::Connected, 15));
        assert_eq!(
            spectrum.get(20),
            Some(PeerEntry {
                peer_id: 20,
                state: LifecyclePeerState::Connected,
                last_heard_from: Some(15),
            })
        );
    }

    #[test]
    fn peers_around_scans_outward_for_requested_states() {
        let spectrum = PeerSpectrum::from_entries(
            1,
            [
                PeerEntry::new(10, LifecyclePeerState::Connected),
                PeerEntry::new(20, LifecyclePeerState::Pending),
                PeerEntry::new(30, LifecyclePeerState::Connected),
                PeerEntry::new(90, LifecyclePeerState::Identified),
            ],
        );

        assert_eq!(spectrum.peers_around(22, 3, ACTIVE), vec![20, 30, 10]);
        assert_eq!(spectrum.peers_around(22, 3, CONNECTED), vec![30, 10]);
        assert_eq!(spectrum.peers_around(89, 2, ACTIVE), vec![30, 20]);
    }

    #[test]
    fn span_around_uses_requested_states() {
        let spectrum = PeerSpectrum::from_entries(
            1,
            [
                PeerEntry::new(10, LifecyclePeerState::Connected),
                PeerEntry::new(20, LifecyclePeerState::Pending),
                PeerEntry::new(30, LifecyclePeerState::Connected),
                PeerEntry::new(40, LifecyclePeerState::Identified),
                PeerEntry::new(50, LifecyclePeerState::Connected),
                PeerEntry::new(60, LifecyclePeerState::Connected),
                PeerEntry::new(70, LifecyclePeerState::Connected),
            ],
        );

        assert_eq!(
            spectrum.span_around(45, 1, CONNECTED),
            Some(RingSpan::new(30, 50))
        );
        assert_eq!(
            spectrum.span_around(45, 1, ACTIVE),
            Some(RingSpan::new(30, 50))
        );
        assert_eq!(
            spectrum.span_around(45, 1, LifecyclePeerState::Identified),
            Some(RingSpan::new(40, 50))
        );
    }

    #[test]
    fn span_around_peer_id_skips_the_peer_itself() {
        let spectrum = PeerSpectrum::from_entries(
            1,
            [
                PeerEntry::new(10, LifecyclePeerState::Connected),
                PeerEntry::new(20, LifecyclePeerState::Connected),
                PeerEntry::new(30, LifecyclePeerState::Connected),
            ],
        );

        assert_eq!(
            spectrum.span_around(20, 1, CONNECTED),
            Some(RingSpan::new(10, 30))
        );
    }

    #[test]
    fn answer_span_uses_outer_proof_tokens() {
        let signature = signature_with_span(10, 40);

        assert_eq!(answer_span(&signature), Some(RingSpan::new(10, 40)));
        assert!(answer_span(&signature).unwrap().contains(10));
        assert!(answer_span(&signature).unwrap().contains(25));
        assert!(answer_span(&signature).unwrap().contains(40));
        assert!(!answer_span(&signature).unwrap().contains(41));
    }

    #[test]
    fn wrapped_span_contains_both_sides_of_ring() {
        let span = RingSpan::new(90, 10);

        assert!(span.contains(95));
        assert!(span.contains(5));
        assert!(!span.contains(50));
    }

    #[test]
    fn answer_repair_starts_election_when_span_is_underfilled() {
        let answer = TokenMapping { id: 25, block: 1 };
        let signature = signature_with_span(10, 40);

        let decision = decide_answer_repair(
            &answer,
            &signature,
            [80, 90],
            AnswerOrigin::Invite,
            AnswerRepairConfig::default(),
        );

        assert_eq!(
            decision,
            AnswerRepairDecision::StartElection {
                span: RingSpan::new(10, 40)
            }
        );
    }

    #[test]
    fn answer_repair_stops_when_span_is_filled() {
        let answer = TokenMapping { id: 25, block: 1 };
        let signature = signature_with_span(10, 40);

        let decision = decide_answer_repair(
            &answer,
            &signature,
            [15, 80],
            AnswerOrigin::Invite,
            AnswerRepairConfig::default(),
        );

        assert_eq!(
            decision,
            AnswerRepairDecision::Stop(AnswerStopReason::Filled)
        );
    }

    #[test]
    fn discovery_probe_answer_must_match_probe_token() {
        let answer = TokenMapping { id: 25, block: 1 };
        let signature = signature_with_span(10, 40);

        let decision = decide_answer_repair(
            &answer,
            &signature,
            [],
            AnswerOrigin::DiscoveryProbe { token: 30 },
            AnswerRepairConfig::default(),
        );

        assert_eq!(
            decision,
            AnswerRepairDecision::Stop(AnswerStopReason::WrongProbeToken)
        );
    }

    #[test]
    fn stale_connected_peers_are_local_pruning_candidates() {
        let stale = stale_connected_peers(
            [
                PeerLiveness {
                    peer_id: 1,
                    last_heard: 10,
                },
                PeerLiveness {
                    peer_id: 2,
                    last_heard: 80,
                },
            ],
            100,
            LivenessConfig { stale_after: 30 },
        );

        assert_eq!(stale, vec![1]);
    }

    #[test]
    fn spectrum_gaps_include_wrapping_gap() {
        let gaps = scan_gaps_in_entries(
            &[
                PeerEntry::new(10, LifecyclePeerState::Connected),
                PeerEntry::new(20, LifecyclePeerState::Connected),
                PeerEntry::new(30, LifecyclePeerState::Connected),
            ],
            CONNECTED,
        );

        assert_eq!(gaps.len(), 3);
        assert_eq!(gaps[0].width, 10);
        assert_eq!(gaps[1].width, 10);
        assert_eq!(gaps[2].left, 30);
        assert_eq!(gaps[2].right, 10);
        assert_eq!(
            gaps[2].midpoint,
            30u64.wrapping_add(gaps[2].width as u64 / 2)
        );
    }

    #[test]
    fn scan_gaps_uses_only_requested_states() {
        let spectrum = PeerSpectrum::from_entries(
            1,
            [
                PeerEntry::new(10, LifecyclePeerState::Identified),
                PeerEntry::new(20, LifecyclePeerState::Pending),
                PeerEntry::new(30, LifecyclePeerState::Connected),
                PeerEntry::new(40, LifecyclePeerState::Connected),
            ],
        );

        let known = spectrum.known_gaps();
        let connected = spectrum.connected_gaps();
        let active = spectrum.scan_gaps(ACTIVE);

        assert_eq!(known.len(), 4);
        assert_eq!(known[0], spectrum_gap(10, 20));
        assert_eq!(connected, vec![spectrum_gap(30, 40), spectrum_gap(40, 30)]);
        assert_eq!(active.len(), 3);
        assert_eq!(active[0], spectrum_gap(20, 30));
    }

    #[test]
    fn scan_gaps_returns_raw_observations_for_policy_to_rank() {
        let spectrum = PeerSpectrum::from_entries(
            1,
            [
                PeerEntry::new(0, LifecyclePeerState::Connected),
                PeerEntry::new(10, LifecyclePeerState::Connected),
                PeerEntry::new(100, LifecyclePeerState::Connected),
            ],
        );

        let gaps = spectrum.scan_gaps(CONNECTED);
        let largest = gaps.iter().max_by_key(|gap| gap.width).unwrap();

        assert_eq!(*largest, spectrum_gap(100, 0));
        assert_eq!(
            largest.midpoint,
            100u64.wrapping_add(largest.width as u64 / 2)
        );
    }

    #[test]
    fn count_in_span_counts_minimum_state_density() {
        let spectrum = PeerSpectrum::from_entries(
            1,
            [
                PeerEntry::new(10, LifecyclePeerState::Identified),
                PeerEntry::new(20, LifecyclePeerState::Pending),
                PeerEntry::new(30, LifecyclePeerState::Connected),
                PeerEntry::new(90, LifecyclePeerState::Connected),
            ],
        );
        let span = RingSpan::new(15, 35);

        assert_eq!(spectrum.count_in_span(span, KNOWN), 2);
        assert_eq!(spectrum.count_in_span(span, ACTIVE), 2);
        assert_eq!(spectrum.count_in_span(span, CONNECTED), 1);
    }
}
