export const meta = {
    id: "local",
    name: "Local Model (OpenAI)",
    version: "1.0.0",
    timeout_ms: 120000,
    no_proxy: true,
    allow_hosts: ["127.0.0.1", "localhost"],
    config: [
        {
            key: "base_url",
            label: "Base URL",
            type: "string",
            default: "http://127.0.0.1:11434/v1",
        },
        { key: "model", label: "Model", type: "string", required: true },
        { key: "api_key", label: "API Key (usually blank for local servers)", type: "secret" },
    ],
};

export async function translate(text, from, to) {
    const base = config.base_url.replace(/\/+$/, "");
    const res = await fetch(`${base}/chat/completions`, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            ...(config.api_key ? { Authorization: `Bearer ${config.api_key}` } : {}),
        },
        body: JSON.stringify({
            model: config.model,
            stream: false,
            messages: [{ role: "user", content: `把下面的${from}翻译成${to}，只输出译文：\n${text}` }],
        }),
    });
    const data = JSON.parse(res.text);
    if (!res.ok) throw new Error(data.error?.message ?? `HTTP ${res.status}`);
    return data.choices[0].message.content;
}
