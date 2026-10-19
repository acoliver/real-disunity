# Build and validation

Install the pinned Rust toolchain and components using rustup, then use the workspace task runner:

```sh
cargo xtask ci
```

The task runner resolves its root from its own manifest, so commands work regardless of the caller's current directory. Quality commands are read-only: formatting uses `--check`, and policy checks never generate baselines or rewrite source.

Useful focused commands include `cargo xtask fmt`, `cargo xtask lint`, `cargo xtask complexity`, `cargo xtask policy`, and `cargo xtask source-size`. `cargo xtask check`, `build`, and `test` use `--locked`, workspace scope, all targets, and all features; tests additionally run doctests.

The policy gate parses Rust sources and package manifests instead of relying on a textual search. It rejects process-environment authority in compiled Rust, dependencies from workspace crates to xtask, and graphics dependencies in packages identified as CPU/loader packages. The policy scan fails when its crate/source roots are absent or unreadable. Update policy tooling and add behavioral tests when workspace architecture adds constraints; do not silently skip checks.

CI runs CPU quality/build/test workflows on Linux, Windows, and macOS, and a separate Linux software-Vulkan workflow for graphics-feature execution. Native graphics support on contributor machines may require system Vulkan libraries and a suitable driver.

## Provenance and licensing

The workspace is marked proprietary and non-publishable. No license is granted by this build guide. Before distributing derived binaries or source, perform a documented provenance and license review. A clean build, a fresh repository, and passing quality checks are not evidence of independent authorship or of rights to distribute dependencies.
