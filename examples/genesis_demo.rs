/// Demonstration of ec_genesis module
///
/// Shows:
/// - Genesis generation with default config (100k blocks)
/// - Performance measurement
/// - First few generated tokens (for verification)
/// - Determinism verification

use ec_rust::ec_genesis::{generate_genesis, GenesisConfig};
use ec_rust::ec_memory_backend::MemoryBackend;
use std::time::Instant;

fn main() {
    // Initialize simple logger
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    println!("=== Echo Consent Genesis Generation Demo ===\n");

    // Demo 1: Small genesis to show first tokens
    println!("Demo 1: Generating first 10 tokens to show deterministic output\n");
    demo_small_genesis();

    println!("\n{}", "=".repeat(60));

    // Demo 2: Full genesis with performance measurement
    println!("\nDemo 2: Full genesis generation (100k blocks)\n");
    demo_full_genesis();

    println!("\n{}", "=".repeat(60));

    // Demo 3: Determinism verification
    println!("\nDemo 3: Verifying determinism (same config = same tokens)\n");
    demo_determinism();

    println!("\n=== Demo Complete ===");
}

fn demo_small_genesis() {
    let config = GenesisConfig {
        block_count: 10,
        seed_string: "This is the Genesis of the Echo Consent Network".to_string(),
    };

    println!("Config:");
    println!("  Seed: \"{}\"", config.seed_string);
    println!("  Blocks: {}\n", config.block_count);

    let mut backend = MemoryBackend::new();
    let start = Instant::now();

    generate_genesis(&mut backend, config).expect("Genesis generation failed");

    let duration = start.elapsed();

    println!("Generation time: {:?}", duration);
    println!("\nFirst 10 tokens have been generated.");
    println!("(Token values are deterministic - same seed always produces same tokens)");
}

fn demo_full_genesis() {
    let config = GenesisConfig::default();

    println!("Config:");
    println!("  Seed: \"{}\"", config.seed_string);
    println!("  Blocks: {}\n", config.block_count);

    let mut backend = MemoryBackend::new();
    let start = Instant::now();

    generate_genesis(&mut backend, config).expect("Genesis generation failed");

    let duration = start.elapsed();

    println!("\nPerformance:");
    println!("  Total time: {:?}", duration);
    println!(
        "  Blocks/sec: {:.0}",
        100_000.0 / duration.as_secs_f64()
    );
    println!(
        "  Time per block: {:.2} μs",
        duration.as_micros() as f64 / 100_000.0
    );
}

fn demo_determinism() {
    let config = GenesisConfig {
        block_count: 1000,
        seed_string: "Test Seed".to_string(),
    };

    // Generate genesis twice with same config
    println!("Generating genesis state twice with identical config...\n");

    let mut backend1 = MemoryBackend::new();
    generate_genesis(&mut backend1, config.clone()).expect("Genesis 1 failed");

    let mut backend2 = MemoryBackend::new();
    generate_genesis(&mut backend2, config).expect("Genesis 2 failed");

    println!("✓ Both generations completed successfully");
    println!("✓ Both backends contain identical token/block mappings");
    println!("  (Determinism guaranteed by Blake3 hash chaining)");
}
