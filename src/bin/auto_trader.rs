use anyhow::Result;
use std::process::Command;
use chrono::Local;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 Starting Full Automated Trading System Lifecycle...");
    let start_time = Local::now();
    println!("🕒 Current Time: {}", start_time.format("%Y-%m-%d %H:%M:%S"));

    // Phase 1 & 2: Data Collection & Incremental Update (including Yahoo/J-Quants hybrid)
    println!("\n--- [Phase 1 & 2] Market Data Sync & Incremental Update ---");
    run_bin("sync_yahoo").await?;

    // Phase 3 & 4: AI Qualitative Analysis & Execution
    // ai_scout.rs already implements:
    // 1. Technical screening
    // 2. Selective news scraping
    // 3. Ollama (Gemma 3) qualitative analysis
    // 4. Discord notification
    // 5. Paper trade recording
    println!("\n--- [Phase 3 & 4] AI Qualitative Analysis & Execution ---");
    run_bin("ai_scout").await?;

    let end_time = Local::now();
    let duration = end_time - start_time;
    println!("\n🏁 Full Automated Trading Cycle Completed in {} min {} sec.", 
        duration.num_minutes(), duration.num_seconds() % 60);

    Ok(())
}

async fn run_bin(bin_name: &str) -> Result<()> {
    println!("📦 Executing: cargo run --release --bin {}", bin_name);
    
    let status = Command::new("cargo")
        .args(["run", "--release", "--bin", bin_name])
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!("Binary {} failed with status {}", bin_name, status));
    }
    
    Ok(())
}
