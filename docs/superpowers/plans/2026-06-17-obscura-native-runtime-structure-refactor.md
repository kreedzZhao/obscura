# Obscura Native Runtime Structure Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `obscura-native-runtime` into focused state, trace, binding, value, and WebAPI domain modules without changing the working 17TRACK behavior.

**Architecture:** Keep `NativeRuntime` as the public facade in `src/lib.rs`. Move shared runtime state to `state.rs`, trace recording to `trace.rs`, V8 registration trampolines to `bindings.rs`, conversion helpers to `values.rs`, and browser APIs into `webapi/*` modules. Preserve the existing trace event shape and the iv8-like native accessor/function trampoline model.

**Tech Stack:** Rust 2021, `v8 = 137.3.0`, `obscura-dom`, `serde_json`, `base64`, `getrandom`, `reqwest` for the example.

## Global Constraints

- Work only inside `/Users/kreedz/Develops/browser/v8-mock-env/obscura`.
- Do not modify the existing `obscura-js` runtime path.
- Preserve all public `obscura_native_runtime` APIs listed in `docs/superpowers/specs/2026-06-17-obscura-native-runtime-structure-design.md`.
- Keep trace always enabled in this refactor.
- Do not introduce macros, WebIDL generation, reflection interceptors, trace filtering, ring buffers, or Python bindings.
- Every behavior move must have test coverage before moving code.
- Run `cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1` after each task.

---

### Task 1: Strengthen Guardrail Tests

**Files:**
- Modify: `crates/obscura-native-runtime/tests/runtime_smoke.rs`

**Interfaces:**
- Consumes: existing `NativeRuntime`, `RuntimeOptions`, and trace APIs.
- Produces: tests that lock down plugin/mime type shape and trace JSON shape before code moves.

- [ ] **Step 1: Add plugin and mime type assertions**

Add this test near `exposes_track17_fingerprint_environment`:

```rust
#[test]
fn exposes_pdf_plugin_and_mime_type_shape_for_track17() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(
        runtime
            .eval_json(
                r#"
                [
                    navigator.plugins.length,
                    navigator.plugins[0].name,
                    navigator.plugins[0].filename,
                    navigator.plugins[0].description,
                    navigator.plugins[0].length,
                    Object.prototype.toString.call(navigator.plugins[0]),
                    navigator.mimeTypes.length,
                    navigator.mimeTypes[0].type,
                    navigator.mimeTypes[0].description,
                    navigator.mimeTypes[0].suffixes,
                    Object.prototype.toString.call(navigator.mimeTypes[0])
                ]
                "#,
            )
            .unwrap(),
        json!([
            5,
            "PDF Viewer",
            "internal-pdf-viewer",
            "Portable Document Format",
            1,
            "[object Plugin]",
            2,
            "application/pdf",
            "Portable Document Format",
            "pdf",
            "[object MimeType]"
        ])
    );
}
```

- [ ] **Step 2: Add trace JSON shape assertions**

Add this test after `records_web_api_trace_events`:

```rust
#[test]
fn trace_json_keeps_stable_event_shape() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    runtime.eval_json("navigator.userAgent; null").unwrap();

    assert_eq!(
        runtime.trace_json(),
        json!([
            {
                "target": "navigator",
                "name": "userAgent",
                "kind": "get",
                "args": [],
                "result": RuntimeOptions::default().user_agent
            }
        ])
    );
}
```

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo test -p obscura-native-runtime exposes_pdf_plugin_and_mime_type_shape_for_track17 trace_json_keeps_stable_event_shape -- --test-threads=1
```

Expected: both tests pass against the current implementation.

- [ ] **Step 4: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all runtime smoke tests pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add crates/obscura-native-runtime/tests/runtime_smoke.rs
git commit -m "test: lock native runtime browser surface"
```

### Task 2: Extract State, Trace, and Value Helpers

