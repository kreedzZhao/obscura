use obscura_native_runtime::{NativeRuntime, RuntimeOptions};
use serde_json::json;

#[test]
fn evaluates_javascript_and_browser_globals() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(runtime.eval_json("1 + 2").unwrap(), json!(3));
    assert_eq!(
        runtime.eval_json("window === globalThis").unwrap(),
        json!(true)
    );
    assert_eq!(
        runtime.eval_json("typeof Window").unwrap(),
        json!("function")
    );
    assert_eq!(
        runtime.eval_json("window instanceof Window").unwrap(),
        json!(true)
    );
    assert_eq!(
        runtime.eval_json("navigator.userAgent").unwrap(),
        json!(RuntimeOptions::default().user_agent)
    );
    assert_eq!(
        runtime.eval_json("navigator.webdriver").unwrap(),
        json!(false)
    );
    assert_eq!(
        runtime.eval_json("navigator.hardwareConcurrency").unwrap(),
        json!(8)
    );
    assert_eq!(runtime.eval_json("screen.width").unwrap(), json!(1920));
    assert_eq!(runtime.eval_json("screen.height").unwrap(), json!(1080));
    assert_eq!(runtime.eval_json("screen.availWidth").unwrap(), json!(1920));
    assert_eq!(
        runtime.eval_json("screen.availHeight").unwrap(),
        json!(1040)
    );
    assert_eq!(runtime.eval_json("screen.colorDepth").unwrap(), json!(24));
    assert_eq!(runtime.eval_json("screen.pixelDepth").unwrap(), json!(24));
    assert_eq!(
        runtime.eval_json("navigator.cookieEnabled").unwrap(),
        json!(true)
    );
    assert_eq!(
        runtime.eval_json("navigator.maxTouchPoints").unwrap(),
        json!(0)
    );
    assert_eq!(
        runtime.eval_json("navigator.plugins.length").unwrap(),
        json!(5)
    );
    assert_eq!(
        runtime.eval_json("navigator.mimeTypes.length").unwrap(),
        json!(2)
    );
    assert_eq!(runtime.eval_json("document.nodeType").unwrap(), json!(9));
    assert_eq!(
        runtime
            .eval_json("Object.prototype.toString.call(document)")
            .unwrap(),
        json!("[object Document]")
    );
    assert_eq!(
        runtime.eval_json("document.toString()").unwrap(),
        json!("[object Document]")
    );
}

#[test]
fn loads_html_into_native_document_state() {
    let mut runtime = NativeRuntime::new(RuntimeOptions {
        url: "https://example.test/path".to_string(),
        ..RuntimeOptions::default()
    });

    runtime
        .load_html(
            "<!doctype html><html><head><title>Native Slice</title></head><body></body></html>",
        )
        .unwrap();

    assert_eq!(
        runtime.eval_json("document.URL").unwrap(),
        json!("https://example.test/path")
    );
    assert_eq!(
        runtime.eval_json("document.title").unwrap(),
        json!("Native Slice")
    );
}

#[test]
fn query_selector_returns_native_element_wrappers() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());
    runtime
        .load_html(
            r#"<!doctype html>
            <html>
              <body>
                <main id="app" data-kind="root">
                  <span class="label">Hello Native DOM</span>
                </main>
              </body>
            </html>"#,
        )
        .unwrap();

    assert_eq!(
        runtime
            .eval_json("document.querySelector('#app').tagName")
            .unwrap(),
        json!("MAIN")
    );
    assert_eq!(
        runtime
            .eval_json("document.querySelector('#app').id")
            .unwrap(),
        json!("app")
    );
    assert_eq!(
        runtime
            .eval_json("document.querySelector('#app').getAttribute('data-kind')")
            .unwrap(),
        json!("root")
    );
    assert_eq!(
        runtime
            .eval_json("document.querySelector('.label').textContent")
            .unwrap(),
        json!("Hello Native DOM")
    );
    assert_eq!(
        runtime
            .eval_json("document.querySelector('.missing')")
            .unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(
        runtime
            .eval_json("Object.prototype.toString.call(document.querySelector('#app'))")
            .unwrap(),
        json!("[object Element]")
    );
}

