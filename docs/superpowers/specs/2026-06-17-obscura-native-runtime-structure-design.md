# Obscura Native Runtime Structure Design

## Goal

Refactor `obscura-native-runtime` from a single large `lib.rs` into focused modules while preserving the working 17TRACK sample, the public runtime API, and the current trace behavior.

## Context

The native runtime now proves the iv8-like direction:

- It embeds bare V8 through the `v8` crate.
- It installs browser APIs through native `ObjectTemplate` and `FunctionTemplate` callbacks.
- It records WebAPI trace events through native accessor/function trampolines.
- It runs `samples/track17_open.py` parity in Rust through `obscura/crates/obscura-native-runtime/examples/track17_open.rs`.
- A real 17TRACK POST succeeds after matching the `navigator.plugins` and `navigator.mimeTypes` shape expected by the fingerprint module.

The current implementation is intentionally compact but has grown too large. `src/lib.rs` contains runtime ownership, V8 setup, state, trace types, binding helpers, BOM/DOM/WebGL/canvas implementations, timers, encoding, base64, crypto, and tests targets. This makes future WebAPI expansion hard to review and easy to break.

## Chosen Approach

Use domain modules plus shared binding and trace infrastructure.

Target shape:

```text
obscura/crates/obscura-native-runtime/src/
  lib.rs
  state.rs
  trace.rs
  bindings.rs
  values.rs
  webapi/
    mod.rs
    base64.rs
    canvas.rs
    crypto.rs
    document.rs
    encoding.rs
    location.rs
    navigator.rs
    screen.rs
    timers.rs
    webgl.rs
    window.rs
```

This is intentionally not a macro or WebIDL generator yet. The next step is to make the native-runtime shape maintainable, then add broader generated registries only after more targets make the needed API surface clearer.

## Module Responsibilities

### `lib.rs`

Owns only the public runtime facade:

- `RuntimeOptions`
- `RuntimeError`
- `NativeRuntime`
- V8 initialization
- `NativeRuntime::new`
- `load_html`
- `eval_json`
- `drain_event_loop`
- trace accessors: `trace`, `take_trace`, `clear_trace`, `trace_json`

It delegates browser object installation to `webapi::install_browser_objects`.

### `state.rs`

Owns runtime state shared by native callbacks:

- `NativeState`
- `TimerTask`
- DOM tree, document title, cookies, timers, trace buffer
- state helper functions that are not WebAPI-specific

`NativeState` remains crate-private. WebAPI modules mutate it only through crate-visible helpers or direct crate-visible fields when that keeps the vertical slice simpler.

### `trace.rs`

Owns trace data structures and generic recording helpers:

- `TraceEvent`
- `trace_json_from_events`
- `record_trace`
- `record_function_trace`
- argument/result formatting helpers

Trace events keep the current shape:

```rust
pub struct TraceEvent {
    pub target: String,
    pub name: String,
    pub kind: String,
    pub args: Vec<String>,
    pub result: Option<String>,
}
```

The initial refactor keeps trace always enabled. Filtering, ring buffers, and asynchronous flushing are out of scope for this refactor.

### `bindings.rs`

Owns native binding registration helpers:

- `AccessorSpec`
- `AccessorValue`
- `accessor`
- `define_template_accessors`
- `define_object_accessors`
- `set_template_function`
- `set_traced_template_function`
- `set_function_property`
- `set_traced_function_property`
- callback data encoding/decoding via `binding_data` and `binding_from_data`
- common `webapi_getter` and `webapi_setter`

Domain modules define their own static accessor specs and pass them into these helpers. The trampoline remains one place, matching the iv8-style native monitor idea.

### `values.rs`

Owns small V8 conversion utilities:

- `v8_str`
- `v8_value_to_json`
- `exception_string`
- helpers for setting object properties
- small constructors used by several WebAPI modules when they do not belong to one domain

This module should stay boring. It is not a WebAPI registry.

### `webapi/mod.rs`

Owns browser surface installation order:

- set global state internal field
- install constructors
- install `window`, `self`, window values
- install timers, encoding, base64, crypto
- create and install `navigator`, `screen`, `location`, `document`