**Files:**
- Create: `crates/obscura-native-runtime/src/state.rs`
- Create: `crates/obscura-native-runtime/src/trace.rs`
- Create: `crates/obscura-native-runtime/src/values.rs`
- Modify: `crates/obscura-native-runtime/src/lib.rs`

**Interfaces:**
- Produces: `state::NativeState`, `state::TimerTask`
- Produces: `trace::TraceEvent`, `trace::record_trace`, `trace::record_function_trace`, `trace::trace_json_from_events`
- Produces: `values::v8_str`, `values::v8_value_to_json`, `values::exception_string`, `values::set_property`
- Preserves: `pub use trace::TraceEvent`

- [ ] **Step 1: Move state structs**

Create `src/state.rs`:

```rust
use obscura_dom::DomTree;

use crate::{RuntimeOptions, TraceEvent};

pub(crate) struct NativeState {
    pub(crate) options: RuntimeOptions,
    pub(crate) dom: DomTree,
    pub(crate) title: String,
    pub(crate) cookies: Vec<(String, String)>,
    pub(crate) next_timer_id: u32,
    pub(crate) timers: Vec<TimerTask>,
    pub(crate) trace: Vec<TraceEvent>,
}

pub(crate) struct TimerTask {
    pub(crate) id: u32,
    pub(crate) callback: v8::Global<v8::Function>,
    pub(crate) repeat: bool,
}
```

Remove `NativeState` and `TimerTask` definitions from `lib.rs`, add:

```rust
mod state;

use state::NativeState;
```

- [ ] **Step 2: Move trace event and recording helpers**

Create `src/trace.rs` with:

```rust
use crate::state::NativeState;
use crate::values::v8_str;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub target: String,
    pub name: String,
    pub kind: String,
    pub args: Vec<String>,
    pub result: Option<String>,
}

pub(crate) fn trace_json_from_events(events: &[TraceEvent]) -> serde_json::Value {
    serde_json::Value::Array(
        events
            .iter()
            .map(|event| {
                serde_json::json!({
                    "target": event.target,
                    "name": event.name,
                    "kind": event.kind,
                    "args": event.args,
                    "result": event.result,
                })
            })
            .collect(),
    )
}
```

Move the existing `record_trace`, `record_function_trace`, `trace_args`, and `trace_value` helpers into this file. Keep their signatures crate-visible:

```rust
pub(crate) fn record_trace(...)
pub(crate) fn record_function_trace(...)
pub(crate) fn trace_args(...)
pub(crate) fn trace_value(...)
```

Add to `lib.rs`:

```rust
mod trace;

pub use trace::TraceEvent;
```

Update `NativeRuntime::trace_json` to call:

```rust
trace::trace_json_from_events(&self.state.trace)
```

- [ ] **Step 3: Move V8 value helpers**

Create `src/values.rs` and move:

```rust
pub(crate) fn set_property(...)
pub(crate) fn v8_str(...)
pub(crate) fn v8_value_to_json(...)
pub(crate) fn exception_string(...)
```

Update imports in `lib.rs`.

- [ ] **Step 4: Run formatter**

Run:

```bash
cargo fmt -p obscura-native-runtime
```

Expected: no output.