#[test]
fn document_create_element_returns_canvas_shim() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(
        runtime
            .eval_json(
                r#"
                const canvas = document.createElement("canvas");
                const context = canvas.getContext("2d");
                const webgl = canvas.getContext("webgl");
                const debugInfo = webgl.getExtension("WEBGL_debug_renderer_info");
                context.fillRect(0, 0, 1, 1);
                context.fillText("x", 0, 0);
                context.beginPath();
                context.arc(1, 1, 1, 0, Math.PI);
                [
                    canvas.tagName,
                    canvas instanceof HTMLCanvasElement,
                    context instanceof CanvasRenderingContext2D,
                    webgl instanceof WebGLRenderingContext,
                    typeof context.fillRect,
                    typeof context.fillText,
                    typeof context.arc,
                    typeof context,
                    typeof webgl,
                    webgl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL),
                    webgl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL),
                    webgl.getSupportedExtensions().length,
                    typeof canvas.toDataURL(),
                    canvas.toDataURL()
                ]
                "#,
            )
            .unwrap(),
        json!([
            "CANVAS",
            true,
            true,
            true,
            "function",
            "function",
            "function",
            "object",
            "object",
            "Google Inc. (NVIDIA)",
            "ANGLE (NVIDIA, NVIDIA GeForce GTX 1650 (0x00001F82) Direct3D11 vs_5_0 ps_5_0, D3D11)",
            0,
            "string",
            ""
        ])
    );
}

#[test]
fn exposes_track17_fingerprint_environment() {
    let mut runtime = NativeRuntime::new(RuntimeOptions {
        url: "https://t.17track.net/en#nums=15504481097205".to_string(),
        user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36".to_string(),
        platform: "MacIntel".to_string(),
        screen_width: 1470,
        screen_height: 956,
        window_inner_width: 1470,
        window_inner_height: 956,
        window_outer_width: 1470,
        window_outer_height: 956,
        device_pixel_ratio: 2.0,
        ..RuntimeOptions::default()
    });

    assert_eq!(
        runtime.eval_json("location.href").unwrap(),
        json!("https://t.17track.net/en#nums=15504481097205")
    );
    assert_eq!(
        runtime.eval_json("navigator.language").unwrap(),
        json!("en-US")
    );
    assert_eq!(
        runtime.eval_json("navigator.languages").unwrap(),
        json!(["en-US", "en"])
    );
    assert_eq!(
        runtime.eval_json("navigator.deviceMemory").unwrap(),
        json!(8)
    );
    assert_eq!(runtime.eval_json("devicePixelRatio").unwrap(), json!(2));
    assert_eq!(runtime.eval_json("innerWidth").unwrap(), json!(1470));
    assert_eq!(runtime.eval_json("innerHeight").unwrap(), json!(956));
    assert_eq!(runtime.eval_json("outerWidth").unwrap(), json!(1470));
    assert_eq!(runtime.eval_json("outerHeight").unwrap(), json!(956));

    runtime
        .eval_json(
            r#"
            document.cookie = "country=CN; path=/; domain=17track.net";
            document.cookie = "v5_Culture=en; path=/; domain=17track.net";
            document.cookie
            "#,
        )
        .unwrap();
    assert_eq!(
        runtime.eval_json("document.cookie").unwrap(),
        json!("country=CN; v5_Culture=en")
    );
}

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

#[test]
fn drains_promise_microtasks() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    runtime
        .eval_json(
            r#"
            window.__done = false;
            Promise.resolve().then(() => { window.__done = true; });
            null
            "#,
        )
        .unwrap();

    assert_eq!(runtime.eval_json("window.__done").unwrap(), json!(true));
}

#[test]
fn exposes_timer_functions_for_browser_bundles() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(
        runtime
            .eval_json(
                r#"
                const interval = setInterval(() => {}, 1000);
                const timeout = setTimeout(() => {}, 0);
                clearInterval(interval);
                clearTimeout(timeout);
                [typeof interval, typeof timeout, interval > 0, timeout > interval]
                "#,
            )
            .unwrap(),
        json!(["number", "number", true, true])
    );
}

