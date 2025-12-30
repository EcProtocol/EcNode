use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2, Params, Version,
};
use std::time::Instant;
use x25519_dalek::{PublicKey, StaticSecret};
use rand::rngs::OsRng;

/// Benchmark single Argon2 computation cost (= validation cost)
fn main() {
    std::env::set_var("RUST_LOG", "error");
    let _ = simple_logger::init();

    println!("\n=== Argon2 Single Hash Benchmark (Validation Cost) ===\n");

    // Generate a sample public key
    let secret = StaticSecret::random_from_rng(OsRng);
    let public_key = PublicKey::from(&secret);
    let public_key_bytes = public_key.as_bytes();

    // Sample salt
    let salt = [0x42u8; 16];
    let salt_b64 = SaltString::encode_b64(&salt).unwrap();

    // Test different Argon2 configurations
    let configs = vec![
        ("Test (256 KiB, 1 iter)", 256, 1),
        ("Very Low (512 KiB, 1 iter)", 512, 1),
        ("Low (1 MiB, 1 iter)", 1024, 1),
        ("Balanced (4 MiB, 1 iter)", 4096, 1),
        ("Medium (16 MiB, 1 iter)", 16384, 1),
        ("High (64 MiB, 1 iter)", 65536, 1),
        ("Production Current (64 MiB, 3 iter)", 65536, 3),
    ];

    println!("{:<40} {:>12} {:>15} {:>20}",
             "Configuration", "Time (ms)", "Validations/s", "Expected Mining");
    println!("{}", "-".repeat(90));

    for (name, memory_kib, time_cost) in configs {
        let params = Params::new(memory_kib, time_cost, 1, Some(32)).unwrap();
        let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

        // Warm-up
        let _ = argon2.hash_password(public_key_bytes, &salt_b64).unwrap();

        // Measure multiple iterations
        let samples = 10;
        let mut total_time = 0.0;

        for _ in 0..samples {
            let start = Instant::now();
            let _ = argon2.hash_password(public_key_bytes, &salt_b64).unwrap();
            total_time += start.elapsed().as_secs_f64();
        }

        let avg_time_ms = (total_time / samples as f64) * 1000.0;
        let validations_per_sec = 1000.0 / avg_time_ms;

        // For 24-hour mining target, what difficulty do we need?
        // 24 hours = 86400 seconds
        // attempts_needed = 2^difficulty
        // mining_time = attempts_needed * single_hash_time
        // 86400 = 2^difficulty * (avg_time_ms / 1000)
        // 2^difficulty = 86400 * 1000 / avg_time_ms
        let target_attempts = 86400.0 * 1000.0 / avg_time_ms;
        let required_difficulty = target_attempts.log2();

        println!("{:<40} {:>12.2} {:>15.0} {:>20.1} bits",
                 name, avg_time_ms, validations_per_sec, required_difficulty);
    }

    println!("\n{}", "=".repeat(90));
    println!("\n=== Analysis ===\n");

    println!("Current Production Settings (64 MiB, 3 iter, 20 bits):");
    println!("  - Validation cost: ~200-250ms per peer");
    println!("  - Throughput: ~4-5 validations/sec");
    println!("  - Problem: TOO SLOW for frequent validation!");

    println!("\nRecommended Production Settings:");
    println!("  Option 1: Balanced (4 MiB, 1 iter, ~22 bits)");
    println!("    - Validation: ~5-10ms per peer");
    println!("    - Throughput: ~100-200 validations/sec");
    println!("    - Still memory-hard, much faster validation");

    println!("\n  Option 2: Low Memory (1 MiB, 1 iter, ~23 bits)");
    println!("    - Validation: ~2-3ms per peer");
    println!("    - Throughput: ~300-500 validations/sec");
    println!("    - Less ASIC-resistant, but very fast");

    println!("\n  Option 3: Very Low (512 KiB, 1 iter, ~23-24 bits)");
    println!("    - Validation: ~1-2ms per peer");
    println!("    - Throughput: ~500-1000 validations/sec");
    println!("    - Fastest validation, rely on difficulty for sybil resistance");

    println!("\nKey Insight:");
    println!("  Mining cost = attempts × single_hash_cost");
    println!("  Validation cost = single_hash_cost (happens frequently!)");
    println!("  → Lower memory + higher difficulty = same mining time, MUCH faster validation");
}
