export const meta = {
    id: "youdao",
    name: "Youdao",
    version: "1.0.0",
    timeout_ms: 15000,
    allow_hosts: ["openapi.youdao.com"],
    config: [
        { key: "appkey", label: "Youdao App Key", type: "string", required: true },
        { key: "key", label: "Youdao App Secret", type: "secret", required: true },
    ],
};

export async function translate(text, from, to) {
    const curtime = String(Math.round(Date.now() / 1000));
    const salt = Math.random().toString(36).slice(2) + Date.now().toString(36);
    const sign = crypto.sha256(config.appkey + truncate(text) + salt + curtime + config.key);
    const params = [
        ["q", text],
        ["from", code(from)],
        ["to", code(to)],
        ["appKey", config.appkey],
        ["salt", salt],
        ["sign", sign],
        ["signType", "v3"],
        ["curtime", curtime],
    ]
        .map(([key, value]) => `${key}=${encodeURIComponent(value)}`)
        .join("&");

    const res = await fetch(`https://openapi.youdao.com/api?${params}`);
    const data = JSON.parse(res.text);
    if (data.errorCode && data.errorCode !== "0") throw new Error(`有道错误码 ${data.errorCode}`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const translation = data.translation?.join("\n");
    if (!translation) throw new Error(JSON.stringify(data));
    return translation.trim();
}

// The sign hashes the query truncated to `前10 + 长度 + 后10` when it is over 20 characters.
function truncate(text) {
    return text.length <= 20 ? text : text.slice(0, 10) + text.length + text.slice(-10);
}

// Selo passes script-based codes (`zh`/`zh-Hant`); Youdao uses its own set.
const LANG = {
    zh: "zh-CHS",
    "zh-Hant": "zh-CHT",
    ja: "jp",
    ko: "kor",
    fr: "fra",
    es: "spa",
    vi: "vie",
};

function code(lang) {
    return LANG[lang] ?? lang;
}
