<div align="center">
  <img src="assets/AppIcon.png" width="110" alt="Selo logo">
  <h1>Selo</h1>
  <p><a href="README.md">English</a> · <a href="README.zh.md">中文</a> · <a href="README.ko.md">한국어</a> · 日本語</p>
</div>

- **選択して翻訳**: 任意のアプリでテキストを選択し、ホットキーで翻訳
- **スクショ翻訳**: 画面範囲を選択 → OCR → OCR 座標に合わせて訳文を原文の位置に重ねる
- **プラグインエンジン**: キー不要のサービスを複数内蔵。ES module を書けば任意の翻訳 API を接続可能
- **ローカル優先**: キーはシステムキーチェーンに保存し、平文でディスクに残さない
- **軽量**: 単一ファイル約 7 MB、ランタイム不要。ネイティブ UI で起動が速く省メモリ

<br>

| 選択して翻訳 | スクショ翻訳 |
|---|---|
| <img src="assets/translate-on-select.gif" width="420" alt="選択して翻訳"> | <img src="assets/screenshot-translation.gif" width="420" alt="スクショ翻訳"> |

<br>

## インストール

お使いのプラットフォーム用のパッケージを [Releases](https://github.com/osdodo/selo/releases) からダウンロードしてください。

**Linux**

```bash
sudo apt install ./selo_<ver>_amd64.deb      # Debian / Ubuntu
sudo dnf install ./selo-<ver>-1.x86_64.rpm   # Fedora / RHEL
```

**macOS**

1. dmg を開き、**Selo.app** を「アプリケーション」にドラッグします。
2. dmg は公証されていません。初回起動で「壊れているため開けません」と表示されたら、一度だけ実行してください:

   ```bash
   xattr -dr com.apple.quarantine /Applications/Selo.app
   ```

3. 初回起動時に「システム設定 → プライバシーとセキュリティ」で Selo に 2 つの権限を許可し、Selo を再起動します:
   - **アクセシビリティ**: 選択テキストの取得
   - **画面収録**: スクショ OCR

   許可後も繰り返しダイアログが出る場合は、アプリが `/Applications` にあること（dmg から直接実行しない）を確認し、アプリを再起動してください。

## 使い方

| ホットキー | 動作 |
|---|---|
| `⌥D` · `Ctrl+Alt+D` | 選択中のテキストを翻訳 |
| `⌥S` · `Ctrl+Alt+S` | 画面範囲を選択 → OCR → 翻訳（ドラッグで選択、右クリックまたはクリックでキャンセル） |

トレイメニュー →「設定…」で翻訳サービス、対象言語、ホットキー、UI 言語を切り替えられます。

## TODO

- [ ] **マルチディスプレイ**: カーソルがあるディスプレイにウィンドウを開く。ディスプレイごとの座標変換は実機検証が必要
- [ ] **ローカル OCR**: CLI 専用のローカル OCR（tesseract / PaddleOCR）はプラグイン `kind: "command"` のサブプロセス対応が必要
- [ ] **ストリーミング**: 翻訳結果をストリーミング（最初のストリーミングエンジン導入時）

## プラグイン

エンジンはプラグイン経由のみです。未設定またはビルド失敗時は既定のサービスにフォールバックします。リクエストはシステムプロキシに従います。

内蔵（既定では deepl / google / bing / ocrspace を表示、残りは「内蔵プラグインを追加」から）:

| id | 種別 | 備考 |
|---|---|---|
| `deepl` | 翻訳 | キー不要 `free` / 公式 `api` |
| `google` | 翻訳 | キー不要 Web エンドポイント / Cloud Translation `api` |
| `bing` | 翻訳 | キー不要 Microsoft Edge エンドポイント |
| `deepseek` | 翻訳 | OpenAI 互換、キーが必要 |
| `youdao` | 翻訳 | 公式 API、appkey / secret が必要 |
| `local` | 翻訳 | OpenAI 互換（Ollama / LM Studio / llama.cpp / vLLM）、`127.0.0.1` のみ |
| `ocrspace` | OCR | クラウド OCR |

カスタムプラグインは単一の ES module で、ユーザーデータディレクトリの `plugins/` に置く（同じ id は内蔵を上書き）か、「設定… → プラグイン設定 → 外部プラグインを追加」から読み込みます:

```js
export const meta = {
    id: "my-engine", name: "My Engine", version: "1.0.0",
    kind: "translate",              // または "ocr"
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

OCR プラグインは `ocr(pngBase64)` をエクスポートし、画像の物理ピクセル・左上原点の `[{ x, y, width, height, text, confidence }]` を返します。

`meta` フィールド:

| フィールド | 説明 |
|---|---|
| `kind` | `translate`（既定）または `ocr` |
| `timeout_ms` | ネットワークと JS のタイムアウト、既定 `30000` |
| `allow_hosts` | アクセス可能なドメインの許可リスト |
| `no_proxy` | `true` でシステムプロキシを迂回して直結 |
| `config[]` | 設定フィールド: `string` / `secret`、`required`、`default`、`options`（ドロップダウン）、`show_if`（連動） |

プラグインは呼び出しごとに新しい QuickJS サンドボックスで実行され、ホストの `fetch` のみ利用できます。

データディレクトリ（`config.toml` と `plugins/` を含み、キーはシステムキーチェーンに保存）:

| プラットフォーム | ディレクトリ |
|---|---|
| macOS | `~/Library/Application Support/Selo/` |
| Windows | `%APPDATA%\Selo\` |
| Linux | `$XDG_DATA_HOME/Selo/` |

## ライセンス

Licensed under [GPL-3.0-or-later](https://www.gnu.org/licenses/gpl-3.0.html).
