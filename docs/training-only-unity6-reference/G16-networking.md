# G16 — Networking and UnityWebRequest

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Desired Unity 6.3 and networking-package versions are unverified. All code/signatures are illustrative authored subsets from memory, not full declarations or compile-tested examples. See U001–U003 and U049–U051.

**Boundary:** `UnityEngine.Networking.UnityWebRequest` and handler types are engine networking-module APIs. The namespace does not mean legacy UNet is the current multiplayer stack. Multiplayer, transport, relay, authentication and lobby services are separate packages/services with distinct contracts.

## G16.T01 — UnityWebRequest lifecycle

```csharp
// Selected constructor/factory/instance shapes.
UnityWebRequest(string url, string method);
UnityWebRequest(string url, string method, DownloadHandler downloadHandler,
                UploadHandler uploadHandler);
static UnityWebRequest Get(string uri);
static UnityWebRequest Head(string uri);
static UnityWebRequest Delete(string uri);
static UnityWebRequest Put(string uri, byte[] bodyData);
UnityWebRequestAsyncOperation SendWebRequest();
void Abort();
void Dispose();
```

String/Uri variants and more factories exist; exact overloads are U049. Construct/configure, send once, wait for completion, inspect result/body/headers, then dispose is the ordinary flow. Setting URL/method/headers/handlers after sending is restricted. SendWebRequest returns an AsyncOperation-derived object, not the response body. It can be yielded in a coroutine; engine Awaitable/Task integration is not assumed.

Useful properties: url, method, timeout, redirectLimit, useHttpContinue, uploadHandler, downloadHandler, certificateHandler, isDone, responseCode, error, result, downloadedBytes, uploadedBytes, downloadProgress and uploadProgress. Exact integer widths and availability are not exhaustively declared. Progress can be unknown or weighted by engine bookkeeping and should not be equated with a trustworthy total byte length.

Selected authored flow (illustrative only):

```csharp
System.Collections.IEnumerator FetchText(string url)
{
    using (var request = UnityWebRequest.Get(url))
    {
        yield return request.SendWebRequest();
        if (request.result == UnityWebRequest.Result.Success)
            UnityEngine.Debug.Log(request.downloadHandler.text);
        else
            UnityEngine.Debug.LogError(request.error);
    }
}
```

This is not production retry/security policy; logging response bodies can leak data. Coroutine cancellation alone is not a portable guarantee that an in-flight request is aborted and disposed in every engine path.

## G16.T02 — Download handlers

`DownloadHandlerBuffer` accumulates data and exposes byte[]/text views. `DownloadHandlerFile` streams to disk, avoiding a full in-memory payload but creating filesystem/partial-file ownership. `DownloadHandlerTexture` and `UnityWebRequestTexture.GetTexture(...)` decode textures; `DownloadHandlerTexture.GetContent(request)` retrieves the result. `DownloadHandlerAudioClip`, `UnityWebRequestMultimedia.GetAudioClip(uri, AudioType)` and corresponding GetContent support audio formats subject to platform codecs.

`DownloadHandlerAssetBundle` and `UnityWebRequestAssetBundle` handle bundles/cache integration (G14). Data-processing failure after HTTP success remains a request failure category. Download handlers can hold native buffers/resources; disposing the request commonly disposes attached handlers according to dispose flags. An extracted Texture/AudioClip/AssetBundle may have a separate engine-object lifetime; verify ownership rather than either leaking it or destroying it prematurely.

`DownloadHandlerScript` provides custom receive callbacks with constructor-buffer variants. Remembered hooks include ReceiveData, ReceiveContentLengthHeader, CompleteContent and GetData; exact protected signatures and worker/main-thread invocation are U050. Bound total bytes, reject oversized content, and do not assume every download advertises Content-Length. For streaming consumers, chunk boundaries are transport artifacts, not message boundaries.

A handler's `.text` decoding uses response/encoding conventions; it is not guaranteed correct for arbitrary binary data. Store binary as bytes, and explicitly validate content type/schema where the application requires it.

## G16.T03 — Upload, forms, request bodies

`UploadHandlerRaw(byte[])` sends an already encoded body. `UploadHandlerFile` supports file uploads in remembered APIs; exact platform availability and lifetime are U050. Content type belongs to HTTP metadata; creating UTF-8 JSON bytes without setting a suitable content type can be interpreted differently by servers.

