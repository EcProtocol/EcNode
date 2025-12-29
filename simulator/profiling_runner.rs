// Profiling Runner - Scenario runner with timing instrumentation
//
// Usage:
//   cargo run --bin profiling_runner --release scenarios/bootstrap.yaml

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
};
use std::fs;
use std::path::Path;
use std::env;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <scenario.yaml>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} scenarios/bootstrap.yaml", args[0]);
        std::process::exit(1);
    }

    let path = Path::new(&args[1]);

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  PROFILING RUNNER                                      ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    let total_start = Instant::now();

    // Time: Loading YAML
    let load_start = Instant::now();
    println!("Loading scenario from: {}", path.display());

    let yaml_content = fs::read_to_string(path)
        .unwrap_or_else(|e| {
            eprintln!("Failed to read {}: {}", path.display(), e);
            std::process::exit(1);
        });

    let load_time = load_start.elapsed();
    println!("  ✓ File read: {:?}", load_time);

    // Time: Parsing YAML
    let parse_start = Instant::now();

    #[derive(Debug, serde::Deserialize)]
    struct ScenarioFile {
        config: ScenarioConfig,
        events: peer_lifecycle::EventSchedule,
        #[serde(default)]
        meta: ScenarioMeta,
    }

    #[derive(Debug, Default, serde::Deserialize)]
    struct ScenarioMeta {
        name: Option<String>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct ScenarioConfig {
        rounds: usize,
        #[serde(default = "default_tick_duration")]
        tick_duration_ms: u64,
        initial_state: peer_lifecycle::InitialNetworkState,
        token_distribution: peer_lifecycle::TokenDistributionConfig,
        #[serde(default)]
        peer_config: Option<PeerConfigOverrides>,
        #[serde(default)]
        network: Option<NetworkConfigOverrides>,
    }

    #[derive(Debug, Default, serde::Deserialize)]
    struct PeerConfigOverrides {
        elections_per_tick: Option<usize>,
        election_timeout: Option<u64>,
        min_collection_time: Option<u64>,
        pending_timeout: Option<u64>,
        connection_timeout: Option<u64>,
        election_consensus_threshold: Option<usize>,
        election_majority_threshold: Option<f64>,
    }

    #[derive(Debug, Default, serde::Deserialize)]
    struct NetworkConfigOverrides {
        delay_fraction: Option<f64>,
        loss_fraction: Option<f64>,
    }

    fn default_tick_duration() -> u64 {
        100
    }

    let scenario: ScenarioFile = serde_yaml::from_str(&yaml_content)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse {}: {}", path.display(), e);
            std::process::exit(1);
        });

    let parse_time = parse_start.elapsed();
    println!("  ✓ YAML parsed: {:?}", parse_time);

    // Time: Building configuration
    let config_start = Instant::now();

    let mut config = PeerLifecycleConfig::default();
    config.rounds = scenario.config.rounds;
    config.tick_duration_ms = scenario.config.tick_duration_ms;
    config.initial_state = scenario.config.initial_state;
    config.token_distribution = scenario.config.token_distribution;
    config.events = scenario.events;

    if let Some(ref peer_overrides) = scenario.config.peer_config {
        if let Some(v) = peer_overrides.elections_per_tick {
            config.peer_config.elections_per_tick = v;
        }
        if let Some(v) = peer_overrides.election_timeout {
            config.peer_config.election_timeout = v;
        }
        if let Some(v) = peer_overrides.min_collection_time {
            config.peer_config.min_collection_time = v;
        }
        if let Some(v) = peer_overrides.pending_timeout {
            config.peer_config.pending_timeout = v;
        }
        if let Some(v) = peer_overrides.connection_timeout {
            config.peer_config.connection_timeout = v;
        }
        if let Some(v) = peer_overrides.election_consensus_threshold {
            config.peer_config.election_config.consensus_threshold = v;
        }
        if let Some(v) = peer_overrides.election_majority_threshold {
            config.peer_config.election_config.majority_threshold = v;
        }
    }

    if let Some(ref net_overrides) = scenario.config.network {
        if let Some(v) = net_overrides.delay_fraction {
            config.network.delay_fraction = v;
        }
        if let Some(v) = net_overrides.loss_fraction {
            config.network.loss_fraction = v;
        }
    }

    let config_time = config_start.elapsed();
    println!("  ✓ Config built: {:?}", config_time);

    println!("\nConfiguration:");
    println!("  Rounds: {}", config.rounds);
    println!("  Initial Peers: {}", config.initial_state.num_peers);
    println!("  Total Tokens: {}", config.token_distribution.total_tokens);

    // Time: Running simulation
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  SIMULATION                                            ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    let sim_start = Instant::now();
    let runner = PeerLifecycleRunner::new(config);

    // We need to manually instrument the run() method, so let's just run it
    let result = runner.run();
    let sim_time = sim_start.elapsed();

    result.print_summary();

    // Print profiling results
    let total_time = total_start.elapsed();

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  PROFILING RESULTS                                     ║");
    println!("╚════════════════════════════════════════════════════════╝\n");

    println!("Time Breakdown:");
    print_timing("  File I/O", load_time, total_time);
    print_timing("  YAML Parsing", parse_time, total_time);
    print_timing("  Config Setup", config_time, total_time);
    print_timing("  Simulation", sim_time, total_time);
    println!("  ─────────────────────────────────────────");
    println!("  Total:           {:>10.3?}  (100.0%)", total_time);

    // Calculate simulation metrics
    let rounds = result.final_metrics.round;
    let total_peers = result.final_metrics.peer_counts.total_peers;
    let total_messages = result.message_overhead.total_messages;

    println!("\nSimulation Metrics:");
    println!("  Rounds:          {:>10}", rounds);
    println!("  Peers:           {:>10}", total_peers);
    println!("  Total Messages:  {:>10}", total_messages);
    println!();

    if sim_time.as_secs() > 0 {
        let rounds_per_sec = rounds as f64 / sim_time.as_secs_f64();
        let messages_per_sec = total_messages as f64 / sim_time.as_secs_f64();

        println!("Performance:");
        println!("  Rounds/sec:      {:>10.1}", rounds_per_sec);
        println!("  Messages/sec:    {:>10.0}", messages_per_sec);
        println!("  Time/round:      {:>10.3?}", sim_time / rounds as u32);

        if total_messages > 0 {
            let ns_per_message = sim_time.as_nanos() / total_messages as u128;
            println!("  Time/message:    {:>10}ns", ns_per_message);
        }
    }

    println!("\n✓ Profiling complete!\n");
}

fn print_timing(label: &str, time: Duration, total: Duration) {
    let percent = (time.as_secs_f64() / total.as_secs_f64()) * 100.0;
    println!("  {:<15}  {:>10.3?}  ({:>5.1}%)", label, time, percent);
}
