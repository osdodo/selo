use selo_core::{Rect, TextBlock};

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub rect: Rect,
    pub text: String,
    pub line_height: f32,
}

const MAX_CHARS: usize = 480;
const GAP: f32 = 0.9;
/// A body line this much wider than the paragraph so far is a heading's body: do not merge.
const HEADING_WIDEN: f32 = 1.3;

pub fn cluster(blocks: &[TextBlock]) -> Vec<Paragraph> {
    let mut sorted: Vec<&TextBlock> = blocks
        .iter()
        .filter(|b| !b.text.trim().is_empty())
        .collect();
    sorted.sort_by(|a, b| {
        a.rect
            .y
            .partial_cmp(&b.rect.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.rect
                    .x
                    .partial_cmp(&b.rect.x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut out: Vec<Paragraph> = Vec::new();
    for block in sorted {
        match out.last_mut() {
            Some(paragraph) if continues(paragraph, block) => paragraph.push(block),
            _ => out.push(Paragraph::new(block)),
        }
    }
    out
}

impl Paragraph {
    fn new(block: &TextBlock) -> Self {
        Self {
            rect: block.rect,
            text: block.text.clone(),
            line_height: block.rect.height,
        }
    }

    fn push(&mut self, block: &TextBlock) {
        self.rect = union(self.rect, block.rect);
        self.text.push(' ');
        self.text.push_str(&block.text);
    }
}

fn continues(paragraph: &Paragraph, block: &TextBlock) -> bool {
    // A new list item always starts its own paragraph; wrapped continuations still merge.
    if starts_item(&block.text) {
        return false;
    }
    let gap = block.rect.y - (paragraph.rect.y + paragraph.rect.height);
    let close_enough = gap <= GAP * paragraph.line_height.max(block.rect.height);
    let same_column = paragraph.rect.x < block.rect.x + block.rect.width
        && block.rect.x < paragraph.rect.x + paragraph.rect.width;
    let fits = paragraph.text.chars().count() + block.text.chars().count() < MAX_CHARS;
    close_enough && same_column && !heading_over_body(paragraph, block) && fits
}

fn heading_over_body(paragraph: &Paragraph, block: &TextBlock) -> bool {
    block.rect.width > paragraph.rect.width * HEADING_WIDEN
}

fn starts_item(line: &str) -> bool {
    let text = line.trim();
    let Some(first) = text.chars().next() else {
        return false;
    };
    if matches!(first, '•' | '·' | '○' | '●' | '◦' | '▪') {
        return true;
    }
    if matches!(first, '-' | '*') {
        // Require a space, so `-8.1e-7` is not read as a bullet.
        let rest = &text[first.len_utf8()..];
        return rest.is_empty() || rest.starts_with(char::is_whitespace);
    }
    if first.is_ascii_digit() {
        let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
        let mut after = rest.chars();
        return match after.next() {
            // `1. text` yes, `3.5 million` no.
            Some('.' | ')' | '、' | '．') => !after.next().is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        };
    }
    if matches!(first, '(' | '（') {
        let rest = text[first.len_utf8()..].trim_start();
        return rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || is_cjk_numeral(c));
    }
    if is_cjk_numeral(first) {
        let rest = text.trim_start_matches(is_cjk_numeral);
        return matches!(rest.chars().next(), Some('、' | '.' | '．'));
    }
    false
}

fn is_cjk_numeral(c: char) -> bool {
    matches!(
        c,
        '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
    )
}

fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: (a.y + a.height).max(b.y + b.height) - y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f32, y: f32, width: f32, height: f32, text: &str) -> TextBlock {
        TextBlock {
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            text: text.into(),
            confidence: 1.0,
        }
    }

    #[test]
    fn close_lines_in_one_column_merge() {
        let blocks = [
            line(10., 10., 200., 20., "hello"),
            line(10., 32., 220., 20., "world"),
        ];
        let paragraphs = cluster(&blocks);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text, "hello world");
        assert_eq!(paragraphs[0].rect.width, 220.);
        assert_eq!(paragraphs[0].line_height, 20.);
    }

    #[test]
    fn a_distant_line_starts_a_new_paragraph() {
        let blocks = [
            line(10., 10., 200., 20., "first"),
            line(10., 200., 200., 20., "second"),
        ];
        assert_eq!(cluster(&blocks).len(), 2);
    }

    #[test]
    fn a_heading_does_not_merge_into_the_body_below_it() {
        let blocks = [
            line(10., 10., 77., 25., "Usage"),
            line(
                10.,
                42.,
                324.,
                14.,
                "Monitor workspace requests, token usage, and costs.",
            ),
        ];
        let paragraphs = cluster(&blocks);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "Usage");
        assert_eq!(
            paragraphs[1].text,
            "Monitor workspace requests, token usage, and costs."
        );
    }

    #[test]
    fn a_heading_splits_even_when_vision_reads_the_body_nearly_as_tall() {
        let blocks = [
            line(10., 10., 121., 40., "Usage"),
            line(
                10.,
                60.,
                597.,
                35.,
                "Monitor workspace requests, token usage, and costs.",
            ),
        ];
        assert_eq!(cluster(&blocks).len(), 2);
    }

    #[test]
    fn same_font_height_noise_does_not_split_a_paragraph() {
        let blocks = [
            line(
                10.,
                10.,
                200.,
                16.,
                "Monitor workspace requests, token usage, and",
            ),
            line(10., 32., 220., 8., "costs and more."),
        ];
        assert_eq!(cluster(&blocks).len(), 1);
    }

    #[test]
    fn a_shorter_line_does_not_split_a_paragraph_when_it_is_narrower() {
        let blocks = [
            line(10., 10., 200., 20., "hello world"),
            line(10., 32., 90., 13., "axe"),
        ];
        assert_eq!(cluster(&blocks).len(), 1);
    }

    #[test]
    fn side_by_side_columns_stay_separate() {
        let blocks = [
            line(10., 10., 100., 20., "left"),
            line(300., 12., 100., 20., "right"),
        ];
        let paragraphs = cluster(&blocks);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "left");
        assert_eq!(paragraphs[1].text, "right");
    }

    #[test]
    fn each_list_item_keeps_its_own_paragraph() {
        let blocks = [
            line(10., 10., 300., 20., "Things to do:"),
            line(10., 32., 300., 20., "1. first thing"),
            line(10., 54., 300., 20., "2. second thing"),
            line(10., 76., 300., 20., "3. third thing"),
        ];
        let paragraphs = cluster(&blocks);
        let texts: Vec<&str> = paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Things to do:",
                "1. first thing",
                "2. second thing",
                "3. third thing"
            ]
        );
    }

    #[test]
    fn numbers_are_not_mistaken_for_list_markers() {
        assert!(!starts_item("3.5 million users"));
        assert!(!starts_item("-8.1000184e-7"));
        assert!(starts_item("1. a real item"));
        assert!(starts_item("- a bullet"));
        assert!(starts_item("1.无空格中文列表"));
    }

    #[test]
    fn a_wrapped_list_item_continues_but_the_next_item_does_not() {
        let blocks = [
            line(10., 10., 300., 20., "1. an item that wraps"),
            line(10., 32., 300., 20., "onto a second line"),
            line(10., 54., 300., 20., "- next bullet"),
        ];
        let paragraphs = cluster(&blocks);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(
            paragraphs[0].text,
            "1. an item that wraps onto a second line"
        );
        assert_eq!(paragraphs[1].text, "- next bullet");
    }

    #[test]
    fn a_paragraph_is_cut_before_it_hits_the_engine_limit() {
        let long = "x".repeat(150);
        let blocks: Vec<TextBlock> = (0..5)
            .map(|i| line(0., i as f32 * 22., 300., 20., &long))
            .collect();
        let paragraphs = cluster(&blocks);
        assert!(
            paragraphs.len() > 1,
            "5×150 chars must not stay one paragraph"
        );
        for paragraph in &paragraphs {
            assert!(paragraph.text.chars().count() <= MAX_CHARS);
        }
    }
}