`UnityWebRequest.Post` has multiple historically changing overload families: WWWForm, lists of `IMultipartFormSection`, field dictionaries and newer raw post-data/content-type forms. Do **not** assume a two-string Post overload sends JSON verbatim; historical forms can encode it as form data, and overload obsolescence is U049. An explicit method plus UploadHandlerRaw is clearer when exact bytes are the contract.

`WWWForm` is a form-building helper, not an HTTP request itself. `AddField` and `AddBinaryData` provide common field/file inputs. `MultipartFormDataSection` and `MultipartFormFileSection` represent multipart fields/files through `IMultipartFormSection`. Filenames, encodings, generated boundaries and request headers must match the chosen form API rather than being mixed from incompatible builders.

`SetRequestHeader(string name, string value)`, `GetRequestHeader`, `GetResponseHeader`, and `GetResponseHeaders` are remembered operations. Some headers are restricted or platform-controlled. Browser networking can reject changes to forbidden headers and enforce credentials/CORS policies independently of C# code. Response header names are case-insensitive in HTTP, even if a particular returned collection needs care.

## G16.T04 — Errors, headers, security, cancellation

`UnityWebRequest.Result` remembered values: `InProgress`, `Success`, `ConnectionError`, `ProtocolError`, `DataProcessingError`. Older `isNetworkError`/`isHttpError` patterns are historical/obsolete rather than recommended target signatures. HTTP 4xx/5xx usually map to protocol failure while still providing a useful response body; DNS, TLS and transport failures differ. `.error` is diagnostic text, not a stable machine-readable application error code.

`responseCode` is an HTTP status observation, not proof a body matches the expected schema or that a command succeeded at business level. A disconnected request might have reached the server before its response was lost. Retry safe/idempotent operations with bounded backoff; retrying a charge/create POST requires application idempotency, not just the same URL. Timeout/redirect semantics differ by platform and are U051.

`CertificateHandler.ValidateCertificate(byte[])` is a custom certificate-validation hook in supported backends. Returning true unconditionally defeats authentication and is not a production workaround. This reference does not define certificate-chain validation or pinning. Prefer the platform's verified TLS behavior and explicitly test any customization. HTTPS is not authorization: protect tokens and apply endpoint-level access policy.

Abort requests when the owning workflow is canceled where appropriate, and still dispose their resources. Disposal timing, handler dispose flags (`disposeDownloadHandlerOnDispose`, `disposeUploadHandlerOnDispose`, `disposeCertificateHandlerOnDispose`) and completion callback races must be chosen/verified. Do not touch a request after disposal merely because a managed reference remains non-null.

## G16.T05 — Transport and multiplayer package boundaries

Unity Transport is a separately versioned package (`com.unity.transport`, commonly `Unity.Networking.Transport` namespace) providing low-level connection/event/driver workflows. It is not UnityWebRequest and does not make HTTP calls just because both are networking. Driver scheduling, native containers, reliability pipelines, sequencing and connection timeouts have their own contracts.

Netcode for GameObjects (`com.unity.netcode.gameobjects`, `Unity.Netcode`) supplies NetworkManager, NetworkObject, NetworkBehaviour, synchronized state and RPC concepts. Exact RPC attributes have evolved; older ServerRpc/ClientRpc shapes must not be declared the only Unity 6-compatible API. Netcode for Entities is another package coupled to Entities, with ghost/snapshot workflows. Neither is equivalent to the deprecated legacy UNet NetworkIdentity/NetworkBehaviour model.

Relay, lobby, authentication, matchmaking and online services have package/backend/version/credential dependencies and operational terms not described here. A local multiplayer simulation does not establish interoperability with those services. Third-party networking products are out of scope. Platform sockets/.NET HttpClient capabilities vary, especially in browsers; they are not direct replacements for every UnityWebRequest backend behavior.

## G16.T06 — Compatibility and protocol test contracts

For a subset runtime, specify supported URI schemes, methods, redirect policy, timeouts, proxy/TLS behavior, encodings, cookie handling, header restrictions, response-size limits, cancellation and handler ownership. Make one layer own retry policy. Separate transport errors, HTTP errors, decoding errors and application errors.

Proposed tests (not executed): local test-server GET/POST exact body bytes; binary download integrity; a 404 body remains inspectable; malformed image produces processing failure; cancellation releases partial file/buffers; redirects follow the declared limit; timeouts are bounded; chunked response without content length stays memory-bounded; and credentials are not leaked across an untrusted redirect. Browser CORS, native certificate handling, every multipart overload and multiplayer protocol compatibility remain unknown.
