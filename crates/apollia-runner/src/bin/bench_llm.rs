//! `bench_llm`: micro-benchmark for the local llama.cpp inference path.
//!
//! Drives [`LlamaCppBackend`] in-process (no HTTP/sidecar overhead) to measure
//! the engine itself: single-request latency, time-to-first-token, multi-turn
//! prefix reuse, and concurrent throughput scaling across slot counts.
//!
//! Build in RELEASE with the platform backend feature, then run:
//!
//! ```sh
//! cargo run -p apollia-runner --release --features local-metal --bin bench_llm
//! cargo run -p apollia-runner --release --features local-cuda  --bin bench_llm
//! # optional: pass explicit model paths, else the two defaults are used
//! cargo run -p apollia-runner --release --features local-metal --bin bench_llm -- /path/a.gguf /path/b.gguf
//! ```
//!
//! Env knobs: `BENCH_MAX_TOKENS` (128), `BENCH_CONCURRENCY` (8), `BENCH_NCTX` (4096).

#[cfg(not(feature = "local-cpu"))]
fn main() {
    eprintln!(
        "bench_llm needs a local backend feature. Rebuild with one of:\n  \
         --features local-metal  (Apple Silicon)\n  \
         --features local-cuda   (NVIDIA)\n  \
         --features local-cpu    (CPU only)"
    );
    std::process::exit(2);
}

