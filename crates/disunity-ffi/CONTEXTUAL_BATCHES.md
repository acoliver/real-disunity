# Native Cleanup lifecycle batches

`session.open` can select `native_contextual_lifecycle_batch_v1` on a fresh world. This is a native-only profile. Its descriptor reports `managed_integration:false` and `callback_batch_attestation:false`. Existing managed consumers do not select it.

The profile supports serial Initialize readiness and lifecycle-only Cleanup batches. It rejects frame advancement, routine operations, and registration of Start, Update, FixedUpdate or LateUpdate metadata. Contacts and nested execution are unsupported. Initialization uses the existing private readiness protocol, including its 4096 root-claim budget. Registration during Initialize returns `{binding}`; the binding is the native generation-checked component handle scoped by the exact current session. Old profiles still return `{}`.

## Authority and wire format

Every callbacks operation requires the exact claimed private native Context. Host phase controls continue to require the separate host credential. Keep both credentials and all bindings private to the host or future managed adapter.

The native envelope is exactly:

```text
{version:1, context:{lease,root_phase,phase,flags,time,
 dynamic_delta,fixed_delta,unscaled_delta,callback_batch}, callbacks:[Record...]}
Record = {binding,component,kind,payload:{}}
```

`lease` projects the existing seven-field Context. In this envelope, lease integers, binding, component, callback_batch, and rational components use integer JSON up to 9007199254740991 or canonical unsigned decimal strings up to u64::MAX. Unsafe numeric integer tokens, signs, whitespace, exponents and leading-zero strings reject. The outer operation Context and existing readiness Decision/completion subprotocol retain their existing exact native-u64 JSON codec. Consumers of those private inherited fields must preserve integers without double conversion.

For this slice, `root_phase` is `Cleanup`, `phase` is `destroy`, and `flags` is 1, meaning partial Cleanup lifecycle scope. `kind` is OnDisable or OnDestroy. `time` is `{numerator,denominator}` from the native rational timeline. Delta projections come from the current native clock without advancement and validate by parsed f32 bit equality. No host-generated time is accepted.

## Linear operations

| Request | Result and transition |
| --- | --- |
| `callbacks.next {context}` | Envelope or null. Reserves current native candidates in lane and handle order. No execution tickets or lifecycle flags are consumed by reservation. |
| `callbacks.inspect {context,batch}` | `{envelope,cursor,accepted,cut}`. Does not advance content revision or authorize entry. |
| `callbacks.accept {context,envelope}` | `{accepted:true}` after full typed equality with the native reservation. One acceptance only. |
| `callbacks.enter {context,batch,index}` | `{execute,replan,decision?}`. Reselects the highest native candidate and enters one ticket only when it matches the next tuple. |
| `readiness.complete {context,ticket,outcome}` | Completes the entered ticket and advances one batch cursor. `ok` and `failed` both count as attempts. The existing last-identical-completion acknowledgment remains idempotent. |
| `callbacks.finish {context,batch}` | Closes a completely drained or natively cut batch. |

On an ordering mismatch, native cuts the unexecuted suffix, retains at most one actual unentered selection, and returns `replan:true`. Finish the cut batch before asking for another envelope. The next envelope contains the retained decision alone. Entry revalidates priority through the same selector, retains that ticket identity if unchanged, and replaces a superseded selection without entering it. This path is defensive in the Cleanup-only scope: disabling or removing an already awakened component preserves its retained native Cleanup obligations and ordering. No dynamic-update priority qualification is claimed.

There is at most one pending envelope, at most 4096 records, at most 1 MiB of encoded envelope bytes, and at most 4096 issuances per root. Existing per-root execution claims remain separate. Direct Cleanup readiness.next/enter is forbidden in this profile. Pending batches block phase.finish and snapshot drain. Native Cleanup obligations must also drain before phase.finish. Terminal takeover revokes a pending batch and abandons uncertain entered tickets through the existing once-only recovery mechanism. The host must unwind external callback execution before takeover.

## Validation and response admission

The request-size check precedes parsing. A streaming raw scan sees every operation spelling before Value conversion, including an earlier callbacks operation overwritten by a duplicate op. Contextual requests reject decoded duplicate schema keys, including escaped equivalents. A bounded token pass limits depth to 64, nodes to 100000 and callback arrays to 4096 before Value or typed record-vector allocation. Typed DTOs independently reject unknown fields and oversized record vectors. Legacy JSON remains on its existing Value and float parsing path after the read-only raw scan.

Reservation, acceptance, entry, completion and finish are staged with the canonical world. Response admission failure commits no batch, ticket, flag or cursor transition. Process-wide checked identity allocation can leave unused generation gaps after rollback. Identities are never reused.

A native-ticket entry is separate from invocation of a managed method. Native tests and C ABI probes establish native selection and linear authority only. CLR execution, managed failure aggregation, contact recipient batches, coroutine batches and full SPEC157/161 remain pending.
