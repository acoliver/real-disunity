# Final artifact validation — training-only Unity reference

**Completed 2026-10-20, offline. Decision: retain as a bounded conceptual reference and source-fixture planning aid, not an implementation specification or Unity compatibility certification.** Artifact inventories, counts, local citation bounds and source identities reconcile. Four Stage 4 report locators were corrected without changing API text or comparison evidence. No unresolved structural discrepancy was found within the checks below. All 57 substantive uncertainty entries remain open.

## Completed work and provenance boundaries

1. Initial authoring produced a training-only, unofficial Unity 6 conceptual survey: 18 groups, six topics each, an index and an uncertainty register. Unity 6.3 is a desired target, not a verified API revision.
2. [Stage 2](review-stage2-corrections.md) recorded and applied 19 bounded correction units using generated text and training knowledge.
3. [Stage 3](review-stage3-completeness.md) assessed the post-Stage-2 reference and proposed 20 further units. Its assessments and proposed/unapplied wording remain historical.
4. [Stage 4](review-stage4-resolution.md) applied those same 20 units through group edits and one [supplement](stage4-supplement.md). The supplement adds zero inventory topics. Inclusion is not proof of correctness or family completeness.
5. [Stage 5](game-comparison.md) compared the written reference against three pinned local games. Its [JSON inventory](game-comparison-inventory.json) is authoritative for normalized requirement records and evidence. Client source establishes observed use, not engine truth.
6. This final pass validated existing artifacts, corrected report bookkeeping and appended navigation. It did not re-research Unity, add API fixes, alter games, or implement packages.

The stages describe fresh author/reviewer agent roles, not independent empirical verification. Fresh agents using the same models can have correlated training knowledge, omissions and errors. Role separation reduces direct conversational carryover but does not establish statistical independence, external ground truth, independent discovery, legal clearance or training-data provenance. Earlier-stage tool/provenance statements are preserved as historical assertions, not retrospectively certified execution logs.

## Reference and review counts

| Population | Errors | Omissions | Clarifications | Total / status |
|---|---:|---:|---:|---|
| Stage 2, C2-001–C2-019 | 3 | 8 | 8 | 19 applied |
| Stage 3, J3-001–J3-020 | 0 | 17 | 3 | 20 proposed at that stage |
| Stage 4 disposition of the same J3 units | 0 | 17 | 3 | All 20 applied; 0 partial/deferred/rejected |

Stage 4 is not another 20 distinct findings: Stages 2 and 3 together contain 39 correction units, of which 3 are categorized as errors, 25 as omissions and 11 as clarifications. Stage 2's evidence basis is 18 training judgments and one local bookkeeping/provenance reconciliation. Stage 3's 20 judgments combine training knowledge and local text observations. These are review categories, not externally verified Unity defect counts. **New R4 API-error findings: 0 recorded.** The four final locator repairs and seven Stage 5 follow-up proposals are not added to API-error counts.

- Inventory presence: **18/18 groups and 108/108 topics**, with unique ordered G01–G18 / T01–T06 identities matching the index's embedded JSON.
- Uncertainties: **57/57 entries U001–U057 remain unresolved/unverified; 0 closed**.
- Stage 3 historical topic grades: **63 substantial / 40 partial / 5 outline / 0 missing**; group grades: **7 substantial / 11 partial**. These were not reassessed after Stage 4 and are **not current post-Stage-4 depth grades**.
- Presence, qualitative depth, recalled correctness, source-requirement coverage and global completeness are different measurements. No global Unity coverage denominator or percentage is known.

## Game-comparison results

A requirement is a curated normalized Unity-facing declaration, behavior or minimal content/package obligation. Repeated uses merge within a game; materially different obligations remain separate. The combined denominator counts **game-requirement instances, not a deduplicated cross-game universe**. Covered means explicit bounded written treatment, not correct or runtime-verified behavior. Partial rows receive no fractional credit.

| Game population | Requirements | Covered | Partial | Absent | Ambiguous |
|---|---:|---:|---:|---:|---:|
| A — modern Breakout | 43 | 26 | 12 | 2 | 3 |
| B — modern Snake | 25 | 15 | 5 | 2 | 3 |
| C — historical Pong gameplay/support | 27 | 16 | 7 | 2 | 2 |
| **Gameplay/support instances** | **95** | **57** | **24** | **6** | **8** |
| Separate Pong test-harness population | 5 | 1 | 2 | 1 | 1 |

Thus 57/95 = **60.00% covered** within this bounded gameplay denominator. The separate five test obligations are excluded from 95. The inventory has **100 unique requirement IDs**, **39 guide-evidence records**, and **7 follow-up proposals**, with no dangling record references. Three-dimensional aggregate status rules and all stored per-game/overall arithmetic reconcile.

| Source | Saved editor | Verified pin | Gameplay/support C# | Test C# | Gameplay/support lines | Test lines |
|---|---|---|---:|---:|---:|---:|
| A | 6000.3.5f2 | `2b960a3589f30b6e0025f2509ef00bf9155d6f45` | 7 | 0 | 435 | 0 |
| B | 6000.3.7f1 | `0ab43c53e834db714439be71c4151d953d241847` | 8 | 0 | 284 | 0 |
| C | 2017.1.0f3 | `e2d3368728e236b0ab5a04afb142a3ae49260dd1` | 20 | 11 | 349 | 670 |
| **Total** | | | **35** | **11** | **1,068** | **670** |