- [ ] **Step 5: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/obscura-native-runtime/src/lib.rs crates/obscura-native-runtime/src/state.rs crates/obscura-native-runtime/src/trace.rs crates/obscura-native-runtime/src/values.rs
git commit -m "refactor: extract native runtime state and trace"
```

### Task 3: Extract Binding Registry Helpers

**Files:**
- Create: `crates/obscura-native-runtime/src/bindings.rs`
- Modify: `crates/obscura-native-runtime/src/lib.rs`

**Interfaces:**
- Produces: `bindings::AccessorSpec`
- Produces: `bindings::AccessorValue`
- Produces: `bindings::accessor`
- Produces: `bindings::define_template_accessors`
- Produces: `bindings::define_object_accessors`
- Produces: `bindings::set_template_function`
- Produces: `bindings::set_traced_template_function`
- Produces: `bindings::set_function_property`
- Produces: `bindings::set_traced_function_property`
- Produces: `bindings::state_ptr`
- Produces: `bindings::binding_from_data`

- [ ] **Step 1: Create bindings module**

Move these items from `lib.rs` into `src/bindings.rs`:

```rust
AccessorSpec
AccessorValue
accessor
instantiate_with_state
set_template_accessor
define_template_accessors
define_object_accessors
set_template_function
set_traced_template_function
set_function_property
set_traced_function_property
binding_data
binding_from_data
find_accessor_spec
webapi_getter
webapi_setter
accessor_value_to_v8
state_ptr
```

Make the items used by WebAPI modules `pub(crate)`.

- [ ] **Step 2: Keep accessor spec arrays temporarily in lib.rs**

Leave `WINDOW_ACCESSORS`, `NAVIGATOR_ACCESSORS`, `SCREEN_ACCESSORS`, `LOCATION_ACCESSORS`, `DOCUMENT_ACCESSORS`, and `CANVAS_2D_NOOP_METHODS` in `lib.rs` for this task. Update them to use `bindings::AccessorSpec`, `bindings::AccessorValue`, and `bindings::accessor`.

- [ ] **Step 3: Update imports**

Add to `lib.rs`:

```rust
mod bindings;

use bindings::{
    accessor, define_object_accessors, define_template_accessors, instantiate_with_state,
    set_function_property, set_template_function, set_traced_function_property,
    set_traced_template_function, state_ptr, AccessorSpec, AccessorValue,
};
```

Adjust the list to exactly what the compiler reports as needed.

- [ ] **Step 4: Run focused trace tests**

Run:

```bash
cargo test -p obscura-native-runtime records_web_api_trace_events trace_json_keeps_stable_event_shape -- --test-threads=1
```

Expected: both trace tests pass.

- [ ] **Step 5: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/obscura-native-runtime/src/lib.rs crates/obscura-native-runtime/src/bindings.rs
git commit -m "refactor: extract native binding helpers"
```

### Task 4: Extract Core Browser API Installation

**Files:**
- Create: `crates/obscura-native-runtime/src/webapi/mod.rs`
- Create: `crates/obscura-native-runtime/src/webapi/window.rs`
- Create: `crates/obscura-native-runtime/src/webapi/navigator.rs`
- Create: `crates/obscura-native-runtime/src/webapi/screen.rs`
- Create: `crates/obscura-native-runtime/src/webapi/location.rs`
- Modify: `crates/obscura-native-runtime/src/lib.rs`
- Modify: `crates/obscura-native-runtime/src/bindings.rs`

**Interfaces:**
- Produces: `webapi::install_browser_objects(scope, state_ptr)`
- Produces: `window::install_window_constructor`, `window::install_window_values`
- Produces: `navigator::create_navigator`
- Produces: `screen::create_screen`
- Produces: `location::create_location`

- [ ] **Step 1: Move browser installer**

Create `src/webapi/mod.rs`:

```rust
pub(crate) mod location;
pub(crate) mod navigator;
pub(crate) mod screen;
pub(crate) mod window;

use std::ffi::c_void;

use crate::state::NativeState;
use crate::values::set_property;

pub(crate) fn install_browser_objects(
    scope: &mut v8::HandleScope,
    state_ptr: *mut NativeState,
) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let external = v8::External::new(scope, state_ptr.cast::<c_void>());
    global.set_internal_field(0, external.into());

    window::install_window_constructor(scope, global);
    set_property(scope, global, "window", global.into());
    set_property(scope, global, "self", global.into());
    window::install_window_values(scope, global);

    let navigator = navigator::create_navigator(scope, state_ptr);
    set_property(scope, global, "navigator", navigator.into());

    let screen = screen::create_screen(scope, state_ptr);
    set_property(scope, global, "screen", screen.into());

    let location = location::create_location(scope, state_ptr);
    set_property(scope, global, "location", location.into());
}
```