#[cfg(feature = "local-cpu")]
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if let Err(e) = imp::run().await {
        eprintln!("bench_llm error: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "local-cpu")]
mod imp {
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use futures::StreamExt;

    use apollia_runner::backends::llama_cpp::LlamaCppBackend;
    use apollia_runner::ipc::{ChatMessage, CompleteParams, LoadModelParams, Role};

    const DEFAULT_MODELS: &[&str] = &[
        "/Users/nidalzoumita/.apollia/models/Mistral-7B-Instruct-v0.3.Q4_K_M.gguf",
        "/Users/nidalzoumita/.apollia/models/Qwen3-30B-A3B.Q6_K.gguf",
    ];

    fn env_u32(key: &str, default: u32) -> u32 {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn backend_label() -> &'static str {
        if cfg!(feature = "local-cuda") {
            "cuda"
        } else if cfg!(feature = "local-metal") {
            "metal"
        } else if cfg!(feature = "local-rocm") {
            "rocm"
        } else if cfg!(feature = "local-vulkan") {
            "vulkan"
        } else {
            "cpu"
        }
    }

    /// A single-turn user prompt, greedy-decoded for reproducible timing.
    fn user_turn(model_id: &str, content: String, max_tokens: u32) -> CompleteParams {
        CompleteParams {
            model_id: model_id.to_string(),
            messages: vec![ChatMessage {
                role: Role::User,
                content,
            }],
            max_tokens,
            temperature: 0.0,
            top_p: 0.95,
            top_k: 40,
            repeat_penalty: 1.1,
            seed: Some(42),
            stop: Vec::new(),
            tools: None,
            grammar: None,
        }
    }

    /// A few hundred tokens of context so prefill is measurable.
    fn long_context() -> String {
        let unit = "Apollia is a sovereign local runtime for autonomous AI agents. \
                    It runs Python agents in isolation with tools and no cloud dependency. ";
        let mut s = String::new();
        for _ in 0..40 {
            s.push_str(unit);
        }
        s.push_str("\n\nSummarize the text above in exactly three sentences.");
        s
    }

    pub async fn run() -> Result<(), String> {
        let max_tokens = env_u32("BENCH_MAX_TOKENS", 128);
        let concurrency = env_u32("BENCH_CONCURRENCY", 8);
        let n_ctx = env_u32("BENCH_NCTX", 4096);

        let models: Vec<PathBuf> = {
            let args: Vec<String> = std::env::args().skip(1).collect();
            if args.is_empty() {
                DEFAULT_MODELS.iter().map(|s| PathBuf::from(*s)).collect()
            } else {
                args.into_iter().map(PathBuf::from).collect()
            }
        };

        println!("bench_llm  backend={}  n_ctx={n_ctx}  max_tokens={max_tokens}  concurrency={concurrency}", backend_label());
        println!("(run with --release for meaningful numbers)\n");

        for path in &models {
            if !path.exists() {
                println!("== {}  [SKIP: not found]\n", path.display());
                continue;
            }
            if let Err(e) = bench_model(path, n_ctx, max_tokens, concurrency).await {
                println!("== {}  [ERROR: {e}]\n", path.display());
            }
        }
        Ok(())
    }

    async fn bench_model(
        path: &Path,
        n_ctx: u32,
        max_tokens: u32,
        concurrency: u32,
    ) -> Result<(), String> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        println!("== {name}  (backend={})", backend_label());

        let backend = LlamaCppBackend::new();

        // --- load (single slot) for the latency/TTFT/multi-turn scenarios ---
        let load_started = Instant::now();
        let loaded = backend
            .load_model(load_params("bench-s1", path, n_ctx, 1))
            .await
            .map_err(|e| e.message)?;
        println!(
            "   load: {} ms   effective_ctx={}   mem_file={} MB   arch={}",
            load_started.elapsed().as_millis(),
            loaded.effective_ctx,
            loaded.memory_used_mb,
            loaded.arch
        );

        // Warmup (kernel compile + first prefill) so timings are steady-state.
        let _ = backend
            .complete(user_turn("bench-s1", "Say hello.".to_string(), 8))
            .await
            .map_err(|e| e.message)?;

        // --- single-request latency ---
        let ctx = long_context();
        let out = backend
            .complete(user_turn("bench-s1", ctx.clone(), max_tokens))
            .await
            .map_err(|e| e.message)?;
        let decode_tps = tps(out.usage.completion_tokens, out.timing.decode_ms);
        println!(
            "   [single]    prompt_tok={}  completion_tok={}  prefill={} ms  decode={} ms  decode_tps={decode_tps:.1}",
            out.usage.prompt_tokens, out.usage.completion_tokens, out.timing.prefill_ms, out.timing.decode_ms
        );

        // --- time to first token (streaming) ---
        let (ttft_ms, total_ms) =
            measure_ttft(&backend, "bench-s1", ctx.clone(), max_tokens).await?;
        println!("   [ttft]      first_token={ttft_ms} ms  total={total_ms} ms");

        // --- multi-turn prefix reuse ---
        let turn1 = backend
            .complete(user_turn("bench-s1", ctx.clone(), 48))
            .await
            .map_err(|e| e.message)?;
        let turn2_msgs = vec![
            ChatMessage {
                role: Role::User,
                content: ctx.clone(),
            },
            ChatMessage {
                role: Role::Assistant,
                content: turn1.text.clone(),
            },
            ChatMessage {
                role: Role::User,
                content: "Now expand the second sentence.".to_string(),
            },
        ];
        let turn2 = backend
            .complete(CompleteParams {
                messages: turn2_msgs,
                ..user_turn("bench-s1", String::new(), 48)
            })
            .await
            .map_err(|e| e.message)?;
        println!(
            "   [multiturn] turn1_prefill={} ms   turn2_prefill={} ms   (lower turn2 = prefix reuse working)",
            turn1.timing.prefill_ms, turn2.timing.prefill_ms
        );

        backend.unload("bench-s1");

        // --- concurrency scaling: slot_count 1 vs 4 ---
        println!("   [concurrency K={concurrency}]");
        let mut baseline_wall: Option<u128> = None;
        for slots in [1u32, 4u32] {
            let model_id = format!("bench-c{slots}");
            backend
                .load_model(load_params(&model_id, path, n_ctx, slots))
                .await
                .map_err(|e| e.message)?;
            // Warmup one slot.
            let _ = backend
                .complete(user_turn(&model_id, "Hi.".to_string(), 8))
                .await
                .map_err(|e| e.message)?;

            let wall_started = Instant::now();
            let futures = (0..concurrency).map(|i| {
                let prompt = format!(
                    "Task {i}: explain concept number {i} about local AI inference in one short paragraph."
                );
                backend.complete(user_turn(&model_id, prompt, max_tokens))
            });
            let results = futures::future::join_all(futures).await;
            let wall = wall_started.elapsed().as_millis();

            let mut total_completion = 0u32;
            let mut errors = 0u32;
            for r in &results {
                match r {
                    Ok(o) => total_completion += o.usage.completion_tokens,
                    Err(_) => errors += 1,
                }
            }
            let agg_tps = tps(total_completion, wall as u64);
            let speedup = baseline_wall
                .map(|b| format!("  speedup={:.2}x", b as f64 / wall.max(1) as f64))
                .unwrap_or_default();
            if slots == 1 {
                baseline_wall = Some(wall);
            }
            println!(
                "     slots={slots}: wall={wall} ms  agg_completion_tok={total_completion}  agg_tps={agg_tps:.1}{}{}",
                if errors > 0 { format!("  errors={errors}") } else { String::new() },
                speedup
            );
            backend.unload(&model_id);
        }

        println!();
        Ok(())
    }

    fn load_params(model_id: &str, path: &Path, n_ctx: u32, slot_count: u32) -> LoadModelParams {
        LoadModelParams {
            model_id: model_id.to_string(),
            model_path: path.to_path_buf(),
            n_ctx,
            n_gpu_layers: -1,
            use_mmap: true,
            use_mlock: false,
            slot_count,
            kv_cache_type: std::env::var("BENCH_KV_TYPE").ok(),
        }
    }

    async fn measure_ttft(
        backend: &LlamaCppBackend,
        model_id: &str,
        prompt: String,
        max_tokens: u32,
    ) -> Result<(u128, u128), String> {
        let started = Instant::now();
        let mut stream = backend
            .stream(user_turn(model_id, prompt, max_tokens))
            .await
            .map_err(|e| e.message)?;
        let mut first: Option<u128> = None;
        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| e.message)?;
            if first.is_none() && !chunk.text.is_empty() {
                first = Some(started.elapsed().as_millis());
            }
        }
        Ok((first.unwrap_or(0), started.elapsed().as_millis()))
    }

    /// Tokens per second from a token count and a millisecond duration.
    fn tps(tokens: u32, ms: u64) -> f64 {
        if ms == 0 {
            0.0
        } else {
            tokens as f64 / (ms as f64 / 1000.0)
        }
    }
}
