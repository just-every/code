//! Benchmark tests for MCP vs subprocess performance
//!
//! FORK-SPECIFIC (just-every/code): Validates performance claims
//!
//! Run with: cargo test --test mcp_consensus_benchmark --release -- --ignored --nocapture

use std::collections::{HashMap, HashSet};
use std::time::Instant;
use codex_core::config_types::McpServerConfig;
use codex_core::mcp_connection_manager::McpConnectionManager;
use codex_tui::SpecStage;

/// Benchmark MCP connection initialization
#[tokio::test]
#[ignore] // Run explicitly with --ignored
async fn bench_mcp_initialization() {
    const ITERATIONS: usize = 10;
    let mut timings = Vec::new();

    for i in 0..ITERATIONS {
        let start = Instant::now();

        let config = HashMap::from([(
            "local-memory".to_string(),
            McpServerConfig {
                command: "local-memory".to_string(),
                args: vec![],
                env: None,
                startup_timeout_ms: Some(5000),
            }
        )]);

        match McpConnectionManager::new(config, HashSet::new()).await {
            Ok((_, errors)) => {
                let elapsed = start.elapsed();
                timings.push(elapsed);

                if !errors.is_empty() {
                    println!("  Iteration {}: {:?} (with errors)", i + 1, elapsed);
                } else {
                    println!("  Iteration {}: {:?}", i + 1, elapsed);
                }
            }
            Err(e) => {
                println!("  Iteration {}: FAILED - {}", i + 1, e);
                println!("  Skipping benchmark - local-memory unavailable");
                return;
            }
        }
    }

    if timings.is_empty() {
        return;
    }

    let total: std::time::Duration = timings.iter().sum();
    let avg = total / timings.len() as u32;
    let min = timings.iter().min().unwrap();
    let max = timings.iter().max().unwrap();

    println!("\n=== MCP Connection Initialization Benchmark ===");
    println!("  Iterations: {}", ITERATIONS);
    println!("  Average: {:?}", avg);
    println!("  Min: {:?}", min);
    println!("  Max: {:?}", max);
    println!("  Total: {:?}", total);
}

/// Benchmark MCP tool calls (search)
#[tokio::test]
#[ignore] // Run explicitly with --ignored
async fn bench_mcp_search_calls() {
    let config = HashMap::from([(
        "local-memory".to_string(),
        McpServerConfig {
            command: "local-memory".to_string(),
            args: vec![],
            env: None,
            startup_timeout_ms: Some(5000),
        }
    )]);

    let (manager, errors) = match McpConnectionManager::new(config, HashSet::new()).await {
        Ok(result) => result,
        Err(e) => {
            println!("  Skipping benchmark - MCP initialization failed: {}", e);
            return;
        }
    };

    if !errors.is_empty() {
        println!("  Skipping benchmark - MCP had initialization errors");
        return;
    }

    const ITERATIONS: usize = 50;
    let mut timings = Vec::new();

    for i in 0..ITERATIONS {
        let args = serde_json::json!({
            "query": format!("test-query-{}", i),
            "limit": 10,
            "search_type": "hybrid"
        });

        let start = Instant::now();
        match manager.call_tool(
            "local-memory",
            "search",
            Some(args),
            Some(std::time::Duration::from_secs(10))
        ).await {
            Ok(_) => {
                let elapsed = start.elapsed();
                timings.push(elapsed);
            }
            Err(e) => {
                println!("  Call {}: FAILED - {}", i + 1, e);
            }
        }
    }

    if timings.is_empty() {
        println!("  No successful calls - skipping analysis");
        return;
    }

    let total: std::time::Duration = timings.iter().sum();
    let avg = total / timings.len() as u32;
    let min = timings.iter().min().unwrap();
    let max = timings.iter().max().unwrap();

    println!("\n=== MCP Search Call Benchmark ===");
    println!("  Successful calls: {}/{}", timings.len(), ITERATIONS);
    println!("  Average: {:?}", avg);
    println!("  Min: {:?}", min);
    println!("  Max: {:?}", max);
    println!("  Total: {:?}", total);
    println!("  Calls/sec: {:.2}", timings.len() as f64 / total.as_secs_f64());
}

