export const meta = {
    id: "bing",
    name: "Bing",
    version: "1.0.0",
    timeout_ms: 30000,
    allow_hosts: ["edge.microsoft.com"],
};

const UA =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Safari/537.36 Edg/113.0.1774.42";

export async function translate(text, from, to) {
    const query = `from=${encodeURIComponent(code(from))}&to=${encodeURIComponent(code(to))}&isEnterpriseClient=false`;
    const res = await fetch(`https://edge.microsoft.com/translate/translatetext?${query}`, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            "User-Agent": UA,
        },
        body: JSON.stringify([text]),
    });
    let data;
    try {
        data = JSON.parse(res.text);
    } catch {
        throw new Error(`HTTP ${res.status}`);
    }
    if (!res.ok) throw new Error(data.error?.message ?? `HTTP ${res.status}`);
    const translation = data[0]?.translations?.[0]?.text;
    if (!translation) throw new Error(JSON.stringify(data));
    return translation.trim();
}

// Selo passes `zh`/`zh-Hant`; Microsoft's endpoint wants `zh-Hans`/`zh-Hant`.
function code(lang) {
    return lang === "zh" ? "zh-Hans" : lang;
}
