export const meta = {
    id: "deepl",
    name: "DeepL",
    version: "1.0.0",
    timeout_ms: 30000,
    no_proxy: true,
    allow_hosts: ["www2.deepl.com", "api.deepl.com", "api-free.deepl.com", "api.deepl-pro.com"],
    config: [
        {
            key: "type",
            label: "Mode",
            type: "string",
            default: "free",
            options: ["free", "api"],
        },
        {
            key: "api_key",
            label: "DeepL API Key (api mode; includes the :fx / :dp suffix)",
            type: "secret",
            show_if: { key: "type", value: "api" },
        },
    ],
};

export async function translate(text, from, to) {
    return config.type === "api"
        ? translateApi(text, from, to)
        : translateFree(text, from, to);
}

async function translateApi(text, from, to) {
    const key = config.api_key;
    if (!key) throw new Error("api 模式需要填写 DeepL API Key");
    const url = key.endsWith(":fx")
        ? "https://api-free.deepl.com/v2/translate"
        : key.endsWith(":dp")
          ? "https://api.deepl-pro.com/v2/translate"
          : "https://api.deepl.com/v2/translate";
    const body = { text: [text], target_lang: code(to) };
    if (from !== "auto") body.source_lang = code(from);
    const res = await fetch(url, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            Authorization: `DeepL-Auth-Key ${key}`,
        },
        body: JSON.stringify(body),
    });
    const data = JSON.parse(res.text);
    if (!res.ok) throw new Error(data.message ?? data.error?.message ?? `HTTP ${res.status}`);
    return data.translations[0].text;
}

async function translateFree(text, from, to) {
    const i = text.split("i").length - 1;
    let timestamp = Date.now();
    if (i !== 0) {
        const step = i + 1;
        timestamp = timestamp - (timestamp % step) + step;
    }
    const id = (Math.floor(Math.random() * 99999) + 100000) * 1000;
    const body = JSON.stringify({
        jsonrpc: "2.0",
        method: "LMT_handle_texts",
        params: {
            splitting: "newlines",
            lang: {
                source_lang_user_selected: code(from).slice(0, 2),
                target_lang: code(to).slice(0, 2),
            },
            texts: [{ text, requestAlternatives: 3 }],
            timestamp,
            id,
        },
    }).replace('"method":"', '"method": "');

    const res = await fetch("https://www2.deepl.com/jsonrpc", {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            "User-Agent":
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            "Accept-Language": "en-US,en;q=0.9",
            Accept: "*/*",
        },
        body,
    });
    const data = JSON.parse(res.text);
    if (data.error) throw new Error(data.error.message);
    return data.result.texts[0].text;
}

// Selo passes `zh`/`zh-Hant`/`pt`; DeepL wants its own uppercase codes (`ZH`/`PT-BR`). Other
// codes are their uppercase form. Matches pot-desktop's deepl language table.
function code(lang) {
    if (lang === "zh" || lang === "zh-Hant") return "ZH";
    if (lang === "pt") return "PT-BR";
    return lang.toUpperCase();
}