This starts with a partial installer. Later tasks add document, timers, encoding, crypto, canvas, and WebGL back into it.

- [ ] **Step 2: Move window code**

Move these from `lib.rs` to `webapi/window.rs`:

```rust
WINDOW_ACCESSORS
install_window_constructor
install_constructor
window_constructor
install_window_values
```

Keep constructor helpers `pub(crate)` when other modules still need them.

- [ ] **Step 3: Move navigator code**

Move these from `lib.rs` to `webapi/navigator.rs`:

```rust
NAVIGATOR_ACCESSORS
create_navigator
create_plugin_object
create_mime_type_object
```

- [ ] **Step 4: Move screen and location code**

Move these from `lib.rs`:

```rust
SCREEN_ACCESSORS -> webapi/screen.rs
create_screen -> webapi/screen.rs
LOCATION_ACCESSORS -> webapi/location.rs
create_location -> webapi/location.rs
```

- [ ] **Step 5: Update lib.rs installer call**

Add:

```rust
mod webapi;
```

Replace the old local `install_browser_objects` call with:

```rust
webapi::install_browser_objects(scope, state_ptr);
```

- [ ] **Step 6: Restore missing installation calls**

If compilation shows missing document/timer/crypto/encoding/base64 installation because `install_browser_objects` was moved partially, keep those calls temporarily in `webapi::install_browser_objects` by importing their still-in-`lib.rs` functions with `pub(crate)` visibility. Do not change behavior.

- [ ] **Step 7: Run browser global tests**

Run:

```bash
cargo test -p obscura-native-runtime evaluates_javascript_and_browser_globals exposes_pdf_plugin_and_mime_type_shape_for_track17 exposes_track17_fingerprint_environment -- --test-threads=1
```

Expected: all listed tests pass.

- [ ] **Step 8: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 9: Commit**

Run:

```bash
git add crates/obscura-native-runtime/src/lib.rs crates/obscura-native-runtime/src/webapi/mod.rs crates/obscura-native-runtime/src/webapi/window.rs crates/obscura-native-runtime/src/webapi/navigator.rs crates/obscura-native-runtime/src/webapi/screen.rs crates/obscura-native-runtime/src/webapi/location.rs crates/obscura-native-runtime/src/bindings.rs
git commit -m "refactor: split core browser api modules"
```

### Task 5: Extract Document and DOM Element APIs

**Files:**
- Create: `crates/obscura-native-runtime/src/webapi/document.rs`
- Modify: `crates/obscura-native-runtime/src/webapi/mod.rs`
- Modify: `crates/obscura-native-runtime/src/lib.rs`

**Interfaces:**
- Produces: `document::create_document`
- Produces: document cookie helpers
- Produces: DOM element wrapper helpers

- [ ] **Step 1: Move document code**

Move these from `lib.rs` into `webapi/document.rs`:

```rust
DOCUMENT_ACCESSORS
create_document
create_element
element_getter
element_get_attribute
element_node_id
element_tag_name
element_attribute
document_cookie
set_document_cookie
document_query_selector
document_create_element
```

Make only `create_document`, `document_cookie`, and `set_document_cookie` `pub(crate)` if other modules need them.

- [ ] **Step 2: Update bindings setter dependency**

If `bindings::webapi_setter` needs `set_document_cookie`, call it through:

```rust
crate::webapi::document::set_document_cookie(state, &value);
```

- [ ] **Step 3: Install document from webapi/mod.rs**

Add to `webapi/mod.rs`:

```rust
pub(crate) mod document;
```

Install it in `install_browser_objects`:

```rust
let document = document::create_document(scope, state_ptr);
set_property(scope, global, "document", document.into());
```

- [ ] **Step 4: Run DOM tests**

