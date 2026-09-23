export const meta = {
    id: "ocrspace",
    name: "OCR.Space",
    version: "1.0.0",
    kind: "ocr",
    timeout_ms: 30000,
    allow_hosts: ["api.ocr.space"],
    config: [
        {
            key: "apikey",
            label: "API Key",
            type: "string",
            default: "helloworld",
        },
        {
            key: "language",
            label: "Language",
            type: "string",
            default: "auto",
            options: [
                "auto", "eng", "chs", "cht", "jpn", "kor", "fra",
                "ger", "spa", "rus", "por", "ita", "ara", "tha", "vnm",
            ],
        },
    ],
};

export async function ocr(pngBase64) {
    const body = [
        ["base64Image", `data:image/png;base64,${pngBase64}`],
        ["isOverlayRequired", "true"],
        ["language", config.language],
        ["OCREngine", "2"],
    ]
        .map(([key, value]) => `${key}=${encodeURIComponent(value)}`)
        .join("&");

    const res = await fetch("https://api.ocr.space/parse/image", {
        method: "POST",
        headers: {
            apikey: config.apikey,
            "Content-Type": "application/x-www-form-urlencoded",
        },
        body,
    });
    const data = JSON.parse(res.text);
    if (data.IsErroredOnProcessing) {
        throw new Error(data.ErrorMessage ?? data.ErrorDetails ?? `HTTP ${res.status}`);
    }
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return blocks(data);
}

// OCR.Space `ParsedResults[0].TextOverlay.Lines[]` → Selo blocks. Coordinates are already image
// physical pixels, top-left (OCR.Space reports them "in the original size of image").
export function blocks(data) {
    const page = data.ParsedResults?.[0];
    if (!page || page.FileParseExitCode !== 1) {
        throw new Error(page?.ErrorMessage ?? "OCR.Space parse failed");
    }
    return (page.TextOverlay?.Lines ?? [])
        .map((line) => {
            const words = line.Words ?? [];
            if (!words.length) return null;
            const left = Math.min(...words.map((word) => word.Left));
            const right = Math.max(...words.map((word) => word.Left + word.Width));
            return {
                x: left,
                y: line.MinTop,
                width: right - left,
                height: line.MaxHeight,
                text: line.LineText ?? words.map((word) => word.WordText).join(" "),
                confidence: 1,
            };
        })
        .filter((block) => block && block.text.trim() && block.width > 0 && block.height > 0);
}
