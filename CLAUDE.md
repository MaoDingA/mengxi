# Mengxi — Project Context for AI Agents

Three-language monorepo: Rust CLI + MoonBit core algorithms + Python AI subsystem.

## Build

```bash
./build_moonbit.sh          # Step 1: MoonBit → libmoonbit_core.a (MUST run first)
cargo build --release       # Step 2: Rust binary (links static lib)
cargo test                  # 351 Rust tests
```

Requires: Rust nightly, MoonBit v0.8.x. Python 3.11+ is optional (runtime subprocess only).

## Architecture

```
crates/cli    → binary "mengxi" (clap 4.5, 9 subcommands)
crates/core   → library "mengxi-core" (rusqlite 0.38, FFI bridge, Python bridge)
crates/format → library "mengxi-format" (DPX, EXR, MOV, LUT parsers)
moonbit/src/  → pure functions compiled to libmoonbit_core.a
python/mengxi_ai/ → ONNX inference subprocess (stdin/stdout JSON-RPC)
```

Dependency direction: cli → core → format. Core owns the sole DB connection. Python never accesses DB directly.

## FFI (Rust ↔ MoonBit)

- `moonbit/src/lib/ffi.c` is **hand-written**, NOT auto-generated
- Data crossing FFI: only numeric arrays + integer tags — no pixel data, no heap objects, no strings
- Color space tags: `ColorSpaceTag` Linear=0, Log=1, Video=2 | `ACESColorSpace` AP0=10, AP1=11, ACEScct=12, Rec709=20 — do NOT conflate
- Output via pre-allocated buffers (caller provides `out_ptr`, `out_len`); negative return = error
- `build.rs` silently skips linking if `libmoonbit_core.a` missing — no compile error, runtime crash
- To add FFI function: update MoonBit source + `ffi.c` + Rust `extern "C"` + rebuild via `build_moonbit.sh`

## Python Subprocess

- Spawned on first AI command via `python -m mengxi_ai`, not at startup
- JSON-RPC: `{"request_id": "uuid", "method": "...", "params": {...}}` → `{"request_id": "...", "result": {...}}`
- Managed by `PythonBridge` in `python_bridge.rs` — idle timeout 300s, auto-restart on crash

## Error Handling

Each module defines `XxxError` enum with Display format `CATEGORY_DETAIL -- message` (e.g., `FINGERPRINT_FFI_ERROR`, `AI_TIMEOUT`). All implement `std::error::Error`. CLI uses `unwrap_or_else` with defaults for graceful degradation.

## Database

SQLite via rusqlite bundled. WAL mode. 16 numbered migrations in `migrations/`. Timestamps as `i64` Unix epoch. Embedding vectors as BLOB. Config at `~/.mengxi/config` (TOML, single file, no env vars).

## Naming Conventions

- Rust: `snake_case` functions, `PascalCase` types, `XxxError` error enums
- MoonBit: `snake_case` functions, `PascalCase` types
- Python: PEP 8 `snake_case`
- DB tables: `snake_case` plural | columns: `snake_case` | indexes: `idx_{table}_{columns}`
- FFI exports: `mengxi_` prefix | CLI flags: `kebab-case` | JSON keys: `snake_case` | Error codes: `CATEGORY_DETAIL`
- Migrations: `NNN_description.sql`

## Testing

- Rust unit: `#[cfg(test)] mod tests {}` in each file | Integration: `tests/` at workspace root
- Python: `python/tests/` with pytest
- Use `tempfile` for isolated test DBs; FFI tests use mocks/stubs
- Cover: happy path, error cases, edge cases (empty input, missing data)

## Anti-Patterns (DO NOT)

- Let pixel data cross FFI — only pre-computed arrays
- MoonBit functions that do I/O — pure functions only
- `camelCase` in DB/JSON — always `snake_case`
- Progress/logs to stdout — stderr only (stdout reserved for JSON mode)
- Python accessing DB, Format calling Core, env vars for config
- New FFI function without updating all 4 locations

## Cross-Layer Boundaries

| Boundary | Allowed | Forbidden |
|----------|---------|-----------|
| CLI → Core | Function calls | CLI cannot access DB/FFI directly |
| Core → Format | Function calls | Format cannot call Core |
| Core → MoonBit | Numeric arrays via FFI | MoonBit cannot do I/O |
| Core → Python | JSON stdin/stdout | Python cannot access DB |
| Format → FS | Read-only | Never modify source files |
| Any → DB | Core only | Others never access SQLite |

## Security

Fully offline. Source files read-only. No auto-overwrite. Python subprocess same-user permissions.
