# infinity-msfs-build-tools

A unified CLI for building MSFS 2024 add-ons: WASM gauges, native SimConnect
apps, JS/TS instruments, and full sim packages — all driven from a single
`infinity-msfs.toml`.

Full documentation, configuration reference, and examples live at
**<https://infinity-simulations.com/docs/developer/build-tools>**.

## Installation

Requires Rust 1.85+ and the `wasm32-wasip1` target. `wasm-opt` (Binaryen) is
recommended for release builds.

```sh
# From a clone of this repo
cargo install --path .

# Required for WASM gauge builds
rustup target add wasm32-wasip1

# Optional but recommended
# https://github.com/WebAssembly/binaryen/releases
```

Run the environment check to confirm everything is wired up:

```sh
infinity-msfs doctor
```

If the MSFS 2024 SDK isn't already installed, fetch the WASM/SimConnect
subset into a local cache:

```sh
infinity-msfs sdk install
```

## Quick start

```sh
infinity-msfs build --release        # cargo build → wasm-opt → copy
infinity-msfs build --js-only        # only the JS half of the pipeline
infinity-msfs package                # fspackagetool (Windows + sim required)
infinity-msfs watch --js             # rebuild on file change
```

See the docs site for the full configuration schema and per-command flags.

## License

MIT
