<div align="center">
  <img src="assets/AppIcon.png" width="110" alt="Selo logo">
  <h1>Selo</h1>
  <p><a href="README.md">English</a> · 中文 · <a href="README.ko.md">한국어</a> · <a href="README.ja.md">日本語</a></p>
</div>

- **划词即译**：选中任意应用里的文本，按热键翻译
- **截图翻译**：框选屏幕区域 → OCR → 按 OCR 坐标把译文盖回原文位置
- **插件化引擎**：内置多个免 key 服务，也能写一个 ES module 接入任意翻译接口
- **本地优先**：密钥存系统钥匙串，不落盘明文
- **轻量**：单文件约 7 MB，无需额外运行时, 原生界面，启动快、内存占用低

<br>

| 划词即译 | 截图翻译 |
|---|---|
| <img src="assets/translate-on-select.gif" width="420" alt="划词即译"> | <img src="assets/screenshot-translation.gif" width="420" alt="截图翻译"> |

<br>

## 安装

从 [Releases](https://github.com/osdodo/selo/releases) 下载对应安装包。

**Linux**

```bash
sudo apt install ./selo_<ver>_amd64.deb      # Debian / Ubuntu
sudo dnf install ./selo-<ver>-1.x86_64.rpm   # Fedora / RHEL
```

**macOS**

1. 打开 dmg，把 **Selo.app** 拖进「应用程序」。
2. dmg 未公证，首次打开若提示「已损坏」，执行一次即可：

   ```bash
   xattr -dr com.apple.quarantine /Applications/Selo.app
   ```

3. 首次运行到「系统设置 → 隐私与安全」给 Selo 授权两项，之后重启 Selo：
   - **辅助功能**：划词取词
   - **屏幕录制**：截图 OCR

   若授权后仍反复弹窗，确认 app 在 `/Applications`（不要从 dmg 直接运行）并重启 app。

## 使用

| 热键 | 作用 |
|---|---|
| `⌥D` · `Ctrl+Alt+D` | 翻译选中的文本 |
| `⌥S` · `Ctrl+Alt+S` | 框选屏幕区域 → OCR → 翻译（拖拽选区，右键或单击取消） |

托盘菜单 →「设置…」可切换翻译服务、目标语言、热键与界面语言。

## 待办

- [ ] **多显示器**：窗口按光标所在屏打开；多屏坐标换算待硬件验证
- [ ] **本地 OCR**：CLI-only 本地 OCR（tesseract / PaddleOCR）待插件 `kind: "command"` 子进程支持
- [ ] **流式输出**：翻译结果流式返回（首个流式引擎落地时）

## 插件

引擎只走插件一条路径，未配置或构建失败时回退默认服务；请求跟随系统代理。

内置（默认显示 deepl / google / bing / ocrspace，其余在「添加内置插件」拉出）：

| id | 类型 | 备注 |
|---|---|---|
| `deepl` | 翻译 | 免 key `free` / 官方 `api` |
| `google` | 翻译 | 免 key 网页端点 / Cloud Translation `api` |
| `bing` | 翻译 | 免 key Microsoft Edge 端点 |
| `deepseek` | 翻译 | OpenAI 兼容，需 key |
| `youdao` | 翻译 | 官方 API，需 appkey / secret |
| `local` | 翻译 | OpenAI 兼容（Ollama / LM Studio / llama.cpp / vLLM），仅 `127.0.0.1` |
| `ocrspace` | OCR | 云端 OCR |

自定义插件是单个 ES module，放进用户数据目录的 `plugins/`（同 id 覆盖内置），或「设置… → 插件设置 → 添加外部插件」导入：

```js
export const meta = {
    id: "my-engine", name: "My Engine", version: "1.0.0",
    kind: "translate",              // 或 "ocr"
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

OCR 插件导出 `ocr(pngBase64)`，返回图片物理像素、左上原点的 `[{ x, y, width, height, text, confidence }]`。

`meta` 字段：

| 字段 | 说明 |
|---|---|
| `kind` | `translate`（默认）或 `ocr` |
| `timeout_ms` | 网络与 JS 超时，默认 `30000` |
| `allow_hosts` | 唯一可访问的域名白名单 |
| `no_proxy` | `true` 时直连，绕过系统代理 |
| `config[]` | 配置字段：`string` / `secret`、`required`、`default`、`options`（下拉）、`show_if`（联动） |

插件在全新 QuickJS 沙箱中运行，仅有宿主 `fetch`。

数据目录（含 `config.toml` 与 `plugins/`，密钥存系统钥匙串）：

| 平台 | 目录 |
|---|---|
| macOS | `~/Library/Application Support/Selo/` |
| Windows | `%APPDATA%\Selo\` |
| Linux | `$XDG_DATA_HOME/Selo/` |

## 许可

Licensed under [GPL-3.0-or-later](https://www.gnu.org/licenses/gpl-3.0.html).
