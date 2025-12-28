// Scenario Builder for Peer Lifecycle Simulations
//
// Provides a fluent API for defining simulation scenarios with scheduled events

use super::config::{
    EventSchedule, ScheduledEvent, NetworkEvent, PeerSelection, InitialNetworkState,
    TokenDistributionConfig, TopologyMode, PeerLifecycleConfig,
};
use ec_rust::ec_interface::PeerId;

// ============================================================================
// ScenarioBuilder
// ============================================================================

/// Builder for creating simulation scenarios
pub struct ScenarioBuilder {
    events: Vec<ScheduledEvent>,
}

impl ScenarioBuilder {
    /// Create a new empty scenario
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
        }
    }

    /// Start defining events for a specific round
    pub fn at_round(self, round: usize) -> RoundBuilder {
        RoundBuilder {
            scenario: self,
            round,
        }
    }

    /// Build the final EventSchedule
    pub fn build(self) -> EventSchedule {
        EventSchedule {
            events: self.events,
        }
    }
}

impl Default for ScenarioBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// RoundBuilder
// ============================================================================

/// Builder for defining events at a specific round
pub struct RoundBuilder {
    scenario: ScenarioBuilder,
    round: usize,
}

impl RoundBuilder {
    /// Report current statistics with optional label
    pub fn report_stats(mut self, label: impl Into<String>) -> ScenarioBuilder {
        self.scenario.events.push(ScheduledEvent {
            round: self.round,
            event: NetworkEvent::ReportStats {
                label: Some(label.into()),
            },
        });
        self.scenario
    }

    /// Report statistics without a label
    pub fn report_stats_unlabeled(mut self) -> ScenarioBuilder {
        self.scenario.events.push(ScheduledEvent {
            round: self.round,
            event: NetworkEvent::ReportStats { label: None },
        });
        self.scenario
    }

    /// Add peers to the network
    pub fn peers_join(
        mut self,
        count: usize,
        coverage: f64,
        bootstrap_method: BootstrapMethod,
        group_name: impl Into<String>,
    ) -> ScenarioBuilder {
        let initial_knowledge = match bootstrap_method {
            BootstrapMethod::Random(n) => {
                // In a real implementation, we'd select N random peers
                // For now, this is a placeholder - the simulator will handle it
                vec![]
            }
            BootstrapMethod::Specific(peers) => peers,
            BootstrapMethod::None => vec![],
        };

        self.scenario.events.push(ScheduledEvent {
            round: self.round,
            event: NetworkEvent::PeerJoin {
                count,
                coverage_fraction: coverage,
                initial_knowledge,
                group_name: Some(group_name.into()),
            },
        });
        self.scenario
    }

    /// Remove peers (crash scenario)
    pub fn peers_crash(mut self, selection: PeerSelection) -> ScenarioBuilder {
        self.scenario.events.push(ScheduledEvent {
            round: self.round,
            event: NetworkEvent::PeerCrash { selection },
        });
        self.scenario
    }

    /// Change network conditions
    pub fn network_conditions(
        mut self,
        delay_fraction: Option<f64>,
        loss_fraction: Option<f64>,
    ) -> ScenarioBuilder {
        self.scenario.events.push(ScheduledEvent {
            round: self.round,
            event: NetworkEvent::NetworkCondition {
                delay_fraction,
                loss_fraction,
            },
        });
        self.scenario
    }

    /// Pause elections for testing recovery
    pub fn pause_elections(mut self, duration: usize) -> ScenarioBuilder {
        self.scenario.events.push(ScheduledEvent {
            round: self.round,
            event: NetworkEvent::PauseElections { duration },
        });
        self.scenario
    }
}

// ============================================================================
// Bootstrap Methods
// ============================================================================

/// Methods for bootstrapping new peers
#[derive(Debug, Clone)]
pub enum BootstrapMethod {
    /// Know N random existing peers
    Random(usize),

    /// Know specific peer IDs
    Specific(Vec<PeerId>),

    /// No initial knowledge (isolated)
    None,
}

// ============================================================================
// Preset Scenarios
// ============================================================================

impl ScenarioBuilder {
    /// Bootstrap scenario: High token coverage, minimal peer knowledge
    /// Demonstrates that shared state knowledge enables rapid bootstrapping
    pub fn bootstrap_shared_state() -> Self {
        Self::new()
            .at_round(50).report_stats("Early bootstrap")
            .at_round(100).report_stats("Mid bootstrap")
            .at_round(150).report_stats("Late bootstrap")
    }

    /// Steady-state join scenario: New peers join with varying coverage
    /// Demonstrates effect of token coverage on connectivity
    pub fn steady_state_joins() -> Self {
        Self::new()
            .at_round(100).report_stats("Before joins - steady state")
            .at_round(110).peers_join(5, 0.95, BootstrapMethod::Random(3), "high-coverage")
            .at_round(120).report_stats("After high-coverage joins")
            .at_round(130).peers_join(5, 0.50, BootstrapMethod::Random(3), "low-coverage")
            .at_round(140).report_stats("After low-coverage joins")
            .at_round(150).report_stats("Final state")
    }

    /// Stress test: Network churn with crashes and recoveries
    pub fn network_churn() -> Self {
        Self::new()
            .at_round(50).report_stats("Baseline")
            .at_round(75).peers_crash(PeerSelection::Random { count: 10 })
            .at_round(80).report_stats("After crash")
            .at_round(100).report_stats("Recovery progress")
            .at_round(150).report_stats("Final recovery")
    }

    /// Network partition recovery test
    pub fn partition_recovery() -> Self {
        Self::new()
            .at_round(50).report_stats("Before partition")
            .at_round(60).network_conditions(Some(0.8), Some(0.3))
            .at_round(70).report_stats("During partition")
            .at_round(100).network_conditions(Some(0.3), Some(0.01))
            .at_round(110).report_stats("After partition healed")
            .at_round(150).report_stats("Full recovery")
    }
}
