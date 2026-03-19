# Mengxi (梦溪)

A CLI-based color pipeline management platform for professional film/TV post-production (Digital Intermediate / DI) workflows. Mengxi helps colorists search their historical project library via image-based similarity matching and export matching styles as LUT files for DaVinci Resolve.

## The Problem

When a director describes a desired visual tone, a colorist traditionally must manually translate that into technical parameters — a ~30-minute creative bottleneck per session. No existing tool (including DaVinci Resolve's own Project Library, Gallery, and PowerGrade features) provides cross-project color style search or semantic retrieval.

**Mengxi reduces tone-setting time from ~30 minutes to under 1 minute.**

## Features

- **Project Import** — Import DPX/EXR/MOV project folders with automatic format detection, keyframe extraction, and color fingerprint extraction
- **Color Fingerprint Extraction** — Extract rich color metadata (histograms, color space distribution, keyframe characteristics) into a local embedded database
- **Image-Based Similarity Search** — Upload a reference image and receive top-N ranked matching results using histogram matching and AI embeddings
- **LUT Export** — Export matching styles as `.cube`, `.3dl`, `.look`, `.csp`, and ASC-CDL format LUT files, loadable directly in DaVinci Resolve
- **LUT Version Control** — Diff comparison between LUT files and dependency tracking
- **Human-AI Tag Calibration** — AI-generated semantic tags with colorist correction feedback loop
- **CLI Interface** — 9 commands (`import`, `search`, `export`, `info`, `tag`, `lut-diff`, `lut-dep`, `stats`, `config`) with interactive and scripted modes

## Architecture

Mengxi uses a three-layer language architecture, each chosen for its strengths:

```
┌─────────────────────────────────────────────────┐
│  Rust — CLI Shell, System I/O, FFI Bridge       │
│  clap · rusqlite · dpx/openexr crates            │
├─────────────────────────────────────────────────┤
│  MoonBit — Core Algorithms                       │
│  ACES 1.3 · Color Science · LUT Engine          │
│  Type-safe color spaces (compile-time safety)    │
├─────────────────────────────────────────────────┤
│  Python — AI Inference (optional subprocess)     │
│  ONNX Runtime · Embedding · Tag Generation       │
└─────────────────────────────────────────────────┘
```

- **Rust** handles CLI, file format decoding (DPX/EXR/MOV), database operations, and manages the Python AI subprocess
- **MoonBit** implements pure color science functions — no I/O, no state, all interfaces are numeric arrays in/out, ensuring testability
- **Python** runs as a long-lived subprocess for AI-enhanced features (embedding generation, tag prediction). The core loop works without Python — Rust + MoonBit alone deliver import, fingerprint, histogram search, and LUT export

### Key Design Decisions

- **FFI boundary**: Image pixel data never crosses FFI — only pre-computed numeric arrays
- **Type-safe color spaces**: MoonBit's type system enforces Linear/Log/Video distinction at compile time, preventing an entire class of color science bugs
- **Embedded SQLite**: Single-file database with WAL mode, zero external dependencies
- **Python is optional**: AI features degrade gracefully; the tool is fully functional without a Python environment

## Project Structure

```
mengxi/
├── Cargo.toml              # Rust workspace root
├── build.rs                # Links libmoonbit_core.a via FFI
├── migrations/             # SQL migration files
├── crates/
│   ├── cli/                # CLI entry point (9 subcommands)
│   ├── core/               # Domain logic, DB, Python bridge, analytics
│   └── format/             # Format decoders (DPX, EXR, MOV, LUT, PowerGrade)
├── moonbit/                # MoonBit core algorithms
│   └── src/                # color_science, fingerprint, similarity, lut_engine, types
├── python/                 # AI inference subprocess
│   └── mengxi_ai/          # main.py, embedding.py, tagging.py, models.py
└── tests/                  # Integration tests + fixtures
```

## Development Status

**Planning complete, implementation in progress.**

- [x] Product Requirements Document
- [x] Architecture Design
- [x] Epics & Stories (5 epics, 21 stories)
- [ ] Sprint 1: CLI Foundation & Project Import
- [ ] Sprint 2: Fingerprint Engine & Search
- [ ] Sprint 3: LUT Engine & Export
- [ ] Sprint 4: AI-Enhanced Tags & Calibration
- [ ] Sprint 5: Analytics & Reporting

## Quick Start

> _Prerequisites: [Rust](https://rustup.rs/) nightly, [MoonBit](https://moonbitlang.com/) toolchain (v0.8.x), Python 3.11+ (optional, for AI features)_

```bash
# Clone the repository
git clone https://github.com/MaoDingA/mengxi.git
cd mengxi

# Build
cargo build --release

# Run
cargo run -- import /path/to/project
cargo run -- search /path/to/reference.png --top 5
cargo run -- export --match 1 --format cube --output style.cube
```

## Roadmap

| Phase | Focus |
|-------|-------|
| **MVP** (4 weeks) | Core 7 features — import, fingerprint, search, export, LUT diff, tag calibration, CLI |
| **Growth** (Month 2–6) | Natural language search, incremental indexing, gRPC DaVinci integration, TUI dashboard |
| **Expansion** (Month 6–12+) | GUI interface, style analysis, DIT on-set integration, streaming platform audit |

## Contributing

Contributions are welcome. Please follow these steps:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/your-feature`)
3. Make your changes and write tests
4. Ensure all tests pass (`cargo test`)
5. Open a Pull Request

## License

This project is licensed under the [MIT License](LICENSE).

## Author

**毛丁 (Mao Ding)** — Colorist with deep domain expertise on major Chinese productions including *The Wandering Earth 2*, *Lost in the Stars*, and *Love Game in Eastern Fantasy*.
