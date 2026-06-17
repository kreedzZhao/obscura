# Obscura Native Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a parallel native V8 runtime crate that proves the iv8-like direction without replacing the existing deno_core runtime.

**Architecture:** Create `obscura-native-runtime` as an independent workspace crate. It embeds the bare `v8` crate directly, registers a minimal browser global surface with native accessors, and stores an `obscura-dom` tree behind the runtime for document access.

**Tech Stack:** Rust 2021, `v8 = 137.3.0`, `obscura-dom`, `serde_json`, `html5ever` parser via `obscura-dom`.

## Global Constraints

- Do not modify the existing `obscura-js` runtime path.
- Do not implement monitor/tracing yet.
- Keep the first slice small: eval, window/navigator/screen/document, and title/URL/DOM parsing basics.
- Tests must be written before production code.

---

### Task 1: Native Runtime Crate Skeleton

**Files:**
- Modify: `obscura/Cargo.toml`
- Create: `obscura/crates/obscura-native-runtime/Cargo.toml`
- Create: `obscura/crates/obscura-native-runtime/src/lib.rs`
- Test: `obscura/crates/obscura-native-runtime/tests/runtime_smoke.rs`

**Interfaces:**
- Produces: `NativeRuntime::new(options: RuntimeOptions) -> Self`
- Produces: `NativeRuntime::eval_json(&mut self, source: &str) -> Result<serde_json::Value, RuntimeError>`
- Produces: `RuntimeOptions { url: String, user_agent: String, platform: String, hardware_concurrency: u32, screen_width: u32, screen_height: u32, color_depth: u32 }`

- [x] Write failing tests for eval and native browser globals.
- [x] Run the package test and confirm the crate is missing.
- [x] Add the crate and minimal runtime implementation.
- [x] Run the package test and confirm it passes.

### Task 2: DOM Bridge Minimal Slice

**Files:**
- Modify: `obscura/crates/obscura-native-runtime/src/lib.rs`
- Test: `obscura/crates/obscura-native-runtime/tests/runtime_smoke.rs`

**Interfaces:**
- Produces: `NativeRuntime::load_html(&mut self, html: &str) -> Result<(), RuntimeError>`
- Browser JS surface includes `document.URL`, `document.title`, `document.nodeType`, and `document.toString()`.

- [x] Write failing tests for loading HTML and reading `document.title`.
- [x] Run the package test and confirm the title assertion fails.
- [x] Implement `load_html` using `obscura_dom::parse_html`.
- [x] Run the package test and confirm it passes.

### Task 3: Workspace Validation

**Files:**
- Modify: `obscura/Cargo.lock`

- [x] Run `cargo test -p obscura-native-runtime`.
- [x] Run `cargo check -p obscura-native-runtime`.
- [x] Inspect `git status --short`.