/// Compare MCP vs subprocess latency (requires both available)
#[tokio::test]
#[ignore] // Run explicitly with --ignored
async fn bench_mcp_vs_subprocess() {
    use std::process::Command;

    println!("\n=== MCP vs Subprocess Comparison ===\n");

    // Benchmark subprocess version
    const SUBPROCESS_ITERATIONS: usize = 10;
    let mut subprocess_timings = Vec::new();

    println!("Benchmarking subprocess calls ({} iterations)...", SUBPROCESS_ITERATIONS);
    for _ in 0..SUBPROCESS_ITERATIONS {
        let start = Instant::now();
        let output = Command::new("local-memory")
            .args(&["search", "test", "--json", "--limit", "5"])
            .output();

        match output {
            Ok(result) if result.status.success() => {
                subprocess_timings.push(start.elapsed());
            }
            Ok(result) => {
                println!("  Subprocess call failed: {:?}", String::from_utf8_lossy(&result.stderr));
            }
            Err(e) => {
                println!("  Subprocess spawn failed: {}", e);
                println!("  Skipping subprocess benchmark");
                break;
            }
        }
    }

    // Benchmark MCP version
    const MCP_ITERATIONS: usize = 10;
    let mut mcp_timings = Vec::new();

    println!("Benchmarking MCP calls ({} iterations)...", MCP_ITERATIONS);

    let config = HashMap::from([(
        "local-memory".to_string(),
        McpServerConfig {
            command: "local-memory".to_string(),
            args: vec![],
            env: None,
            startup_timeout_ms: Some(5000),
        }
    )]);

    let (manager, _) = match McpConnectionManager::new(config, HashSet::new()).await {
        Ok(result) => result,
        Err(e) => {
            println!("  MCP initialization failed: {}", e);
            println!("  Skipping MCP benchmark");
            return;
        }
    };

    for _ in 0..MCP_ITERATIONS {
        let args = serde_json::json!({
            "query": "test",
            "limit": 5,
            "search_type": "hybrid"
        });

        let start = Instant::now();
        match manager.call_tool(
            "local-memory",
            "search",
            Some(args),
            Some(std::time::Duration::from_secs(10))
        ).await {
            Ok(_) => {
                mcp_timings.push(start.elapsed());
            }
            Err(e) => {
                println!("  MCP call failed: {}", e);
            }
        }
    }

    // Analysis
    if !subprocess_timings.is_empty() {
        let subprocess_avg = subprocess_timings.iter().sum::<std::time::Duration>() / subprocess_timings.len() as u32;
        println!("\nSubprocess Results:");
        println!("  Average: {:?}", subprocess_avg);
        println!("  Min: {:?}", subprocess_timings.iter().min().unwrap());
        println!("  Max: {:?}", subprocess_timings.iter().max().unwrap());

        if !mcp_timings.is_empty() {
            let mcp_avg = mcp_timings.iter().sum::<std::time::Duration>() / mcp_timings.len() as u32;
            println!("\nMCP Results:");
            println!("  Average: {:?}", mcp_avg);
            println!("  Min: {:?}", mcp_timings.iter().min().unwrap());
            println!("  Max: {:?}", mcp_timings.iter().max().unwrap());

            let speedup = subprocess_avg.as_secs_f64() / mcp_avg.as_secs_f64();
            println!("\nSpeedup: {:.2}x", speedup);

            if speedup >= 5.0 {
                println!("  ✓ MCP is significantly faster (5x+)");
            } else if speedup >= 2.0 {
                println!("  ✓ MCP is faster (2-5x)");
            } else if speedup >= 1.2 {
                println!("  ≈ MCP is marginally faster");
            } else {
                println!("  ⚠ WARNING: MCP not faster than subprocess");
            }
        }
    }
}
