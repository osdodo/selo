<div align="center">
  <img src="assets/AppIcon.png" width="110" alt="Selo logo">
  <h1>Selo</h1>
  <p>English · <a href="README.zh.md">中文</a> · <a href="README.ko.md">한국어</a> · <a href="README.ja.md">日本語</a></p>
</div>

- **Translate on select**: select text in any app, press the hotkey to translate
- **Screenshot translation**: select a screen region → OCR → overlay the translation back over the original text at the OCR coordinates
- **Pluggable engines**: several key-free services built in, or write an ES module to plug in any translation API
- **Local-first**: keys live in the system keychain, never written to disk in plaintext
- **Lightweight**: ~7 MB single binary, no runtime needed; native UI, fast startup, low memory

<br>

| Translate on select | Screenshot translation |
|---|---|
| <img src="assets/translate-on-select.gif" width="420" alt="Translate on select"> | <img src="assets/screenshot-translation.gif" width="420" alt="Screenshot translation"> |

<br>

## Install

Download the package for your platform from [Releases](https://github.com/osdodo/selo/releases).

**Linux**

```bash
sudo apt install ./selo_<ver>_amd64.deb      # Debian / Ubuntu
sudo dnf install ./selo-<ver>-1.x86_64.rpm   # Fedora / RHEL
```

**macOS**

1. Open the dmg and drag **Selo.app** into Applications.
2. The dmg is not notarized; if the first launch says "damaged", run once:

   ```bash
   xattr -dr com.apple.quarantine /Applications/Selo.app
   ```

3. On first run, grant Selo two permissions in System Settings → Privacy & Security, then restart Selo:
   - **Accessibility**: reading the selected text
   - **Screen Recording**: screenshot OCR

   If it keeps prompting after granting, make sure the app is in `/Applications` (don't run it from the dmg) and restart the app.

## Usage

| Hotkey | Action |
|---|---|
| `⌥D` · `Ctrl+Alt+D` | Translate the selected text |
| `⌥S` · `Ctrl+Alt+S` | Select a screen region → OCR → translate (drag to select, right-click or click to cancel) |

Tray menu → "Settings…" switches the translation service, target language, hotkeys and UI language.

## TODO

- [ ] **Multi-monitor**: open windows on the display under the cursor; per-display coordinate conversion needs hardware verification
- [ ] **Local OCR**: CLI-only local OCR (tesseract / PaddleOCR) needs plugin `kind: "command"` subprocess support
- [ ] **Streaming**: stream translation results (when the first streaming engine lands)

## Plugins

Engines go through plugins only; falls back to the default service when none is configured or a plugin fails to build. Requests follow the system proxy.

Built in (deepl / google / bing / ocrspace shown by default; the rest live under "Add built-in plugin"):

| id | Type | Notes |
|---|---|---|
| `deepl` | Translate | key-free `free` / official `api` |
| `google` | Translate | key-free web endpoint / Cloud Translation `api` |
| `bing` | Translate | key-free Microsoft Edge endpoint |
| `deepseek` | Translate | OpenAI-compatible, needs a key |
| `youdao` | Translate | official API, needs appkey / secret |
| `local` | Translate | OpenAI-compatible (Ollama / LM Studio / llama.cpp / vLLM), `127.0.0.1` only |
| `ocrspace` | OCR | cloud OCR |

A custom plugin is a single ES module dropped into `plugins/` in the user data directory (same id overrides a built-in), or imported via "Settings… → Plugin settings → Add external plugin":

```js
export const meta = {
    id: "my-engine", name: "My Engine", version: "1.0.0",
    kind: "translate",              // or "ocr"
    timeout_ms: 15000,
    allow_hosts: ["api.example.com"],
    config: [{ key: "api_key", label: "API Key", type: "secret", required: true }],
};

export async function translate(text, from, to) {
    const res = await fetch("https://api.example.com/translate", {
        method: "POST",
        headers: { Authorization: `Bearer ${config.api_key}` },
        body: JSON.stringify({ text, from, to }),
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return JSON.parse(res.text).translation;
}
```

An OCR plugin exports `ocr(pngBase64)` and returns `[{ x, y, width, height, text, confidence }]` in the image's physical pixels, top-left origin.

`meta` fields:

| Field | Description |
|---|---|
| `kind` | `translate` (default) or `ocr` |
| `timeout_ms` | network and JS timeout, default `30000` |
| `allow_hosts` | only domains the plugin may reach |
| `no_proxy` | `true` connects directly, bypassing the system proxy |
| `config[]` | config fields: `string` / `secret`, `required`, `default`, `options` (dropdown), `show_if` (conditional) |

Plugins run in a fresh QuickJS sandbox per call with only the host `fetch`.

Data directory (holds `config.toml` and `plugins/`; keys go to the system keychain):

| Platform | Directory |
|---|---|
| macOS | `~/Library/Application Support/Selo/` |
| Windows | `%APPDATA%\Selo\` |
| Linux | `$XDG_DATA_HOME/Selo/` |

## License

Licensed under [GPL-3.0-or-later](https://www.gnu.org/licenses/gpl-3.0.html).
