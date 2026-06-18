//! Steady-State Mining Benchmark para eWatts Protocol
//!
//! Mede throughput real de MBPoW mining em estado estável (steady state).
//! Roda mining contínuo sem overhead de blockchain (sem state, sem disk I/O).
//!
//! Uso:
//!   cargo run --bin steady-bench -- [opts]
//!
//! Flags:
//!   --duration <s>        Duração do benchmark em segundos (default 60)
//!   --dag-size <mb>       Tamanho do DAG em MB (default 4 = testnet)
//!   --dag-cache           Salva/carrega DAG de disco ("ewatts_dag_cache_{size}mb.bin")
//!   --difficulty <n>      Dificuldade de mining (default 100)
//!   --threads <n>         Número de threads paralelas (default 1)
//!   --report-interval <s> Intervalo de relatório parcial (default 10)
//!
//! Exemplo:
//!   cargo run --bin steady-bench -- --duration 120 --dag-size 1024 --dag-cache

use std::io::Write;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

fn flush() {
    let _ = std::io::stdout().flush();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let duration_secs: u64 = parse_arg(&args, "--duration").unwrap_or(60);
    let dag_size_mb: u64 = parse_arg(&args, "--dag-size").unwrap_or(4);
    let difficulty: u64 = parse_arg(&args, "--difficulty").unwrap_or(100);
    let num_threads: usize = parse_arg(&args, "--threads").unwrap_or(1) as usize;
    let report_interval: u64 = parse_arg(&args, "--report-interval").unwrap_or(10);
    let use_dag_cache = args.iter().any(|s| s == "--dag-cache");

    let dag_size_bytes = dag_size_mb * 1024 * 1024;
    let cache_path = format!("ewatts_dag_cache_{}mb.bin", dag_size_mb);

    println!("╔══════════════════════════════════════════════════════╗");
    flush();
    println!("║   eWatts Steady-State Mining Benchmark              ║");
    println!("╠══════════════════════════════════════════════════════╣");
    println!("║  DAG size:      {} MB ({})", dag_size_mb, format_bytes(dag_size_bytes));
    println!("║  Difficulty:    {}", difficulty);
    println!("║  Duration:      {}s", duration_secs);
    println!("║  Threads:       {}", num_threads);
    println!("║  Report every:  {}s", report_interval);
    if use_dag_cache {
        println!("║  DAG cache:     {} ({} on disk)",
            cache_path,
            if std::path::Path::new(&cache_path).exists() { "found" } else { "will save" });
    }
    println!("╠══════════════════════════════════════════════════════╣");
    println!("║  Loading/generating DAG...                          ║");
    println!("╚══════════════════════════════════════════════════════╝");
    flush();

    // Load or generate DAG
    let dag_gen_start = Instant::now();
    let dag = if use_dag_cache && std::path::Path::new(&cache_path).exists() {
        println!("  Loading DAG from {}...", cache_path);
        flush();
        let raw = std::fs::read(&cache_path).expect("Failed to read DAG cache");
        let n = raw.len() / 64;
        let mut elements = Vec::with_capacity(n);
        for i in 0..n {
            let mut buf = [0u8; 64];
            buf.copy_from_slice(&raw[i*64..(i+1)*64]);
            elements.push(buf);
        }
        let elapsed = dag_gen_start.elapsed();
        println!("  Loaded {} elements in {:.3}s", n, elapsed.as_secs_f64());
        flush();
        ewatts_protocol::dag::Dag { elements, epoch: 0, size_bytes: dag_size_bytes }
    } else {
        println!("  Generating DAG ({} MB)...", dag_size_mb);
        flush();
        let dag = ewatts_protocol::dag::Dag::generate_with_size(0, dag_size_bytes);
        let elapsed = dag_gen_start.elapsed();
        println!("  DAG generated in {:.3}s ({} elements)", elapsed.as_secs_f64(), dag.len());
        flush();

        if use_dag_cache {
            println!("  Saving DAG to {}...", cache_path);
            flush();
            let mut raw = Vec::with_capacity(dag.elements.len() * 64);
            for elem in &dag.elements {
                raw.extend_from_slice(elem);
            }
            std::fs::write(&cache_path, &raw).expect("Failed to write DAG cache");
            println!("  Saved ({})", format_bytes(raw.len() as u64));
            flush();
        }
        dag
    };

    // Atomic counters
    let running = Arc::new(AtomicBool::new(true));
    let total_walks = Arc::new(AtomicU64::new(0));
    let total_solutions = Arc::new(AtomicU64::new(0));
    let total_elapsed_ms = Arc::new(AtomicU64::new(0));
    let dag = Arc::new(dag);

    let header_hash = [0xabu8; 32];

    println!("\n  Mining benchmark running for {}s...", duration_secs);
    println!("  Press Ctrl+C to stop early.\n");
    flush();

    let start = Instant::now();

    // Spawn miner threads
    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let running = running.clone();
        let total_walks = total_walks.clone();
        let total_solutions = total_solutions.clone();
        let total_elapsed_ms = total_elapsed_ms.clone();
        let dag = dag.clone();

        let handle = thread::spawn(move || {
            let nonce_limit = 50000u64;

            while running.load(Ordering::Relaxed) {
                let sol = ewatts_protocol::proof::mine(&header_hash, difficulty, dag.as_ref(), nonce_limit);

                let thread_walks = ewatts_protocol::proof::difficulty_to_accesses(difficulty);
                total_walks.fetch_add(thread_walks, Ordering::Relaxed);

                if let Some(s) = sol {
                    total_solutions.fetch_add(1, Ordering::Relaxed);
                    total_elapsed_ms.fetch_add(s.elapsed_ms, Ordering::Relaxed);
                }
            }
        });

        handles.push(handle);
    }

    // Reporter thread
    let reporter_running = running.clone();
    let reporter_walks = total_walks.clone();
    let reporter_solutions = total_solutions.clone();

    let reporter = thread::spawn(move || {
        let mut last_walks = 0u64;
        let mut last_time = Instant::now();

        while reporter_running.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(report_interval));

            let current_walks = reporter_walks.load(Ordering::Relaxed);
            let current_solutions = reporter_solutions.load(Ordering::Relaxed);
            let now = Instant::now();
            let interval_secs = now.duration_since(last_time).as_secs_f64();

            let delta_walks = current_walks - last_walks;

            if delta_walks > 0 {
                let bytes_per_access = 64u64;
                let gb_processed = (delta_walks as f64 * bytes_per_access as f64) / (1073741824.0);
                let gbps = gb_processed / interval_secs;

                let _ = writeln!(std::io::stdout(),
                    "  [{:4}s] Walks: {} ({}/s) | Solutions: {} | {:.2} GB/s",
                    now.duration_since(start).as_secs(),
                    current_walks,
                    delta_walks as f64 / interval_secs,
                    current_solutions,
                    gbps,
                );
                flush();
            }

            last_walks = current_walks;
            last_time = now;
        }
    });

    // Run for specified duration
    thread::sleep(Duration::from_secs(duration_secs));
    running.store(false, Ordering::Relaxed);

    // Wait for threads to finish
    for h in handles {
        let _ = h.join();
    }
    let _ = reporter.join();

    let elapsed = start.elapsed();
    let total_s = elapsed.as_secs_f64();

    // Compute aggregate metrics
    let walks = total_walks.load(Ordering::Relaxed);
    let solutions = total_solutions.load(Ordering::Relaxed);
    let mining_ms = total_elapsed_ms.load(Ordering::Relaxed);

    // Throughput computation
    let accesses_per_walk = ewatts_protocol::proof::difficulty_to_accesses(difficulty);
    let bytes_per_access = 64u64;
    let bytes_per_walk = accesses_per_walk * bytes_per_access;
    let total_bytes = walks as f64 * bytes_per_walk as f64;
    let total_gb = total_bytes / 1073741824.0;
    let avg_gbps = if total_s > 0.0 { total_gb / total_s } else { 0.0 };

    // Energy estimation using J_PER_GB
    let j_per_gb = ewatts_protocol::constants::J_PER_GB;
    let total_joules = total_gb * j_per_gb;
    let avg_watts = if total_s > 0.0 { total_joules / total_s } else { 0.0 };
    let kwh = total_joules / 3_600_000.0;

    let _ = writeln!(std::io::stdout(), "");
    let _ = writeln!(std::io::stdout(), "╔══════════════════════════════════════════════════════╗");
    let _ = writeln!(std::io::stdout(), "║               BENCHMARK RESULTS                     ║");
    let _ = writeln!(std::io::stdout(), "╠══════════════════════════════════════════════════════╣");
    let _ = writeln!(std::io::stdout(), "║  Duration:        {:.1}s", total_s);
    let _ = writeln!(std::io::stdout(), "║  Threads:         {}", num_threads);
    let _ = writeln!(std::io::stdout(), "║  DAG:             {} MB", dag_size_mb);
    let _ = writeln!(std::io::stdout(), "║  Difficulty:      {}", difficulty);
    let _ = writeln!(std::io::stdout(), "╠══════════════════════════════════════════════════════╣");
    let _ = writeln!(std::io::stdout(), "║  Total walks:     {}", walks);
    let _ = writeln!(std::io::stdout(), "║  Solutions:       {}", solutions);
    let _ = writeln!(std::io::stdout(), "║  Walk attempts/s: {:.0}", walks as f64 / total_s);
    let _ = writeln!(std::io::stdout(), "║  Avg walk time:   {:.1} ms",
        if walks > 0 { mining_ms as f64 / walks as f64 } else { 0.0 });
    let _ = writeln!(std::io::stdout(), "╠══════════════════════════════════════════════════════╣");
    let _ = writeln!(std::io::stdout(), "║  Total data:      {:.3} GB", total_gb);
    let _ = writeln!(std::io::stdout(), "║  Throughput:      {:.2} GB/s", avg_gbps);
    let _ = writeln!(std::io::stdout(), "╠══════════════════════════════════════════════════════╣");
    let _ = writeln!(std::io::stdout(), "║  Est. energy:     {:.2} J ({:.6} kWh)", total_joules, kwh);
    let _ = writeln!(std::io::stdout(), "║  Avg power:       {:.2} W", avg_watts);
    let _ = writeln!(std::io::stdout(), "║  J_PER_GB used:   {} J/GB", j_per_gb);
    let _ = writeln!(std::io::stdout(), "╚══════════════════════════════════════════════════════╝");
    flush();

    // VR implication
    if solutions > 0 {
        let avg_mining_ms = mining_ms as f64 / solutions as f64;
        let _ = writeln!(std::io::stdout(), "");
        let _ = writeln!(std::io::stdout(), "  VR implications (est. {:.2} W node):", avg_watts);
        let _ = writeln!(std::io::stdout(), "    Joules/block:  {:.1} J (avg {:.0}ms mining)", avg_mining_ms * avg_watts / 1000.0, avg_mining_ms);
        let _ = writeln!(std::io::stdout(), "    VR/block:      ~{:.3} kWh/Ewatt (theoretical)",
            avg_watts * (600.0) / 3_600_000.0);
        flush();
    }
}

fn parse_arg(args: &[String], flag: &str) -> Option<u64> {
    args.windows(2).find(|w| w[0] == flag).and_then(|w| w[1].parse().ok())
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1073741824 {
        format!("{:.2} GB", bytes as f64 / 1073741824.0)
    } else if bytes >= 1048576 {
        format!("{:.2} MB", bytes as f64 / 1048576.0)
    } else {
        format!("{} B", bytes)
    }
}
