# Compatibility traceability checker

The catalog is `fixtures/compatibility-catalog.json`. Its authoritative populations come from `docs/unity-compatibility-kit-cases.md` and `docs/training-only-unity6-reference/game-comparison-inventory.json`.

It retains 54 definitions (50 compatibility and 4 harness), 95 gameplay requirement instances (43 Breakout, 25 Snake, 27 Pong), and the separate 5 historical tooling requirements and 11 historical test scripts. All 95 gameplay requirements have reviewed bounded executable mappings. Mapping completeness does not approve the full requirement. Each claim names an executable test/check or an indexed fixture assertion and states its remaining obligation.

## Commands

Run from `/home/acoliver/projects/real-disunity/branch-1`:

```sh
source .session/env.sh
python3 -B -m unittest discover -s tools/compat-traceability -v
python3 -B tools/compat-traceability/check.py --gate integrity
python3 -B tools/compat-traceability/check.py --gate required
python3 -B tools/compat-traceability/recheck_games.py
```

The checker prints JSON. Integrity returns 0 for valid mappings/provenance and 2 for invalid catalog/evidence. The required gate returns 1 for any required nonpass resolution. It currently returns 1. These are this Python checker's exit codes; they do not implement the full runner's infrastructure/deadline exit taxonomy.

`recheck_games.py` invokes the existing public game artifact verifiers in isolated Python children with 180-second child timeouts. It reads existing records, source/compiler proofs, state and pixels. It starts no game worker, build, desktop or browser. Its result establishes recorded-version revalidation only. Pillow is required by the existing Pong/Breakout verifiers and was available in the sourced environment.

The three bounded package metadata probes can be invoked separately:

```sh
python3 -B tools/compat-traceability/package_checks.py breakout_urp
python3 -B tools/compat-traceability/package_checks.py breakout_input
python3 -B tools/compat-traceability/package_checks.py snake_tooling
```

Their `metadata: pass` means the original manifest declares the expected package version. Their `provider: unmeasured` prevents interpreting that observation as package execution or pipeline fidelity.

## Validation and evidence limits

The checker rejects changed/unknown/duplicate/missing case and requirement identities, wrong declaration links, missing or substituted reviewed executable mappings, unreviewed commands, unknown test symbols, wrong assertion indices/payloads, stale source/log/artifact hashes, path escapes, mismatched compiler inputs and relabelled run profiles/assertion totals. Fixture claims require their selected validated run proof.

Selected game run proofs validate the recorded source revision, actual original compiler source bytes, before/after source inventories, exact compiler input manifest, run-local game assembly, selected managed/native images, timeline/profile/fidelity, fixture assertion payloads, capture hashes/dimensions and teardown. Engine source can evolve independently. Each recorded run continues to identify its selected provider. Code-location hashes describe the current mapped test/check source. Historical CI markers carry `source_binding: unverified-historical`, because that log has no per-test-source build attestation.

The 23 selected gameplay lanes retain native provider `92d102b8f83e31bdd1bc9814eacd0aa0c53dda141509e779586474054bab2657`. The separately staged handshake provider `3be201a216639e9bf00d922d7c31469b75f2c966bfcb686b911a3bdfb6ccbe9f` has its own acceptance under `.session/resumed-handshake-status.md`. This checker grants it no gameplay pass.

`build_catalog.py` materializes the reviewed mappings and reads available artifact identities. After legitimate mapped-source changes, review the affected assertions and regenerate with `python3 -B tools/compat-traceability/build_catalog.py`. Regeneration refreshes location hashes; it does not execute Rust/C# tests or promote historical results. Changed game fixtures or original game bytes require matching newly qualified runs before replacing selected proofs. Preserve earlier evidence separately.

All current catalog resolutions remain nonpassing. The JSON report includes `full_spec.completion: partial` with remaining obligations. Pending semantics, original missing sprites/font, original URP fidelity and package-provider gaps remain visible. Result statuses use the nine named states from REQ-10; absent execution is `unmeasured`. This catalog checker does not replace the existing compat-kit runner/dashboard, independent acceptance decisions, OS confinement evidence or current-code integration tests.

The 31,454 assertions across selected runs, 35 unchanged gameplay/support source files, executable location counts, source API member-reference census and 54/95 definition/requirement counts are different measures. No global engine support percentage is calculated.

## Maintainer boundaries

`model.py` imports populations and definitions. `claims.py` and `locations.py` own reviewed bounded mappings. `build_catalog.py` materializes the catalog. `mappings.py`, `references.py` and `records.py` validate links, executable identities and artifacts. `reporting.py` reports acceptance limits. `check.py` is the read-only CLI. `test_*.py` exercise its supported validation interfaces with isolated project-owned temporary files.
