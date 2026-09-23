use rquickjs::module::Evaluated;
use rquickjs::prelude::Opt;
use rquickjs::{Context, Ctx, Exception, Function, Module, Object, Promise, Runtime, Value};
use selo_core::{
    Error, Image, OcrProvider, Rect, Result, TextBlock, TranslateProvider, TranslateRequest,
};
use selo_plugin::PluginMeta;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TRANSLATE_MEMORY: usize = 16 * 1024 * 1024;
const OCR_MEMORY: usize = 64 * 1024 * 1024;

const CRYPTO: &str = r#"(() => {
const K = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
function sha256(text) { return hex(digest(utf8(text))); }
function digest(bytes) {
    const length = bytes.length;
    const bitLo = (length << 3) >>> 0;
    const bitHi = Math.floor(length / 536870912);
    bytes = bytes.slice();
    bytes.push(0x80);
    while (bytes.length % 64 !== 56) bytes.push(0);
    for (const word of [bitHi, bitLo]) {
        bytes.push((word >>> 24) & 255, (word >>> 16) & 255, (word >>> 8) & 255, word & 255);
    }
    const h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    const w = new Array(64);
    const rr = (x, n) => (x >>> n) | (x << (32 - n));
    for (let i = 0; i < bytes.length; i += 64) {
        for (let j = 0; j < 16; j++) {
            const k = i + j * 4;
            w[j] = (bytes[k] << 24) | (bytes[k + 1] << 16) | (bytes[k + 2] << 8) | bytes[k + 3];
        }
        for (let j = 16; j < 64; j++) {
            const x = w[j - 15];
            const y = w[j - 2];
            w[j] = (w[j - 16] + (rr(x, 7) ^ rr(x, 18) ^ (x >>> 3)) + w[j - 7] + (rr(y, 17) ^ rr(y, 19) ^ (y >>> 10))) | 0;
        }
        let [a, b, c, d, e, f, g, hh] = h;
        for (let j = 0; j < 64; j++) {
            const t1 = (hh + (rr(e, 6) ^ rr(e, 11) ^ rr(e, 25)) + ((e & f) ^ (~e & g)) + K[j] + w[j]) | 0;
            const t2 = ((rr(a, 2) ^ rr(a, 13) ^ rr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c))) | 0;
            hh = g; g = f; f = e; e = (d + t1) | 0;
            d = c; c = b; b = a; a = (t1 + t2) | 0;
        }
        h[0] = (h[0] + a) | 0; h[1] = (h[1] + b) | 0; h[2] = (h[2] + c) | 0; h[3] = (h[3] + d) | 0;
        h[4] = (h[4] + e) | 0; h[5] = (h[5] + f) | 0; h[6] = (h[6] + g) | 0; h[7] = (h[7] + hh) | 0;
    }
    const out = [];
    for (const word of h) out.push((word >>> 24) & 255, (word >>> 16) & 255, (word >>> 8) & 255, word & 255);
    return out;
}
function utf8(text) {
    const out = [];
    for (const ch of text) {
        const c = ch.codePointAt(0);
        if (c < 0x80) out.push(c);
        else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
        else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
        else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    }
    return out;
}
function hex(bytes) {
    return bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
}
return { sha256 };
})()"#;

fn install_crypto<'js>(ctx: &Ctx<'js>) -> Result<()> {
    let crypto: Object = ctx.eval(CRYPTO).map_err(rt)?;
    ctx.globals().set("crypto", crypto).map_err(rt)?;
    Ok(())
}

pub struct JsPlugin {
    meta: PluginMeta,
    source: String,
    config: HashMap<String, String>,
    client: reqwest::blocking::Client,
}

impl JsPlugin {
    pub fn new(source: String, config: HashMap<String, String>) -> Result<Self> {
        let meta = meta(&source)?;
        let config = resolve_config(&meta, config)?;
        let client = client(&meta)?;
        Ok(Self {
            meta,
            source,
            config,
            client,
        })
    }

    pub fn meta(&self) -> &PluginMeta {
        &self.meta
    }
}

