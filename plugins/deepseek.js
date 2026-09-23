export const meta = {
    id: "deepseek",
    name: "DeepSeek",
    version: "1.0.0",
    timeout_ms: 30000,
    allow_hosts: ["api.deepseek.com"],
    config: [
        { key: "api_key", label: "DeepSeek API Key", type: "secret", required: true },
        { key: "model", label: "Model", type: "string", default: "deepseek-flash" },
    ],
};

export async function translate(text, from, to) {
    const res = await fetch("https://api.deepseek.com/chat/completions", {
        method: "POST",
        headers: {
            Authorization: `Bearer ${config.api_key}`,
            "Content-Type": "application/json",
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
