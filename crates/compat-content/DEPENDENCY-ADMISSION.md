# Receipt dependency admission

Hosts need a dependency decision before allocating native objects. `import_receipt` retains the source bytes and graph even when dependencies are unresolved. Its `dependency_references()` accessor and JSON proof expose original source locators, document class/file identity or metadata importer identity, signed file IDs, reference types, whole-file hashes and resolution observations. Original `$missing` edges remain in the graph.

The new public boundary is:

```rust,ignore
let receipt = import_receipt(root, scene, &profile, &sidecar)?;
let dependencies = DependencyPolicy::from_json(&policy_bytes)?;
let admitted = receipt.admit(&components, &dependencies, &trusted_providers)?;
admitted.verify_inputs(root, &components, &dependencies, &current_providers)?;
let materialized = admitted.materialization_plan().execute(request)?;
```

`TrustedProviderInventory` starts empty. Trusted host composition implements `TrustedProvider`, supplies actual implementation bytes and retained contract-test evidence bytes, and registers the provider. Registration computes both hashes. The inventory has no deserializer. Fixture JSON can request a capability through a dependency policy; it cannot supply provider readiness. Registration trusts the host's behavior qualification. This crate does not execute renderer or keyboard contract tests during registration.

`AdmittedContent` privately borrows the receipt and owns the validated plan, policies, provider binding snapshot and sealed proof. Its accessors return shared references. Admission performs no native requests. A failure returns `DependencyAdmissionError { message, proof }`, with a blocked proof and every observed dependency decision. The error exposes no executable admitted object.

## Rules and supported decisions

A rule matches one exact source path, owner, field-key/index sequence, reference GUID/fileID/type, resource kind and source SHA256. Rule IDs and locators must be unique. Unmatched/stale rules reject admission. Every rule requires evidence and a reason. Observed evidence files inside the receipt must match their hashes; external evidence paths/hashes remain reviewer attestations. They never establish provider readiness.

`DependencyPolicy::from_json` rejects duplicate keys before converting to JSON values, including nested duplicates. It rejects unknown fields/variants, unsupported schema/capability versions, invalid GUIDs/hashes, noncanonical paths, missing evidence/reasons, invalid owner identities and overflowing IDs. Constructing a policy directly in Rust still runs semantic validation during admission.

The implemented classifications are bounded:

* Exact reviewed PackageManager script13964 and multiplayer-roles script15023 settings records can be classified as editor configuration under explicit translated fidelity. Different owners, paths, fields, IDs and source bytes reject.
* Exact class30/fileID1 GraphicsSettings shader slots can be deactivated by a reviewed render-settings translation with a trusted `canonical_unlit_quads` version1/render-settings capability. The original shader remains unresolved. A material provider cannot satisfy that capability or a shader use.
* A builtin mapping requires an exact reviewed `ProvidedBuiltin` rule, the observed mapping name and a trusted provider of the observed resource kind. A familiar provider name, builtin-shaped GUID prefix or arbitrary `$builtin` string grants no exemption.
* `ImportToolNotExecuted` records the proposed importer decision and evidence but always blocks execution in this increment. A current compiled assembly/source-census/bounded keyboard API profile proof is still required. Wrapper generation, action evaluation and unsupported managed APIs receive no exemption.
* Unknown references and unclassified graph-projected reference markers block. Objectless material/scriptable-object records are included. Original source reserved `$ref`/`$builtin`/`$type`/`$missing` syntax cannot assert readiness. An available material asset cannot satisfy a shader slot. Raw shader source alone supplies no executable shader provider.
* `m_Material` on matching class/kind owners Rigidbody2D, BoxCollider2D, CircleCollider2D, PolygonCollider2D and TilemapCollider2D uses the distinct `physics_material_2d` resource kind. Resolution requires the exact target document key, class62 and kind `PhysicsMaterial2D`. The file extension alone cannot establish that identity. Rendering material and shader checks retain their separate requirements. Unknown resource-kind wire variants reject.

The ledger compares original and projected settings documents, asset metadata, environment metadata and asset documents using their original typed owner and field-key/index locations. It descends below reference-marker objects and into arrays, preserving each original reference alongside introduced nested markers. Metadata's internal settings envelope does not enter the source locator. Changed scalar, null, array or object projections cannot inherit availability from the old reference target. An explicit `$ref` must identify the original target or its actual expansion alias. Multiple reserved markers at one projected site block.

