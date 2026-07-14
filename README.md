# Rust Playground (Backend)

A Rust backend service that receives user-submitted Rust code, compiles it to
WebAssembly, and executes it inside a locked-down [wasmtime](https://wasmtime.dev/)
sandbox.

## Features

- Axum-based HTTP API at `POST /api/run`
- `rustc` compiles user code to `wasm32-wasip1`
- wasmtime sandbox with:
  - 256 MB memory limit (StoreLimits + `trap_on_grow_failure`)
  - 5-second execution timeout via epoch interruption
  - No filesystem access
  - No network access
  - No environment variables
  - No subprocess spawning

## Project Structure

```
rust-playground/
├── Cargo.toml          # Backend dependencies
├── src/
│   ├── main.rs         # Axum server bootstrap
│   ├── api.rs          # HTTP route handlers
│   ├── compiler.rs     # rustc invocation
│   └── sandbox.rs      # wasmtime execution wrapper
└── README.md
```

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain, tested with 1.96+)
- The `wasm32-wasip1` target (newer name for `wasm32-wasi`):

```bash
rustup target add wasm32-wasip1
```

## Running the Server

```bash
cargo run
```

The server starts on `http://127.0.0.1:3000`.

## API

### `POST /api/run`

Request body:

```json
{
  "code": "fn main() { println!(\"Hello, WASM!\"); }"
}
```

Response body:

```json
{
  "success": true,
  "stdout": "Hello, WASM!\n",
  "stderr": "",
  "error": null
}
```

## Example

```bash
curl -X POST http://127.0.0.1:3000/api/run \
  -H "Content-Type: application/json" \
  -d '{"code":"fn main() { let result = (1..=100).sum::<i32>(); println!(\"Sum of 1 to 100: {}\", result); }"}'
```

Expected output:

```json
{
  "success": true,
  "stdout": "Sum of 1 to 100: 5050\n",
  "stderr": "",
  "error": null
}
```

## Security Notes

This is an MVP intended for learning and local use. Before exposing it to the
public internet, consider additional hardening:

- Rate limiting per IP/user
- Shorter maximum code length
- Container or VM-level isolation for the backend process
- OS-level resource quotas (CPU, memory) for the cargo/rustc process
- Input sanitization and abuse detection
- Do not run the compiler or wasmtime as root

### Known Limitations

- The 256 MB memory limit is enforced through wasmtime's `StoreLimits` on linear
  memory growth. It catches typical `malloc`/`realloc` growth paths, but very
  large single `calloc` allocations in wasi-libc may bypass the linear-memory
  grow path on some platforms. For stronger guarantees, run the whole service in
  a memory-capped container or cgroup.

## License

MIT
