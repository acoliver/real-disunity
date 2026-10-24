# Exact translated execution admission

The additive translated boundary preserves the complete imported `ContentReceipt`, its source bytes and its original admission proof. Selection clones the execution graph. It never changes the original receipt, the dependency resolver or the protected per-game policies.

## Public contract

1. Import a `ContentReceipt` using the existing importer.
2. Construct a `SelectionPolicy` bound to that receipt's `original_closure_sha256` and a named translated profile. Each operation names an exact record, source SHA-256, old field value or record hash, reason, replacement behavior and explicit lost effects.
3. Call `receipt.select_execution(&component_policy, &dependency_policy, &selection_policy)`.
4. Register host-qualified providers in `TrustedProviderInventory`. Construct `GameSourceQualification::from_host(&dyn TrustedGameSourceProof)` from trusted host verification.
5. Call `translated.admit(root, &providers, &source_qualification)`. Only successful admission creates opaque `AdmittedTranslatedContent`.
6. Before adopting its `materialization_plan()`, call `admitted.verify_inputs(root, current_selection_policy, current_providers, current_source_qualification)`.

Admission reimports current content, checks the complete selected C# census, validates profile/content/typed API-use qualification, checks provider and exact resource-byte bindings, scans every retained execution record and reference, and builds a plan with the existing strict planner. Replacement does not silently regenerate other authored or native projections. Every remaining projection must pass validation.

`TranslatedAdmissionProof` retains the complete original receipt proof and original admission proof, the complete selection policy including losses, an original-observation selection ledger, and a separate effective-reference ledger. Effective observations identify document, component projection, settings or asset metadata and the complete key/index path. The proof binds execution graph, policy, providers and GameSource qualification hashes. Deserialization checks the serialized seal; `TranslatedContentReceipt::verify_admission` recomputes the actual current proof and compares its entire content. A correctly resealed forged ledger cannot authorize adoption.

## Supported selection scope

`CanonicalUnlitQuads` remains version 1 / `RenderSettings`. `CanonicalKeyboardSnapshot` remains version 1 / `Input`. The distinct version-1 IDs `CanonicalUnlitMaterial`, `CanonicalUnlitShader`, `AuthoredSprite`, `MappedFont` and `MappedTileType` are restricted to Material, Shader, Texture, Font and Script respectively. A capability cannot authorize arbitrary fields or whole-record deletion. `Unknown` cannot be a provider kind.

The complete QualitySettings record and all quality tiers remain retained. An exact tier's `customRenderPipeline` field can select a null canonical pipeline; all other fields and tiers remain present. Every retained tier must pass the execution scan.

One narrowly identified opaque record may be excluded from execution: `ProjectSettings/URPProjectSettings.asset`, with its exact source and projected record hashes, exactly one class-114/fileID-1 MonoBehaviour document, script fileID 11500000 and GUID `247994e1f5a72c2419c26a37e9334c01`. This requires the explicit `opaque-unconsumed` disposition and named lost effects. The entire original record remains in the receipt and original proof as unsupported. It receives no EditorTool classification. Incoming references to unavailable execution targets remain blockers.

Other unsupported records, unresolved retained observations, unlisted resources, wrong-kind resource uses, malformed or nested unknown markers, missing targets and inconsistent effective identities block admission. There are no class-wide or GUID-wide exemptions. Input importer record exclusion and automatic material/native/script reprojection require further concrete contracts and remain unavailable.

## Host trust and qualification

`TrustedProvider` is implemented by trusted composition after behavior qualification of an actual service. Registration binds implementation and qualification evidence bytes. Exact selected resources additionally bind provider ID, resource ID, kind, capability and SHA-256 of the actual retained resource bytes.

`TrustedGameSourceProof` is an attestation boundary. Its host implementation must verify the actual compiled GameSource PE, all selected source checksums and the complete transitive typed API-use closure, including helpers, generated wrappers and action consumers. Content checks the PE envelope, source census and typed selected contract. It does not implement a CLR metadata verifier. A fixture ready flag, source type list or self-authored JSON cannot construct a qualification without this trusted implementation. The host must refresh qualification when its actual PE, facade or verification evidence changes.

The keyboard contract uses the actual facade name `UnityEngine.InputSystem.KeyControl`, declared in `csharp/UnityCompat/Presentation.cs`. The absent `.Controls.KeyControl` namespace is rejected. Unknown and action/wrapper API uses block regardless of the policy's `keyboard_only` flag.

`tests/translated_execution.rs` uses explicitly synthetic provider and PE attestations to test this boundary. Those tests do not qualify a renderer, keyboard, PE inspector or real GameSource assembly. The positive microfixture proves that an original blocked shader-settings receipt can retain its entire original proof while admitting a separately selected graph under trusted boundary doubles.

## Game and adoption limits

This module does not wire player/session adoption. Q2 owns actual provider qualification; Q4 owns admission/session integration. Provider qualification for canonical unlit settings and keyboard input does not qualify material, shader, sprite, font or tile resources.

Snake, Breakout and Pong retain their original blocked proofs. The observation / blocked-observation / total-blocker counts remain 91 / 25 / 27, 413 / 204 / 206 and 483 / 38 / 40. Those original proofs retain no provider bindings. There is no original-fidelity, translated-game execution or gameplay acceptance claim. Selected URP behavior is an explicit profile loss; unsupported remaining URP and importer behavior remains blocked.
