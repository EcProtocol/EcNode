use ec_rust::{AddressConfig, PeerIdentity};
use std::time::Instant;

/// Benchmark Argon2 validation cost with different parameters
fn main() {
    // Suppress debug logs for cleaner output
    std::env::set_var("RUST_LOG", "error");
    let _ = simple_logger::init();

    println!("\n=== Argon2 Validation Cost Benchmark ===\n");

    // Test configurations
    let configs = vec![
        (
            "Test (256 KiB, 1 iter)",
            AddressConfig::TEST,
        ),
        (
            "Production Current (64 MiB, 3 iter)",
            AddressConfig::PRODUCTION,
        ),
        (
            "Low Memory (1 MiB, 1 iter, 23 bits)",
            AddressConfig {
                difficulty: 23,
                memory_cost: 1024,    // 1 MiB
                time_cost: 1,
                parallelism: 1,
            },
        ),
        (
            "Very Low (256 KiB, 1 iter, 24 bits)",
            AddressConfig {
                difficulty: 24,
                memory_cost: 256,     // 256 KiB
                time_cost: 1,
                parallelism: 1,
            },
        ),
        (
            "Balanced (4 MiB, 1 iter, 22 bits)",
            AddressConfig {
                difficulty: 22,
                memory_cost: 4096,    // 4 MiB
                time_cost: 1,
                parallelism: 1,
            },
        ),
    ];

    for (name, config) in configs {
        println!("Configuration: {}", name);
        println!("  Memory: {} KiB", config.memory_cost);
        println!("  Time cost: {} iterations", config.time_cost);
        println!("  Difficulty: {} trailing zero bits", config.difficulty);

        // Create and mine an identity
        let mut identity = PeerIdentity::new();

        let mine_start = Instant::now();
        identity.mine(config);
        let mine_duration = mine_start.elapsed();

        println!("  Mining: {:.2}s ({} attempts)",
                 mine_duration.as_secs_f64(),
                 identity.attempts);

        // Measure validation cost (single Argon2 computation)
        let validation_samples = 10;
        let mut total_validation_time = 0.0;

        for _ in 0..validation_samples {
            let start = Instant::now();
            let is_valid = PeerIdentity::validate(
                &identity.public_key,
                identity.salt().unwrap(),
                identity.peer_id().unwrap(),
                &config,
            );
            total_validation_time += start.elapsed().as_secs_f64();
            assert!(is_valid);
        }

        let avg_validation_ms = (total_validation_time / validation_samples as f64) * 1000.0;

        println!("  Validation: {:.2} ms per peer (avg of {} runs)",
                 avg_validation_ms, validation_samples);

        // Estimate peers validated per second
        let peers_per_sec = 1000.0 / avg_validation_ms;
        println!("  Throughput: ~{:.0} peer validations/sec", peers_per_sec);

        // Expected mining time for 1 day target
        let expected_attempts = 2u64.pow(config.difficulty);
        let single_hash_ms = avg_validation_ms;
        let expected_mining_hours = (expected_attempts as f64 * single_hash_ms / 1000.0) / 3600.0;
        println!("  Expected mining time: ~{:.1} hours (2^{} attempts)",
                 expected_mining_hours, config.difficulty);

        println!();
    }

    println!("\n=== Recommendations ===");
    println!("For production (targeting ~24 hour mining on modern CPU):");
    println!("  - Use LOW memory (1-4 MiB) for fast validation");
    println!("  - Increase difficulty (22-24 bits) to compensate");
    println!("  - Keep time_cost = 1 (validation happens frequently)");
    println!("\nTrade-off:");
    println!("  - Lower memory = less ASIC-resistant, but much faster validation");
    println!("  - Higher difficulty = longer mining, but maintains sybil resistance");
    println!("  - Validation cost matters more than mining cost (mining is one-time)");
}