impl TranslateProvider for JsPlugin {
    fn translate(&self, request: &TranslateRequest) -> Result<String> {
        let text = request.text.trim();
        if text.is_empty() {
            return Err(Error::NoSelection);
        }
        let runtime = runtime(self.meta.timeout_ms, TRANSLATE_MEMORY)?;
        let context = Context::full(&runtime).map_err(rt)?;
        context.with(|ctx| {
            let module = load(&ctx, &self.source).map_err(|err| caught(&ctx, err))?;
            let translate: Function = module.get("translate").map_err(|err| caught(&ctx, err))?;

            let config = Object::new(ctx.clone()).map_err(rt)?;
            for (key, value) in &self.config {
                config.set(key.as_str(), value.as_str()).map_err(rt)?;
            }
            ctx.globals().set("config", config).map_err(rt)?;
            ctx.globals()
                .set("fetch", fetch(&ctx, &self.meta, &self.client)?)
                .map_err(rt)?;
            install_crypto(&ctx)?;

            let out: Value = translate
                .call((text, request.from.code(), request.to.as_str()))
                .map_err(|err| caught(&ctx, err))?;
            let translated: String = if out.is_promise() {
                Promise::from_value(out)
                    .map_err(|err| caught(&ctx, err))?
                    .finish()
                    .map_err(|err| caught(&ctx, err))?
            } else {
                out.get().map_err(|err| caught(&ctx, err))?
            };
            Ok(translated.trim().to_string())
        })
    }
}

pub struct JsOcr {
    meta: PluginMeta,
    source: String,
    config: HashMap<String, String>,
    client: reqwest::blocking::Client,
}

impl JsOcr {
    pub fn new(source: String, config: HashMap<String, String>) -> Result<Self> {
        let meta = meta(&source)?;
        let config = resolve_config(&meta, config)?;
        let client = client(&meta)?;
        Ok(Self {
            meta,
            source,
            config,
            client,
        })
    }
}

impl OcrProvider for JsOcr {
    fn recognize(&self, image: &Image) -> Result<Vec<TextBlock>> {
        if image.png.is_empty() {
            return Err(Error::Ocr("empty image".into()));
        }
        let payload = base64(&image.png);
        let runtime = runtime(self.meta.timeout_ms, OCR_MEMORY)?;
        let context = Context::full(&runtime).map_err(rt)?;
        context.with(|ctx| {
            let module = load(&ctx, &self.source).map_err(|err| caught(&ctx, err))?;
            let ocr: Function = module.get("ocr").map_err(|err| caught(&ctx, err))?;

            let config = Object::new(ctx.clone()).map_err(rt)?;
            for (key, value) in &self.config {
                config.set(key.as_str(), value.as_str()).map_err(rt)?;
            }
            ctx.globals().set("config", config).map_err(rt)?;
            ctx.globals()
                .set("fetch", fetch(&ctx, &self.meta, &self.client)?)
                .map_err(rt)?;
            install_crypto(&ctx)?;

            let out: Value = ocr.call((payload,)).map_err(|err| caught(&ctx, err))?;
            let out = if out.is_promise() {
                Promise::from_value(out)
                    .map_err(|err| caught(&ctx, err))?
                    .finish()
                    .map_err(|err| caught(&ctx, err))?
            } else {
                out
            };
            let json = json_value(&ctx, &out).map_err(|err| caught(&ctx, err))?;
            let blocks: Vec<Block> = serde_json::from_str(&json).map_err(|err| {
                Error::Ocr(format!(
                    "plugin `{}` returned invalid blocks: {err}",
                    self.meta.id
                ))
            })?;
            Ok(blocks.into_iter().map(Into::into).collect())
        })
    }
}

#[derive(serde::Deserialize)]
struct Block {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text: String,
    #[serde(default = "default_confidence")]
    confidence: f32,
}

fn default_confidence() -> f32 {
    1.
}

impl From<Block> for TextBlock {
    fn from(block: Block) -> Self {
        Self {
            rect: Rect {
                x: block.x,
                y: block.y,
                width: block.width,
                height: block.height,
            },
            text: block.text,
            confidence: block.confidence,
        }
    }
}

pub fn meta(source: &str) -> Result<PluginMeta> {
    let runtime = runtime(0, TRANSLATE_MEMORY)?;
    let context = Context::full(&runtime).map_err(rt)?;
    context.with(|ctx| {
        let module = load(&ctx, source).map_err(|err| caught(&ctx, err))?;
        let meta: Object = module.get("meta").map_err(|err| caught(&ctx, err))?;
        let json: String = stringify(&ctx, &meta).map_err(|err| caught(&ctx, err))?;
        serde_json::from_str(&json)
            .map_err(|err| Error::Translate(format!("plugin meta is invalid: {err}")))
    })
}

