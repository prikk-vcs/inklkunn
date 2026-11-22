//! Where in a file a refusal points: the line of a key, found in the file's block structure.
//!
//! The YAML parser gives a line for the refusals of the *text*. A refusal of the *schema* (an unknown key, a word
//! outside a list, the ordering rule) is found after the parse, from a value that has no position; this finds the line
//! of the key it is about by walking the file's indentation, so that a person told "line 9" can look at line 9. It is
//! a pointer and says nothing about what the line holds. A key the file writes inside a one-line mapping
//! (`rungs: {accept: admin}`), or one that is missing, points at the nearest enclosing key it can find; and when
//! nothing is found there is no line, never a wrong one.

/// One step into the file: a key, or the n-th (1-based) entry of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Step {
    Key(String),
    Item(usize),
}

struct Row {
    no: usize,
    /// The column the row's key starts at: the indent, plus the `- ` of a list entry.
    col: usize,
    item_start: bool,
    key: Option<String>,
}

fn rows(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - trimmed.len();
        let (item_start, rest, extra) = match trimmed.strip_prefix('-') {
            Some(after) if after.is_empty() || after.starts_with(' ') => {
                let rest = after.trim_start();
                (true, rest, 1 + (after.len() - rest.len()))
            }
            _ => (false, trimmed, 0),
        };
        let key = rest.split_once(':').and_then(|(k, after)| {
            let k = k.trim().trim_matches(['"', '\'']);
            let plain = !k.is_empty() && !k.contains([' ', '"', '\'', '{', '[', '#']);
            (plain && (after.is_empty() || after.starts_with(' '))).then(|| k.to_owned())
        });
        out.push(Row {
            no: i + 1,
            col: indent + extra,
            item_start,
            key,
        });
    }
    out
}

/// The 1-based line of the last step of `steps` that the file's structure lets us find (the line of the deepest key
/// or entry found), or `None` when not even the first is.
pub(super) fn line_of(text: &str, steps: &[Step]) -> Option<usize> {
    let all = rows(text);
    let (mut lo, mut hi) = (0usize, all.len());
    let mut found = None;
    for step in steps {
        let region = all.get(lo..hi)?;
        let Some(level) = region.iter().map(|r| r.col).min() else {
            break;
        };
        match step {
            Step::Key(k) => {
                let Some(i) = region
                    .iter()
                    .position(|r| r.col == level && r.key.as_deref() == Some(k.as_str()))
                else {
                    break;
                };
                found = region.get(i).map(|r| r.no);
                let after = region.get(i + 1..).unwrap_or(&[]);
                let end = after
                    .iter()
                    .position(|r| r.col <= level)
                    .map_or(region.len(), |p| i + 1 + p);
                (lo, hi) = (lo + i + 1, lo + end);
            }
            Step::Item(n) => {
                let starts: Vec<usize> = region
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.item_start && r.col == level)
                    .map(|(i, _)| i)
                    .collect();
                let Some(i) = n.checked_sub(1).and_then(|n| starts.get(n)).copied() else {
                    break;
                };
                found = region.get(i).map(|r| r.no);
                let end = starts.get(*n).copied().unwrap_or(region.len());
                (lo, hi) = (lo + i, lo + end);
            }
        }
    }
    found
}