Run:

```bash
cargo test -p obscura-native-runtime loads_html_into_native_document_state query_selector_returns_native_element_wrappers exposes_track17_fingerprint_environment -- --test-threads=1
```

Expected: all listed tests pass.

- [ ] **Step 5: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/obscura-native-runtime/src/lib.rs crates/obscura-native-runtime/src/bindings.rs crates/obscura-native-runtime/src/webapi/mod.rs crates/obscura-native-runtime/src/webapi/document.rs
git commit -m "refactor: split native document api"
```

### Task 6: Extract Timers, Encoding, Base64, and Crypto

**Files:**
- Create: `crates/obscura-native-runtime/src/webapi/timers.rs`
- Create: `crates/obscura-native-runtime/src/webapi/encoding.rs`
- Create: `crates/obscura-native-runtime/src/webapi/base64.rs`
- Create: `crates/obscura-native-runtime/src/webapi/crypto.rs`
- Modify: `crates/obscura-native-runtime/src/webapi/mod.rs`
- Modify: `crates/obscura-native-runtime/src/lib.rs`

**Interfaces:**
- Produces: `timers::install_timer_functions`
- Produces: `encoding::install_encoding`
- Produces: `base64::install_base64`
- Produces: `crypto::install_crypto`

- [ ] **Step 1: Move timer code**

Move these from `lib.rs` to `webapi/timers.rs`:

```rust
install_timer_functions
set_timeout
set_interval
allocate_timer
clear_timer
```

- [ ] **Step 2: Move encoding code**

Move these from `lib.rs` to `webapi/encoding.rs`:

```rust
install_encoding
install_encoding_constructor
text_encoder_constructor
text_decoder_constructor
text_encoder_encode
text_decoder_decode
```

- [ ] **Step 3: Move base64 code**

Move these from `lib.rs` to `webapi/base64.rs`:

```rust
install_base64
atob
btoa
```

- [ ] **Step 4: Move crypto code**

Move these from `lib.rs` to `webapi/crypto.rs`:

```rust
install_crypto
crypto_get_random_values
```

- [ ] **Step 5: Install modules from webapi/mod.rs**

Add module declarations and calls:

```rust
pub(crate) mod base64;
pub(crate) mod crypto;
pub(crate) mod encoding;
pub(crate) mod timers;