Stripped prefab provenance has an expansion alias and no direct projected owner document. Its original available-target observations remain retained. Each expanded document receives a complete effective-use scan, including nested instances, arrays, provenance fields and fields unused by native conversion. Each nonzero reference or reserved marker is checked against the destination owner's class/kind and field semantics. Physics targets require the exact class62/`PhysicsMaterial2D` document key. Rendering materials require the exact class21/`Material` document key in a material asset. Finding a different compatible document in the same file cannot satisfy either slot.

The additive `expanded_document` locator owner distinguishes effective uses from original serialized observations. It carries the template's asset GUID, original document index, class, fileID and kind, plus the actual graph `SourceRef.instance` chain. The source path/hash identify the template bytes. The receipt closure also binds every instance/override source, profile and sidecar. Effective `raw_reference` records the actual projected value. Its reference GUID/fileID come from that value; projected markers have no retained reference type, so their `reference_type` is null. Original source observations retain the authored reference type and override provenance separately. No expanded use borrows an original observation's resource decision or target availability.

An effective `$ref` must identify the same GUID/fileID or a real graph alias and pass destination resource validation. Null-to-reference overrides are permitted when the effective target validates. Original source scans still reject reserved marker syntax and inspect descendants below reference objects. Effective scans also inspect those descendants. A live override is never represented as a second original template owner. Removing an effective observation or changing its instance, field or target invalidates the proof seal and current-receipt qualification. Existing version1 source locators and policy bytes retain their meaning; policies that need an effective-use exception must bind its distinct locator explicitly.

The existing component omission policy remains separate. The successful proof records its effective component omissions. This increment conservatively blocks unresolved original references even in omitted runtime records. It supplies no general objectless-record or runtime-component exemption.

`DependencyPolicy::default()` selects strict original fidelity with no exceptions. Existing `import_project`, `materialization_plan` and `materialize` entry points retain their behavior. Callers opt into the new receipt boundary explicitly.

## Proof and source rechecks

`AdmissionProof` includes the source closure, importer identity, dependency interpreter version, policy hashes, actual provider/evidence hashes, all reference decisions, reviewed dispositions/reasons/evidence, component omissions, limitations and outcome. Deterministic typed JSON is hashed with `admission_sha256` set to the empty string. Source observations are sorted by locator; rule, capability and evidence arrays retain authored ordering; maps use deterministic key ordering. Array reordering can therefore require fresh qualification.

`AdmissionProof::from_json` rejects ambiguous JSON and checks its seal/version. A seal is an integrity binding, not an authorization signature. `ContentReceipt::verify_admission` recomputes admission using the actual receipt, policies and current trusted inventory, verifies the supplied seal and compares identities. Editing classifications while retaining an old ID rejects.

`AdmittedContent::verify_current(root)` rechecks sources and retained policy/provider snapshots. `verify_inputs(root, components, dependencies, current_providers)` additionally checks newly supplied current policy/provider bindings. Hosts must rebuild that inventory from current actual implementation/evidence bytes when external providers can change. `package_current` rechecks sources and retained admission before returning receipt-owned package inputs. A host that defers execution or adoption must call `verify_inputs` immediately under its source/provider leases. These optimistic checks are not filesystem transactions.

Fresh compatible gameplay sources and newly qualified providers remain usable through a new receipt/admission. An old proof cannot authorize changed inputs. Refresh owners must retain the admission ID with source closure, assembly hash, current source census and local revision proof, then recheck before lock replacement. No refresh/adoption or player wiring was changed here.

## Reviewed protected inputs and remaining blocks

`profiles/{snake,breakout,pong}-dependencies.json` encode the reviewed 17/18/15 metadata locations from the agreed review. They request translated-unlit/keyboard-subset fidelity. They contain no provider inventory and do not qualify a game. Breakout's script GUID `247994e1f5a72c2419c26a37e9334c01` remains explicitly blocked. Its expanded original URP pipeline, renderer and global-settings dependencies also remain in the ledger.

The `dependency_receipt` example reports admission with an empty provider inventory:

```sh
cargo run --offline --locked -p worldrunner-compat-content \
  --example dependency_receipt -- \
  ROOT SCENE PROFILE_JSON SIDECAR_JSON COMPONENTS_JSON DEPENDENCIES_JSON
```

A blocked proof is valid reporting output, so process success alone does not establish qualification. Consumers must inspect `outcome`. The existing `receipt` executable retains its strict command-line contract and rejection behavior.

Provider boundary doubles in the Rust admission tests establish matching and proof binding only. They establish no rendering, held/edge keyboard behavior, axes semantics or original URP fidelity. The implementation and observed protected blocks are recorded in [the recovery handoff](../../.session/resumed-content-policy.md); broader import behavior is described in [README.md](README.md).