fn caught(ctx: &Ctx, err: rquickjs::Error) -> Error {
    let value = ctx.catch();
    if value.is_undefined() || value.is_null() {
        return rt(err);
    }
    let message = value
        .as_object()
        .and_then(|obj| obj.get::<_, Option<String>>("message").ok().flatten())
        .or_else(|| value.as_string().and_then(|text| text.to_string().ok()))
        .unwrap_or_else(|| value.type_name().to_string());
    Error::Translate(format!("plugin: {message}"))
}

fn runtime(timeout_ms: u64, memory: usize) -> Result<Runtime> {
    let runtime = Runtime::new().map_err(rt)?;
    runtime.set_memory_limit(memory);
    runtime.set_max_stack_size(512 * 1024);
    // Blocking `fetch` time is not counted by the interrupt, hence the slack.
    let deadline = Instant::now() + Duration::from_millis(timeout_ms + 1000);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    Ok(runtime)
}

fn load<'js>(ctx: &Ctx<'js>, source: &str) -> rquickjs::Result<Module<'js, Evaluated>> {
    let module = Module::declare(ctx.clone(), "plugin", source.to_string())?;
    let (module, promise) = module.eval()?;
    promise.finish::<()>()?;
    Ok(module)
}

fn stringify<'js>(ctx: &Ctx<'js>, value: &Object<'js>) -> rquickjs::Result<String> {
    let json: Object = ctx.globals().get("JSON")?;
    let stringify: Function = json.get("stringify")?;
    stringify.call((value.clone(),))
}

fn fetch<'js>(
    ctx: &Ctx<'js>,
    meta: &PluginMeta,
    client: &reqwest::blocking::Client,
) -> Result<Function<'js>> {
    let allowed = meta.allow_hosts.clone();
    let client = client.clone();
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>,
              url: String,
              options: Opt<Object<'js>>|
              -> rquickjs::Result<Object<'js>> {
            let host = selo_plugin::host(&url)
                .ok_or_else(|| Exception::throw_message(&ctx, "fetch: url has no host"))?;
            if !allowed
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(host))
            {
                return Err(Exception::throw_message(
                    &ctx,
                    &format!("fetch: host `{host}` is not in allow_hosts"),
                ));
            }

            let mut method = "GET".to_string();
            let mut headers = Vec::new();
            let mut body = None;
            if let Some(options) = options.0 {
                if let Some(value) = options.get::<_, Option<String>>("method")? {
                    method = value;
                }
                if let Some(value) = options.get::<_, Option<String>>("body")? {
                    body = Some(value);
                }
                if let Some(map) = options.get::<_, Option<Object>>("headers")? {
                    for entry in map.props::<String, String>() {
                        headers.push(entry?);
                    }
                }
            }

            let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|err| {
                Exception::throw_message(&ctx, &format!("fetch: bad method: {err}"))
            })?;
            let mut request = client.request(method, &url);
            for (name, value) in &headers {
                request = request.header(name, value);
            }
            if let Some(body) = body {
                request = request.body(body);
            }
            let response = request
                .send()
                .map_err(|err| Exception::throw_message(&ctx, &format!("fetch: {err}")))?;
            let status = response.status();
            let text = response
                .text()
                .map_err(|err| Exception::throw_message(&ctx, &format!("fetch: {err}")))?;

            let out = Object::new(ctx.clone())?;
            out.set("ok", status.is_success())?;
            out.set("status", status.as_u16())?;
            out.set("text", text)?;
            Ok(out)
        },
    )
    .map_err(rt)
}

fn resolve_config(
    meta: &PluginMeta,
    mut config: HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    for field in &meta.config {
        if config.contains_key(&field.key) {
            continue;
        }
        match &field.default {
            Some(default) => {
                config.insert(field.key.clone(), default.clone());
            }
            None if field.required => {
                return Err(Error::Translate(format!(
                    "plugin `{}`: missing required config `{}`",
                    meta.id, field.key
                )));
            }
            None => {}
        }
    }
    Ok(config)
}

fn client(meta: &PluginMeta) -> Result<reqwest::blocking::Client> {
    let mut builder =
        reqwest::blocking::Client::builder().timeout(Duration::from_millis(meta.timeout_ms));
    if meta.no_proxy {
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|err| Error::Translate(err.to_string()))
}