Recursive Assets C# enumeration matches all **46** stored script paths and line counts. Comments/blanks are included; line totals are bookkeeping, not complexity or coverage metrics. All three source trees are clean under `git status --short`; pins match supplied and stored values. Saved editor strings and selected manifest/lock package versions match the inventory. Pong remains historical, not a Unity 6.3 fixture.

The comparison's conditional recommendation remains **B first, A second, C for historical regression/test design**. It does not promise runnable unmodified fixtures. Existing limitations include Snake's missing sprite reference and application-rule defects, Breakout's tag/stale-field/native physics/URP closure, and Pong's historical serialization/event/UI/test-plugin boundaries. Root MIT notices do not clear every asset, built-in, package or plugin; no independent rights clearance was performed.

## Validation methodology and actual checks

Only this generated directory and the supplied read-only A/B/C roots were used for document/source evidence. Tools were local file reads/writes, Python standard-library checking, read-only git metadata inspection, git status and diff whitespace checks. There was **no network or external-link fetching, external documentation/search research, dependency installation, compilation, build, Unity/game execution, package implementation, vendor/Unity implementation inspection or DLL-content inspection**. Existing asset/package URLs were not accessed.

Checks performed:

- Parse the standalone comparison JSON and the authoritative JSON fence in the index; reconcile IDs, ordered topic headings, uncertainty IDs, requirement uniqueness, dimensions, aggregates and finding references.
- Recount Stage 2/3 classifications from individual findings, Stage 4's 20 applied rows, and historical Stage 3 grade cells. Inspect actual Stage 4 group/supplement additions against the report's claims.
- Check **220 requirement-source citation ranges**, **28 other stored source/metadata citation ranges**, and all **39 guide-evidence ranges** for existing paths and 1-based inclusive bounds. Check **145 context-expanded comparison Markdown citation instances comprising 174 inclusive ranges**, plus two enabled-scene recovery-counterpart line bounds and the new report/summary locators. Shortened filenames, shared game references and bare line spans are resolved in context. Bounds checks establish locatability, not semantic correctness or exhaustive extraction.
- Check **80 local Markdown links and 60 paired code fences across 26 Markdown files**: no broken targets, unclosed fences, trailing whitespace or missing final newlines. There are zero external Markdown link targets in this scan; no external targets were fetched. This is a local structural check, not an external Markdown rendering certification.
- Match actual script paths, roles/populations and line counts; read local pins, saved editor strings and selected package metadata; check clean game working trees.
- Confirm that the 24 protected existing artifacts remain byte-for-byte unchanged from the start of this pass: all 18 group files, supplement, uncertainty register, Stage 2/3 reports, comparison Markdown and comparison JSON. Confirm the index's entire original 16,364-byte / 121-line prefix is unchanged; new links are appended after it. Existing API and comparison citation line numbers therefore remain stable.
- Run `git diff --check`. Because the generated directory is untracked, this command alone cannot check its contents; a separate Python scan checks generated-file trailing whitespace and Markdown structure.

These are artifact/inventory checks, **not behavioral tests of Unity or the games**. No API correctness tests were run or claimed.

## Exact discrepancy repairs and preserved history

Only the Stage 4 resolution report required existing-content edits:

- **J3-010:** replace the implied index-scope edit with the actual navigation addition at `stage4-supplement.md:35–37`; G07.T05 is conceptual context, not an edited location.
- **J3-013:** replace the implied index-scope edit with the actual video boundary at `stage4-supplement.md:47–49`; G09.T01/T04 are conceptual context.
- **J3-019:** the specific untracked Addressables-instance clarification is at `stage4-supplement.md:73`, not in G14.T05.
- **J3-020:** the specific property-block versus shader/render-state clarification is at `stage4-supplement.md:75`, not in G09.T03.

These are locator/bookkeeping discrepancies, not four additional false API claims. All 20 J3 additions are present at the bounded conceptual level recorded by Stage 4. No API text was rewritten to make the report appear correct. Stage 2/3 historical reports and game-comparison files remain unchanged. Earlier counts such as Stage 2's 21 Markdown files and Stage 4's 24 Markdown files describe their own completion points, not today's directory population.

## Final deliverables and remaining limits

The directory now contains **27 artifacts: 26 Markdown files and 1 standalone JSON file**. Breakdown: 18 API groups, 1 index, 1 uncertainty register, 3 review/resolution reports, 1 supplement, 1 game-comparison report, 1 game-comparison JSON, and this 1 final summary. The index also retains its embedded inventory JSON, which is not another file or artifact.

This pass changed **three files**: created `final-summary.md`, corrected/appended `review-stage4-resolution.md`, and appended summary/comparison/JSON links to `index.md`. No staging or commit occurred. Workspace status still includes the generated directory and the two pre-existing untracked proposal documents (`docs/unity-6.3-target-and-sample-research.md` and `docs/unity-compatibility-tck-proposal.md`); those proposals were not touched.

**Unresolved structural discrepancies within checked scope: none.** Substantive unknowns remain: true global completeness, exact signatures/overloads/aliases, Unity 6.3/package/platform/backend accuracy, native serialized/import/render/physics fidelity, source-fixture runtime behavior and performance, test-runner execution, full rights clearance and training provenance. All 57 U entries remain open. The artifacts are useful for selecting and specifying a small next contract, but do not establish full Unity accuracy, independently cleared provenance or a working compatible runtime.
