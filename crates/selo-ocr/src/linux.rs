use selo_core::{Error, Image, OcrProvider, Rect, Result, TextBlock};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct LinuxOcr;

impl OcrProvider for LinuxOcr {
    fn recognize(&self, image: &Image) -> Result<Vec<TextBlock>> {
        if image.png.is_empty() {
            return Err(Error::Ocr("empty image".into()));
        }
        let path = std::env::temp_dir().join(format!(
            "selo-ocr-{}-{}.png",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, &image.png).map_err(|err| Error::Ocr(err.to_string()))?;
        let output = Command::new("tesseract")
            .arg(&path)
            .arg("stdout")
            .args(["-l", "chi_sim+eng", "--psm", "3", "tsv"])
            .output();
        let _ = std::fs::remove_file(&path);
        let output = output.map_err(|err| Error::Ocr(format!("tesseract not runnable: {err}")))?;
        if !output.status.success() {
            return Err(Error::Ocr(format!(
                "tesseract failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(parse_tsv(&String::from_utf8_lossy(&output.stdout)))
    }
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Groups word rows by `(block, paragraph, line)`; rows arrive in reading order, so a running key
/// is enough.
fn parse_tsv(tsv: &str) -> Vec<TextBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<((u32, u32, u32), Line)> = None;

    for (index, row) in tsv.lines().enumerate() {
        if index == 0 {
            continue;
        }
        let cols: Vec<&str> = row.split('\t').collect();
        // level 5 is a word; anything else is a page/block/paragraph/line aggregate.
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }
        let Ok(confidence) = cols[10].parse::<f32>() else {
            continue;
        };
        if confidence < 0.0 {
            continue;
        }
        let key = (
            cols[2].parse().unwrap_or(0),
            cols[3].parse().unwrap_or(0),
            cols[4].parse().unwrap_or(0),
        );
        let x = cols[6].parse::<f32>().unwrap_or(0.0);
        let y = cols[7].parse::<f32>().unwrap_or(0.0);
        let width = cols[8].parse::<f32>().unwrap_or(0.0);
        let height = cols[9].parse::<f32>().unwrap_or(0.0);

        if current.as_ref().is_none_or(|(line, _)| *line != key) {
            if let Some((_, line)) = current.take() {
                blocks.push(line.finish());
            }
            current = Some((key, Line::new()));
        }
        if let Some((_, line)) = current.as_mut() {
            line.push(x, y, width, height, text, confidence / 100.0);
        }
    }
    if let Some((_, line)) = current.take() {
        blocks.push(line.finish());
    }
    blocks
}

struct Line {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    words: Vec<String>,
    confidence: f32,
    count: u32,
}

impl Line {
    fn new() -> Self {
        Self {
            min_x: f32::MAX,
            min_y: f32::MAX,
            max_x: 0.0,
            max_y: 0.0,
            words: Vec::new(),
            confidence: 0.0,
            count: 0,
        }
    }

    fn push(&mut self, x: f32, y: f32, width: f32, height: f32, text: &str, confidence: f32) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x + width);
        self.max_y = self.max_y.max(y + height);
        self.words.push(text.to_string());
        self.confidence += confidence;
        self.count += 1;
    }

    fn finish(self) -> TextBlock {
        TextBlock {
            rect: Rect {
                x: self.min_x,
                y: self.min_y,
                width: self.max_x - self.min_x,
                height: self.max_y - self.min_y,
            },
            text: join_words(&self.words),
            confidence: if self.count > 0 {
                self.confidence / self.count as f32
            } else {
                0.0
            },
        }
    }
}

/// A space only between two ASCII-alphanumeric words, so Chinese text is not spaced out.
fn join_words(words: &[String]) -> String {
    let mut joined = String::new();
    for word in words {
        if !joined.is_empty() {
            if let (Some(previous), Some(next)) = (joined.chars().last(), word.chars().next())
                && previous.is_ascii_alphanumeric()
                && next.is_ascii_alphanumeric()
            {
                joined.push(' ');
            }
        }
        joined.push_str(word);
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    const TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
5\t1\t1\t1\t1\t1\t10\t20\t30\t12\t96.5\tHello\n\
5\t1\t1\t1\t1\t2\t45\t20\t40\t12\t90.0\tworld\n\
5\t1\t1\t1\t2\t1\t10\t40\t60\t12\t80.0\t你好世界\n";

    #[test]
    fn groups_words_into_one_block_per_line() {
        let blocks = parse_tsv(TSV);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Hello world");
        assert_eq!(blocks[0].rect.x, 10.0);
        assert_eq!(blocks[0].rect.width, 75.0);
        assert_eq!(blocks[1].text, "你好世界");
    }

    #[test]
    fn cjk_words_are_not_space_separated() {
        assert_eq!(join_words(&["你".into(), "好".into()]), "你好");
        assert_eq!(join_words(&["Hello".into(), "world".into()]), "Hello world");
    }
}