fn json_value<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> rquickjs::Result<String> {
    let json: Object = ctx.globals().get("JSON")?;
    let stringify: Function = json.get("stringify")?;
    stringify.call((value.clone(),))
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18 & 63) as usize] as char);
        out.push(TABLE[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn rt(err: impl std::fmt::Display) -> Error {
    Error::Translate(format!("plugin: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECHO: &str = r#"
export const meta = { id: "echo", name: "Echo", version: "1.0.0" };
export async function translate(text, from, to) { return `${text.toUpperCase()}/${from}/${to}`; }
"#;

    const OCR_ECHO: &str = r#"
export const meta = { id: "ocr-echo", name: "OCR Echo", version: "1.0.0", kind: "ocr" };
export function ocr(png) { return [{ x: 1, y: 2, width: 3, height: 4, text: png.slice(0, 4) }]; }
"#;

    #[test]
    fn reads_meta_with_defaults() {
        let meta = meta(ECHO).unwrap();
        assert_eq!(meta.id, "echo");
        assert_eq!(meta.timeout_ms, 30_000);
        assert!(meta.allow_hosts.is_empty());
    }

    #[test]
    fn parses_a_conditional_config_field() {
        let source = r#"
export const meta = { id: "cond", name: "Cond", version: "1.0.0",
  config: [
    { key: "type", label: "Mode", type: "string", options: ["free", "api"] },
    { key: "api_key", label: "Key", type: "secret", show_if: { key: "type", value: "api" } },
  ] };
export function translate(text) { return text; }
"#;
        let meta = meta(source).unwrap();
        assert!(meta.config[0].show_if.is_none());
        let show = meta.config[1].show_if.as_ref().unwrap();
        assert_eq!(show.key, "type");
        assert_eq!(show.value, "api");
    }

    #[test]
    fn runs_an_async_translate_without_network() {
        let plugin = JsPlugin::new(ECHO.into(), HashMap::new()).unwrap();
        let out = plugin.translate(&TranslateRequest::auto("hello")).unwrap();
        assert_eq!(out, "HELLO/en/zh");
    }

    #[test]
    fn config_defaults_reach_the_script() {
        let source = r#"
export const meta = { id: "cfg", name: "Cfg", version: "1.0.0",
  config: [{ key: "tag", label: "Tag", type: "string", default: "d" }] };
export function translate(text) { return `${config.tag}:${text}`; }
"#;
        let plugin = JsPlugin::new(source.into(), HashMap::new()).unwrap();
        assert_eq!(
            plugin.translate(&TranslateRequest::auto("x")).unwrap(),
            "d:x"
        );
    }

    #[test]
    fn a_required_config_field_is_enforced() {
        let source = r#"
export const meta = { id: "need", name: "Need", version: "1.0.0",
  config: [{ key: "api_key", label: "Key", type: "secret", required: true }] };
export function translate(text) { return text; }
"#;
        assert!(JsPlugin::new(source.into(), HashMap::new()).is_err());
    }

    #[test]
    fn ocr_plugin_maps_blocks_back_to_text_blocks() {
        let provider = JsOcr::new(OCR_ECHO.into(), HashMap::new()).unwrap();
        let image = Image {
            width: 1,
            height: 1,
            png: b"PNG!".to_vec(),
        };
        let blocks = provider.recognize(&image).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].rect.x, 1.);
        assert_eq!(blocks[0].rect.height, 4.);
        assert_eq!(blocks[0].text, "UE5H");
        assert_eq!(blocks[0].confidence, 1.);
    }

    #[test]
    fn bing_manifest_parses() {
        let meta = meta(include_str!("../../../plugins/bing.js")).unwrap();
        assert_eq!(meta.id, "bing");
        assert_eq!(meta.allow_hosts, ["edge.microsoft.com"]);
    }

    fn plugin_code(manifest: &str, lang: &str) -> String {
        let source = format!("{manifest}\nexport {{ code }};");
        let runtime = runtime(0, TRANSLATE_MEMORY).unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let module = load(&ctx, &source).unwrap();
            let code: Function = module.get("code").unwrap();
            code.call((lang,)).unwrap()
        })
    }

    #[test]
    fn chinese_codes_map_per_plugin() {
        let google = include_str!("../../../plugins/google.js");
        let bing = include_str!("../../../plugins/bing.js");
        let youdao = include_str!("../../../plugins/youdao.js");
        let deepl = include_str!("../../../plugins/deepl.js");
        for (manifest, input, expected) in [
            (google, "zh", "zh-CN"),
            (google, "zh-Hant", "zh-TW"),
            (google, "en", "en"),
            (bing, "zh", "zh-Hans"),
            (bing, "zh-Hant", "zh-Hant"),
            (youdao, "zh", "zh-CHS"),
            (youdao, "zh-Hant", "zh-CHT"),
            (youdao, "ja", "jp"),
            (youdao, "es", "spa"),
            (youdao, "vi", "vie"),
            (youdao, "en", "en"),
            (deepl, "zh", "ZH"),
            (deepl, "zh-Hant", "ZH"),
            (deepl, "pt", "PT-BR"),
        ] {
            assert_eq!(plugin_code(manifest, input), expected, "code({input})");
        }
    }

    #[test]
    fn youdao_manifest_parses() {
        let meta = meta(include_str!("../../../plugins/youdao.js")).unwrap();
        assert_eq!(meta.id, "youdao");
        assert_eq!(meta.allow_hosts, ["openapi.youdao.com"]);
        assert!(meta.config[0].required && meta.config[1].required);
    }

    #[test]
    fn local_manifest_parses() {
        let meta = meta(include_str!("../../../plugins/local.js")).unwrap();
        assert_eq!(meta.id, "local");
        assert!(meta.no_proxy);
        assert_eq!(meta.allow_hosts, ["127.0.0.1", "localhost"]);
        assert_eq!(
            meta.config[0].default.as_deref(),
            Some("http://127.0.0.1:11434/v1")
        );
        assert!(meta.config[1].required);
    }

    #[test]
    fn ocrspace_manifest_parses() {
        let meta = meta(include_str!("../../../plugins/ocrspace.js")).unwrap();
        assert_eq!(meta.id, "ocrspace");
        assert_eq!(meta.kind, selo_plugin::PluginKind::Ocr);
        assert_eq!(meta.allow_hosts, ["api.ocr.space"]);
    }

    #[test]
    fn ocrspace_maps_overlay_lines_to_blocks() {
        let source = include_str!("../../../plugins/ocrspace.js");
        let runtime = runtime(0, OCR_MEMORY).unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let module = load(&ctx, source).unwrap();
            let blocks: Function = module.get("blocks").unwrap();
            let data: Value = ctx
                .eval(
                    r#"({ ParsedResults: [{ FileParseExitCode: 1, TextOverlay: { Lines: [
                        { LineText: "Hello world", MinTop: 90, MaxHeight: 13, Words: [
                            { WordText: "Hello", Left: 106, Top: 91, Width: 11, Height: 9 },
                            { WordText: "world", Left: 121, Top: 90, Width: 51, Height: 13 }
                        ] }
                    ] } }] })"#,
                )
                .unwrap();
            let out: Value = blocks.call((data,)).unwrap();
            let json: String = json_value(&ctx, &out).unwrap();
            assert_eq!(
                json,
                r#"[{"x":106,"y":90,"width":66,"height":13,"text":"Hello world","confidence":1}]"#
            );
        });
    }

    #[test]
    fn crypto_global_matches_known_vectors() {
        let runtime = runtime(0, TRANSLATE_MEMORY).unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let crypto: Object = ctx.eval(CRYPTO).unwrap();
            let sha256: Function = crypto.get("sha256").unwrap();
            let digest: String = sha256.call(("abc",)).unwrap();
            assert_eq!(
                digest,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
        });
    }

    #[test]
    fn meta_reports_the_plugin_kind() {
        assert_eq!(meta(OCR_ECHO).unwrap().kind, selo_plugin::PluginKind::Ocr);
        assert_eq!(meta(ECHO).unwrap().kind, selo_plugin::PluginKind::Translate);
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn fetch_rejects_a_host_outside_allow_hosts() {
        let source = r#"
export const meta = { id: "nope", name: "Nope", version: "1.0.0", allow_hosts: ["ok.example"] };
export async function translate(text) {
    const res = await fetch("https://evil.example/");
    return res.text;
}
"#;
        let plugin = JsPlugin::new(source.into(), HashMap::new()).unwrap();
        let err = plugin.translate(&TranslateRequest::auto("x")).unwrap_err();
        assert!(err.to_string().contains("allow_hosts"), "{err}");
    }
}
