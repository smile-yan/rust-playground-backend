use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::{error, warn};
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimitsBuilder};

const MEMORY_LIMIT_BYTES: usize = 256 * 1024 * 1024; // 256 MB
const EXECUTION_TIMEOUT_SECONDS: u64 = 5;
const EPOCH_DEADLINE_TICKS: u64 = 1;

pub struct SandboxOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

/// Holds both the WASI context and the resource limiter for a sandboxed run.
struct SandboxState {
    wasi: wasmtime_wasi::preview1::WasiP1Ctx,
    limits: wasmtime::StoreLimits,
}

/// Runs the provided WASM module inside a sandbox.
///
/// This function is async on the surface but moves the actual execution to a
/// dedicated blocking thread via `tokio::task::spawn_blocking`. This keeps the
/// Tokio runtime responsive and lets us use wasmtime's synchronous WASI API
/// together with a host thread that increments the engine epoch on timeout.
pub async fn run_wasm(wasm_bytes: &[u8]) -> anyhow::Result<SandboxOutput> {
    let bytes = wasm_bytes.to_vec();
    tokio::task::spawn_blocking(move || run_wasm_sync(&bytes)).await?
}

fn run_wasm_sync(wasm_bytes: &[u8]) -> anyhow::Result<SandboxOutput> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);

    let engine = Engine::new(&config)?;
    let mut linker = Linker::new(&engine);

    let stdout_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(64 * 1024);
    let stderr_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(64 * 1024);

    let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new()
        .stdout(stdout_pipe.clone())
        .stderr(stderr_pipe.clone())
        .build_p1();

    let limits = StoreLimitsBuilder::new()
        .memory_size(MEMORY_LIMIT_BYTES)
        .trap_on_grow_failure(true)
        .build();

    let mut store = Store::new(
        &engine,
        SandboxState {
            wasi: wasi_ctx,
            limits,
        },
    );
    store.limiter(|state| &mut state.limits);

    wasmtime_wasi::preview1::add_to_linker_sync(
        &mut linker,
        |state: &mut SandboxState| &mut state.wasi,
    )?;

    store.set_epoch_deadline(EPOCH_DEADLINE_TICKS);

    let module = Module::new(&engine, wasm_bytes)?;
    let instance = linker.instantiate(&mut store, &module)?;

    let run_func = instance.get_typed_func::<(), ()>(&mut store, "_start")?;

    // Spawn a host watchdog thread that interrupts execution on timeout.
    let timed_out = Arc::new(AtomicBool::new(false));
    let timed_out_for_watchdog = timed_out.clone();
    let engine_for_watchdog = engine.clone();
    let watchdog = thread::spawn(move || {
        thread::sleep(Duration::from_secs(EXECUTION_TIMEOUT_SECONDS));
        warn!("Execution exceeded {} seconds, incrementing epoch", EXECUTION_TIMEOUT_SECONDS);
        timed_out_for_watchdog.store(true, Ordering::SeqCst);
        engine_for_watchdog.increment_epoch();
    });

    let execution_result = run_func.call(&mut store, ());

    // Execution finished: signal the watchdog to exit early.
    // We can't safely cancel a std::thread, but we can detach it. The epoch
    // increment after the request has already completed is harmless because
    // each request creates a fresh `Engine`.
    let _ = watchdog;

    let stdout = String::from_utf8_lossy(&stdout_pipe.contents()).to_string();
    let stderr = String::from_utf8_lossy(&stderr_pipe.contents()).to_string();

    match execution_result {
        Ok(()) => Ok(SandboxOutput {
            success: true,
            stdout,
            stderr,
            error: None,
        }),
        Err(e) => {
            error!("WASM execution error: {:#}", e);
            let msg = e.to_string();
            let lower = msg.to_lowercase();
            let error_msg = if timed_out.load(Ordering::SeqCst) {
                "Execution timed out (exceeded 5 seconds)".to_string()
            } else if lower.contains("memory") || lower.contains("growing memory") {
                format!("Memory limit exceeded (max {} MB)", MEMORY_LIMIT_BYTES / 1024 / 1024)
            } else {
                format!("Runtime error: {}", e)
            };
            Ok(SandboxOutput {
                success: false,
                stdout,
                stderr,
                error: Some(error_msg),
            })
        }
    }
}
