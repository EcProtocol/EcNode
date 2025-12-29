// Scenario Runner - Load and execute scenario YAML files
//
// Usage:
//   cargo run --bin scenario_runner scenarios/bootstrap.yaml
//   cargo run --bin scenario_runner scenarios/  (runs all .yaml files in directory)
//   cargo run --bin scenario_runner scenarios/bootstrap.yaml --seed 0x1234...

mod peer_lifecycle;

use peer_lifecycle::{
    PeerLifecycleConfig,
    PeerLifecycleRunner,
    InitialNetworkState,
    TokenDistributionConfig,
    EventSchedule,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// Simplified scenario file format
#[derive(Debug, serde::Deserialize)]
struct ScenarioFile {
    /// Scenario metadata
    #[serde(default)]
    meta: ScenarioMeta,

    /// Configuration overrides
    config: ScenarioConfig,

    /// Event schedule
    events: EventSchedule,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ScenarioMeta {
    name: Option<String>,
    description: Option<String>,
    hypothesis: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ScenarioConfig {
    // Core settings
    rounds: usize,

    #[serde(default = "default_tick_duration")]
    tick_duration_ms: u64,

    // Initial state
    initial_state: InitialNetworkState,

    // Token distribution
    token_distribution: TokenDistributionConfig,

    // Peer manager config overrides (optional)
    #[serde(default)]
    peer_config: Option<PeerConfigOverrides>,

    // Network config overrides (optional)
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

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <scenario.yaml | directory/> [--seed SEED_HEX]", args[0]);
        eprintln!("\nExamples:");
        eprintln!("  {} scenarios/bootstrap.yaml", args[0]);
        eprintln!("  {} scenarios/", args[0]);
        eprintln!("  {} scenarios/bootstrap.yaml --seed 0x123456...", args[0]);
        std::process::exit(1);
    }

    let path = Path::new(&args[1]);

    // Parse optional seed
    let seed: Option<[u8; 32]> = if args.len() >= 4 && args[2] == "--seed" {
        Some(parse_seed_hex(&args[3]))
    } else {
        None
    };

    if path.is_file() {
        run_scenario_file(path, seed);
    } else if path.is_dir() {
        run_scenario_directory(path, seed);
    } else {
        eprintln!("Error: Path does not exist: {}", path.display());
        std::process::exit(1);
    }
}

fn run_scenario_directory(dir: &Path, seed: Option<[u8; 32]>) {
    let mut scenarios = Vec::new();

    // Find all .yaml files
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") ||
               path.extension().and_then(|s| s.to_str()) == Some("yml") {
                scenarios.push(path);
            }
        }
    }

    scenarios.sort();

    if scenarios.is_empty() {
        eprintln!("No .yaml files found in {}", dir.display());
        std::process::exit(1);
    }

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  SCENARIO RUNNER - Multiple Scenarios                 ║");
    println!("╚════════════════════════════════════════════════════════╝\n");
    println!("Found {} scenario(s) to run\n", scenarios.len());

    for (i, scenario_path) in scenarios.iter().enumerate() {
        println!("\n{}/{} Running: {}\n", i + 1, scenarios.len(), scenario_path.display());
        run_scenario_file(scenario_path, seed);
    }

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  All scenarios complete!                               ║");
    println!("╚════════════════════════════════════════════════════════╝\n");
}

fn run_scenario_file(path: &Path, seed: Option<[u8; 32]>) {
    println!("Loading scenario from: {}", path.display());

    // Load and parse YAML
    let yaml_content = fs::read_to_string(path)
        .unwrap_or_else(|e| {
            eprintln!("Failed to read {}: {}", path.display(), e);
            std::process::exit(1);
        });

    let scenario: ScenarioFile = serde_yaml::from_str(&yaml_content)
        .unwrap_or_else(|e| {
            eprintln!("Failed to parse {}: {}", path.display(), e);
            std::process::exit(1);
        });

    // Print scenario header
    println!("\n╔════════════════════════════════════════════════════════╗");
    if let Some(ref name) = scenario.meta.name {
        println!("║  {}  {}", name, " ".repeat(54_usize.saturating_sub(name.len())));
    } else {
        println!("║  Scenario: {}  ", path.file_stem().unwrap().to_str().unwrap());
    }
    println!("╚════════════════════════════════════════════════════════╝\n");

    if let Some(ref desc) = scenario.meta.description {
        println!("{}\n", desc);
    }

    if let Some(ref hypothesis) = scenario.meta.hypothesis {
        println!("Hypothesis:");
        println!("  {}\n", hypothesis);
    }

    // Build configuration
    let mut config = PeerLifecycleConfig::default();

    // Apply scenario config
    config.rounds = scenario.config.rounds;
    config.tick_duration_ms = scenario.config.tick_duration_ms;
    config.initial_state = scenario.config.initial_state;
    config.token_distribution = scenario.config.token_distribution;
    config.events = scenario.events;
    config.seed = seed;

    // Apply peer config overrides
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

    // Apply network config overrides
    if let Some(ref net_overrides) = scenario.config.network {
        if let Some(v) = net_overrides.delay_fraction {
            config.network.delay_fraction = v;
        }
        if let Some(v) = net_overrides.loss_fraction {
            config.network.loss_fraction = v;
        }
    }

    println!("Configuration:");
    println!("  Rounds: {}", config.rounds);
    println!("  Initial Peers: {}", config.initial_state.num_peers);
    println!("  Topology: {:?}", config.initial_state.initial_topology);
    println!("  Total Tokens: {}", config.token_distribution.total_tokens);
    println!("  Coverage: {:.0}%", config.token_distribution.coverage_fraction * 100.0);
    println!("\nStarting simulation...\n");

    // Run simulation
    let runner = PeerLifecycleRunner::new(config);
    let result = runner.run();

    // Print results
    result.print_summary();

    println!("\n✓ Scenario complete!\n");
}

fn parse_seed_hex(hex: &str) -> [u8; 32] {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let mut seed = [0u8; 32];

    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 32 {
            break;
        }
        let byte_str = std::str::from_utf8(chunk).unwrap();
        seed[i] = u8::from_str_radix(byte_str, 16)
            .unwrap_or_else(|e| {
                eprintln!("Invalid hex seed: {}", e);
                std::process::exit(1);
            });
    }

    seed
}
