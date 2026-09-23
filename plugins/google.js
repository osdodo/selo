export const meta = {
    id: "google",
    name: "Google",
    version: "1.0.0",
    timeout_ms: 15000,
    allow_hosts: ["translate.google.com", "translation.googleapis.com"],
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
            label: "Google Cloud API Key (api mode)",
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

async function translateFree(text, from, to) {
    const query =
        "dt=at&dt=bd&dt=ex&dt=ld&dt=md&dt=qca&dt=rw&dt=rm&dt=ss&dt=t&client=gtx" +
        `&sl=${encodeURIComponent(code(from))}&tl=${encodeURIComponent(code(to))}&hl=${encodeURIComponent(code(to))}` +
        `&ie=UTF-8&oe=UTF-8&otf=1&ssel=0&tsel=0&kc=7&q=${encodeURIComponent(text)}`;
    const res = await fetch(`https://translate.google.com/translate_a/single?${query}`, {
        headers: { "content-type": "application/json" },
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = JSON.parse(res.text);
    return data[0]
        .map((segment) => segment[0])
        .filter(Boolean)
        .join("");
}

async function translateApi(text, from, to) {
    if (!config.api_key) throw new Error("api 模式需要填写 Google Cloud API Key");
    const url = `https://translation.googleapis.com/language/translate/v2?key=${encodeURIComponent(config.api_key)}`;
    const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ q: text, source: code(from), target: code(to), format: "text" }),
    });
    const data = JSON.parse(res.text);
    if (!res.ok) throw new Error(data.error?.message ?? `HTTP ${res.status}`);
    return data.data.translations[0].translatedText;
}

// Selo passes script-based codes (`zh`/`zh-Hant`); Google's endpoints use region-based ones.
function code(lang) {
    return lang === "zh" ? "zh-CN" : lang === "zh-Hant" ? "zh-TW" : lang;
}