timers::install_timer_functions(scope, global);
encoding::install_encoding(scope, global);
base64::install_base64(scope, global);
crypto::install_crypto(scope, global);
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test -p obscura-native-runtime exposes_timer_functions_for_browser_bundles set_timeout_advances_async_browser_code drains_pending_timer_callbacks text_encoder_encodes_utf8_bytes text_decoder_decodes_utf8_bytes exposes_base64_helpers exposes_crypto_get_random_values -- --test-threads=1
```

Expected: all listed tests pass.

- [ ] **Step 7: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

Run:

```bash
git add crates/obscura-native-runtime/src/lib.rs crates/obscura-native-runtime/src/webapi/mod.rs crates/obscura-native-runtime/src/webapi/timers.rs crates/obscura-native-runtime/src/webapi/encoding.rs crates/obscura-native-runtime/src/webapi/base64.rs crates/obscura-native-runtime/src/webapi/crypto.rs
git commit -m "refactor: split utility browser APIs"
```

### Task 7: Extract Canvas and WebGL APIs

**Files:**
- Create: `crates/obscura-native-runtime/src/webapi/canvas.rs`
- Create: `crates/obscura-native-runtime/src/webapi/webgl.rs`
- Modify: `crates/obscura-native-runtime/src/webapi/mod.rs`
- Modify: `crates/obscura-native-runtime/src/webapi/document.rs`
- Modify: `crates/obscura-native-runtime/src/lib.rs`

**Interfaces:**
- Produces: `canvas::install_canvas_constructor`
- Produces: `canvas::create_canvas_element`
- Produces: `canvas::install_canvas_members`
- Produces: `webgl::install_webgl_constructor`
- Produces: `webgl::create_webgl_context`

- [ ] **Step 1: Move canvas code**

Move these from `lib.rs` to `webapi/canvas.rs`:

```rust
CANVAS_2D_NOOP_METHODS
html_canvas_element_constructor
canvas_rendering_context_2d_constructor
create_canvas_element
install_canvas_members
canvas_get_context
create_canvas_context
traced_noop_function
canvas_to_data_url
```

- [ ] **Step 2: Move WebGL code**

Move these from `lib.rs` to `webapi/webgl.rs`:

```rust
webgl_rendering_context_constructor
create_webgl_context
webgl_get_extension
webgl_get_supported_extensions
webgl_get_parameter
```

- [ ] **Step 3: Install constructors from webapi/mod.rs**

Add:

```rust
pub(crate) mod canvas;
pub(crate) mod webgl;
```

Expose constructor installer helpers from `canvas.rs` and `webgl.rs`, then call them from `install_browser_objects` before document creation:

```rust
canvas::install_canvas_constructor(scope, global);
webgl::install_webgl_constructor(scope, global);
```

- [ ] **Step 4: Update document createElement dependency**

In `webapi/document.rs`, call:

```rust
crate::webapi::canvas::install_canvas_members(scope, element);
```

when creating a `CANVAS` element.

- [ ] **Step 5: Run canvas/WebGL tests**

Run:

```bash
cargo test -p obscura-native-runtime document_create_element_returns_canvas_shim records_web_api_trace_events -- --test-threads=1
```

Expected: both tests pass.

- [ ] **Step 6: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

Run:

```bash
git add crates/obscura-native-runtime/src/lib.rs crates/obscura-native-runtime/src/webapi/mod.rs crates/obscura-native-runtime/src/webapi/document.rs crates/obscura-native-runtime/src/webapi/canvas.rs crates/obscura-native-runtime/src/webapi/webgl.rs
git commit -m "refactor: split canvas and webgl APIs"
```

### Task 8: Final Validation and Cleanup

**Files:**
- Modify: `crates/obscura-native-runtime/src/lib.rs`
- Modify: any `crates/obscura-native-runtime/src/*.rs` module with unused imports or visibility cleanup
- Modify: `docs/superpowers/plans/2026-06-17-obscura-native-runtime-structure-refactor.md`

**Interfaces:**
- Produces: clean module tree with no dead helper copies in `lib.rs`.
- Preserves: working `track17_open` real POST path.

- [ ] **Step 1: Search for leftover responsibilities in lib.rs**

Run:

```bash
rg -n "AccessorSpec|AccessorValue|record_trace|set_traced|create_canvas|webgl_|document_cookie|set_document_cookie|text_encoder|crypto_get_random_values|atob|btoa" crates/obscura-native-runtime/src/lib.rs
```

Expected: no matches except module imports or public facade references.

- [ ] **Step 2: Run formatter**

Run:

```bash
cargo fmt -p obscura-native-runtime
```

Expected: no output.

- [ ] **Step 3: Run full smoke test**

Run:

```bash
cargo test -p obscura-native-runtime --test runtime_smoke -- --test-threads=1
```

Expected: all tests pass.

- [ ] **Step 4: Run example check**

Run:

```bash
cargo check -p obscura-native-runtime --example track17_open
```

Expected: check passes.

- [ ] **Step 5: Run live 17TRACK verification**

Run:

```bash
cargo run -p obscura-native-runtime --example track17_open
```

Expected:

- sign length is 1265 for the current chunk
- HTTP status is 200
- response contains `"shipments":[{"code":200`
- response does not contain `"code":-13`

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/obscura-native-runtime/src docs/superpowers/plans/2026-06-17-obscura-native-runtime-structure-refactor.md
git commit -m "refactor: organize native runtime modules"
```
