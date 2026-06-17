use obscura_native_runtime::{NativeRuntime, RuntimeOptions};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::json;

const TRACKING_NUMBER: &str = "15504481097205";
const PAGE_URL: &str = "https://t.17track.net/en#nums=";
const API_URL: &str = "https://t.17track.net/track/restapi";
const FINGERPRINT_CHUNK_URL: &str =
    "https://static.17track.net/t/2026-06/_next/static/chunks/ff19fa74.3f3e88cff61b4f59.js";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";

const WEBPACK_CAPTURE_JS: &str = r#"
(() => {
  window.self = window;
  window.__track17Modules = {};
  window.__track17Cache = {};
  window.webpackChunk_N_E = [];
  window.webpackChunk_N_E.push = function(chunk) {
    const modules = (chunk && chunk[1]) || {};
    Object.keys(modules).forEach((id) => {
      window.__track17Modules[id] = modules[id];
    });
  };

  function requireModule(id) {
    id = String(id);
    if (window.__track17Cache[id]) return window.__track17Cache[id].exports;

    const factory = window.__track17Modules[id];
    if (!factory) throw new Error("missing webpack module " + id);

    const mod = { exports: {} };
    window.__track17Cache[id] = mod;
    factory(mod, mod.exports, requireModule);
    return mod.exports;
  }

  requireModule.r = function(exports) {
    Object.defineProperty(exports, "__esModule", { value: true });
    if (typeof Symbol !== "undefined" && Symbol.toStringTag) {
      Object.defineProperty(exports, Symbol.toStringTag, { value: "Module" });
    }
  };

  requireModule.d = function(exports, definition) {
    for (const key in definition) {
      if (!Object.prototype.hasOwnProperty.call(exports, key)) {
        Object.defineProperty(exports, key, {
          enumerable: true,
          get: definition[key]
        });
      }
    }
  };

  requireModule.g = globalThis;
  window.__track17Require = requireModule;
})();
"#;

const GENERATE_SIGN_JS: &str = r#"
(() => {
  window.YQ = window.YQ || {};
  window.YQ.configs = window.YQ.configs || {};
  window.YQ.configs.lang = "en";
  window.YQ.configs.md5 = "1.0.182";
  try {
    document.cookie = "country=CN; path=/; domain=17track.net";
    document.cookie = "_yq_bid=G-ADF043D32170B638; path=/; domain=17track.net";
    document.cookie = "v5_Culture=en; path=/; domain=17track.net";
  } catch (error) {}
  window.__track17Fingerprint = { done: false, error: null, sign: null };
  (async () => {
    try {
      const module4279 = window.__track17Require(4279);
      await module4279.default();
      window.__track17Fingerprint.sign = module4279.get_fingerprint([]);
    } catch (error) {
      window.__track17Fingerprint.error =
        error && (error.stack || error.message || String(error));
    } finally {
      window.__track17Fingerprint.done = true;
    }
  })();
})();
"#;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut number = TRACKING_NUMBER.to_string();
    let mut no_post = false;
    let mut trace = false;
    for arg in std::env::args().skip(1) {
        if arg == "--no-post" {
            no_post = true;
        } else if arg == "--trace" {
            trace = true;
        } else {
            number = arg;
        }
    }

    let page_url = format!("{PAGE_URL}{number}");
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    println!("[main] fetching fingerprint chunk");
    let chunk_js = client
        .get(FINGERPRINT_CHUNK_URL)
        .headers(browser_headers(Some(&page_url))?)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    println!(
        "[fetch] 200 {} ({} chars)",
        FINGERPRINT_CHUNK_URL,
        chunk_js.len()
    );

    println!("[main] executing get_fingerprint in obscura-native-runtime");
    let sign = generate_sign(&chunk_js, &page_url, trace)?;
    let body = json!({
        "data": [{"num": number, "fc": 0, "sc": 0}],
        "guid": "",
        "timeZoneOffset": -480,
        "sign": sign,
    });

    println!("[sign] length={}", sign.len());
    println!("[sign] preview={}...", &sign[..sign.len().min(96)]);
    println!("[body]");
    println!("{}", serde_json::to_string_pretty(&body)?);

    if no_post {
        return Ok(());
    }

    println!("[main] POST {API_URL}");
    let payload = tracking_payload(&number, &sign)?;
    let response = client
        .post(API_URL)
        .headers(browser_headers(Some(&page_url))?)
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header("Origin", "https://t.17track.net")
        .body(payload)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    println!("[response] HTTP {}", status.as_u16());
    println!("{}", &text[..text.len().min(4000)]);
    Ok(())
}

fn tracking_payload(number: &str, sign: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let number = serde_json::to_string(number)?;
    let sign = serde_json::to_string(sign)?;
    Ok(format!(
        r#"{{"data":[{{"num":{number},"fc":0,"sc":0}}],"guid":"","timeZoneOffset":-480,"sign":{sign}}}"#
    )
    .into_bytes())
}

fn generate_sign(
    chunk_js: &str,
    page_url: &str,
    trace: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut runtime = NativeRuntime::new(RuntimeOptions {
        url: page_url.to_string(),
        user_agent: USER_AGENT.to_string(),
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

    runtime.eval_json(WEBPACK_CAPTURE_JS)?;
    runtime.eval_json(chunk_js)?;

    let modules = runtime.eval_json("Object.keys(window.__track17Modules)")?;
    println!("[native] captured modules: {modules}");
    if !modules
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.as_str() == Some("4279")))
    {
        return Err("fingerprint module 4279 was not captured".into());
    }

    runtime.eval_json(GENERATE_SIGN_JS)?;
    for _ in 0..50 {
        runtime.drain_event_loop()?;
        if runtime
            .eval_json("window.__track17Fingerprint.done")?
            .as_bool()
            .unwrap_or(false)
        {
            break;
        }
    }

    let result = runtime.eval_json("window.__track17Fingerprint")?;
    if !result
        .get("done")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Err(format!("fingerprint did not finish: {result}").into());
    }
    if let Some(error) = result.get("error").and_then(|value| value.as_str()) {
        if !error.is_empty() {
            return Err(error.to_string().into());
        }
    }
    let sign = result
        .get("sign")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or("get_fingerprint returned an empty sign")?;
    if trace {
        println!("[trace]");
        println!("{}", serde_json::to_string_pretty(&runtime.trace_json())?);
    }
    Ok(sign.to_string())
}

fn browser_headers(referer: Option<&str>) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    let values = [
        ("User-Agent", USER_AGENT),
        ("Accept", "*/*"),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("Accept-Encoding", "gzip, identity"),
        ("Cache-Control", "no-cache"),
        ("Pragma", "no-cache"),
    ];
    for (name, value) in values {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes())?,
            HeaderValue::from_str(value)?,
        );
    }
    if let Some(referer) = referer {
        headers.insert("Referer", HeaderValue::from_str(referer)?);
    }
    Ok(headers)
}