#[test]
fn set_timeout_advances_async_browser_code() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    runtime
        .eval_json(
            r#"
            window.__done = false;
            (async () => {
                await new Promise((resolve) => setTimeout(resolve, 0));
                window.__done = true;
            })();
            null
            "#,
        )
        .unwrap();

    assert_eq!(runtime.eval_json("window.__done").unwrap(), json!(true));
}

#[test]
fn drains_pending_timer_callbacks() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    runtime
        .eval_json(
            r#"
            window.__timerDone = false;
            setTimeout(() => { window.__timerDone = true; }, 0);
            null
            "#,
        )
        .unwrap();

    runtime.drain_event_loop().unwrap();

    assert_eq!(
        runtime.eval_json("window.__timerDone").unwrap(),
        json!(true)
    );
}

#[test]
fn drains_v8_platform_tasks_for_wasm_instantiation() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    runtime
        .eval_json(
            r#"
            window.__wasmReady = false;
            const wasm = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]).buffer;
            WebAssembly.instantiate(wasm).then(() => { window.__wasmReady = true; });
            null
            "#,
        )
        .unwrap();

    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(1));
        runtime.drain_event_loop().unwrap();
        if runtime
            .eval_json("window.__wasmReady")
            .unwrap()
            .as_bool()
            .unwrap_or(false)
        {
            break;
        }
    }

    assert_eq!(
        runtime.eval_json("window.__wasmReady").unwrap(),
        json!(true)
    );
}

#[test]
fn text_encoder_encodes_utf8_bytes() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(
        runtime
            .eval_json("Array.from(new TextEncoder().encode('A中'))")
            .unwrap(),
        json!([65, 228, 184, 173])
    );
}

#[test]
fn text_decoder_decodes_utf8_bytes() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(
        runtime
            .eval_json("new TextDecoder().decode(new Uint8Array([65, 228, 184, 173]))")
            .unwrap(),
        json!("A中")
    );
}

#[test]
fn exposes_base64_helpers() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(runtime.eval_json("atob('QUI=')").unwrap(), json!("AB"));
    assert_eq!(runtime.eval_json("btoa('AB')").unwrap(), json!("QUI="));
}

#[test]
fn exposes_crypto_get_random_values() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    assert_eq!(
        runtime
            .eval_json(
                r#"
                const values = new Uint8Array(4);
                const returned = crypto.getRandomValues(values);
                [returned === values, values.length, values.some((value) => value !== 0)]
                "#,
            )
            .unwrap(),
        json!([true, 4, true])
    );
}

#[test]
fn records_web_api_trace_events() {
    let mut runtime = NativeRuntime::new(RuntimeOptions::default());

    runtime
        .eval_json(
            r#"
            navigator.userAgent;
            screen.width;
            location.href;
            document.cookie = "country=CN; path=/";
            document.cookie;
            const canvas = document.createElement("canvas");
            const context = canvas.getContext("2d");
            context.fillText("track", 1, 2);
            const webgl = canvas.getContext("webgl");
            const info = webgl.getExtension("WEBGL_debug_renderer_info");
            webgl.getParameter(info.UNMASKED_RENDERER_WEBGL);
            crypto.getRandomValues(new Uint8Array(2));
            null
            "#,
        )
        .unwrap();

    let trace = runtime.take_trace();
    assert!(trace
        .iter()
        .any(|event| event.target == "navigator" && event.name == "userAgent"));
    assert!(trace
        .iter()
        .any(|event| event.target == "screen" && event.name == "width"));
    assert!(trace
        .iter()
        .any(|event| event.target == "location" && event.name == "href"));
    assert!(trace
        .iter()
        .any(|event| event.target == "document" && event.name == "cookie"));
    assert!(trace
        .iter()
        .any(|event| event.target == "document" && event.name == "createElement"));
    assert!(trace
        .iter()
        .any(|event| event.target == "HTMLCanvasElement" && event.name == "getContext"));
    assert!(trace
        .iter()
        .any(|event| event.target == "CanvasRenderingContext2D" && event.name == "fillText"));
    assert!(trace
        .iter()
        .any(|event| event.target == "WebGLRenderingContext" && event.name == "getParameter"));
    assert!(trace
        .iter()
        .any(|event| event.target == "crypto" && event.name == "getRandomValues"));
    assert!(runtime.trace().is_empty());
}

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