The order stays behavior-compatible with the working runtime.

### Domain Modules

Each domain module owns the native implementation for one WebAPI area:

- `window.rs`: `Window`, `devicePixelRatio`, dimensions, constructor.
- `navigator.rs`: navigator accessors, plugin/mime type object shapes.
- `screen.rs`: screen accessors.
- `location.rs`: location accessors.
- `document.rs`: document object, DOM element wrappers, cookies, query/create APIs.
- `canvas.rs`: `HTMLCanvasElement`, 2D context, canvas method table, `toDataURL`.
- `webgl.rs`: `WebGLRenderingContext`, debug extension, parameters.
- `timers.rs`: `setTimeout`, `setInterval`, clear functions, timer allocation.
- `encoding.rs`: `TextEncoder`, `TextDecoder`.
- `base64.rs`: `atob`, `btoa`.
- `crypto.rs`: `crypto.getRandomValues`.

Domain modules may expose only `pub(crate)` installers and callbacks required by the binding helpers.

## Public API Compatibility

The following names and signatures must remain source-compatible:

- `RuntimeOptions`
- `RuntimeOptions::default`
- `RuntimeError`
- `TraceEvent`
- `NativeRuntime::new(options: RuntimeOptions) -> Self`
- `NativeRuntime::load_html(&mut self, html: &str) -> Result<(), RuntimeError>`
- `NativeRuntime::eval_json(&mut self, source: &str) -> Result<serde_json::Value, RuntimeError>`
- `NativeRuntime::drain_event_loop(&mut self) -> Result<(), RuntimeError>`
- `NativeRuntime::trace(&self) -> &[TraceEvent]`
- `NativeRuntime::take_trace(&mut self) -> Vec<TraceEvent>`
- `NativeRuntime::clear_trace(&mut self)`
- `NativeRuntime::trace_json(&self) -> serde_json::Value`

The `track17_open` example keeps its `--trace` flag and output behavior.

## Behavior Guardrails

The refactor must not change these observed behaviors:

- `navigator.plugins.length === 5`
- `navigator.plugins[0].name === "PDF Viewer"`
- `navigator.mimeTypes.length === 2`
- `navigator.mimeTypes[0].type === "application/pdf"`
- `Object.prototype.toString.call(document) === "[object Document]"`
- `canvas.getContext("2d") instanceof CanvasRenderingContext2D`
- `canvas.getContext("webgl") instanceof WebGLRenderingContext`
- `crypto.getRandomValues(new Uint8Array(4))` mutates and returns the same view
- trace records `navigator`, `screen`, `document`, `HTMLCanvasElement`, `CanvasRenderingContext2D`, `WebGLRenderingContext`, and `crypto` events
- `track17_open` produces a sign of length 1265 for the current 17TRACK chunk and returns real shipment data

## Testing Strategy

Use TDD for each refactor slice:

1. Add or strengthen a focused test before moving a behavior.
2. Run the focused test and confirm the existing code satisfies it.
3. Move code into the target module.
4. Run the focused test again.
5. Run the full native runtime smoke test.

Required test commands:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
cargo check -p obscura-native-runtime --example track17_open
```

Required live verification after the module split:

```bash
cargo run -p obscura-native-runtime --example track17_open
```

The live command requires network access and should return HTTP 200 with real `shipments`, not `meta.code=-13`.

## Non-Goals

- Do not change the existing `obscura-js` runtime path.
- Do not add Python bindings in this project.
- Do not introduce a WebIDL generator or macro-heavy registry in this refactor.
- Do not implement trace filtering, trace ring buffers, reflection interceptors, DevTools monitor, or async trace flushing yet.
- Do not change the 17TRACK request body format.

## Risks

- Rust module visibility can become noisy if state and helpers are over-split. Prefer `pub(crate)` over public exports.
- V8 handle lifetimes are sensitive. Keep callback implementations close to the domain that owns them and move helper code only when it is genuinely shared.
- Moving timer code can accidentally change event-loop behavior. Timer tests must run after each timer-related move.
- Moving plugin/mime type code can silently regress 17TRACK. The plugin shape tests are required before moving navigator code.
