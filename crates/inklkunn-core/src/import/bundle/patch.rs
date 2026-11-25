//! A history record's `patch` (wiki-commons XFER-7): a unified diff from the previous revision's text, applied **exactly**.
//!
//! The patch is untrusted text, so it is applied strictly and refused on the first line that does not fit: every context and
//! removed line must be the line of the text it names, every hunk must have the line counts its header says, hunks must come in
//! order and not overlap, and nothing but hunks may follow the header. The answer is the new text or **a rule that was broken**
//! (a fixed sentence, never a line of the patch or of the text). The record's `sha256` is checked by the caller against the
//! result, so a patch that applies but makes something else is refused there.

/// What a hunk line does.
enum Kind {
    Context,
    Remove,
    Add,
}

/// One line of a hunk: what it does, its text, and whether the line has **no** line feed at its end (`\ No newline at end of
/// file` follows it).
struct Edit<'a> {
    kind: Kind,
    text: &'a str,
    no_final_newline: bool,
}

/// A hunk: the 0-based index of its first old line (or of the line after which it inserts) and its lines.
struct Hunk<'a> {
    old_at: usize,
    edits: Vec<Edit<'a>>,
}

/// The numbers of a hunk header `@@ -a[,b] +c[,d] @@…`: the old start and count and the new count.
fn header(line: &str) -> Option<(usize, usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let pair = |s: &str| -> Option<(usize, usize)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (old_start, old_count) = pair(old)?;
    let (_, new_count) = pair(new)?;
    Some((old_start, old_count, new_count))
}

/// Read the hunks of `patch`: the rule it broke, or each hunk with its lines.
fn hunks_of(patch: &str) -> Result<Vec<Hunk<'_>>, &'static str> {
    let mut lines = patch.split('\n').peekable();
    // The header lines (`--- a`, `+++ b`) before the first hunk.
    while let Some(line) = lines.peek() {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            lines.next();
        } else {
            break;
        }
    }
    let mut hunks: Vec<Hunk<'_>> = Vec::new();
    while let Some(line) = lines.next() {
        if line.is_empty() && lines.peek().is_none() {
            break; // the patch's own final line feed
        }
        let Some((old_start, old_count, new_count)) = header(line) else {
            return Err("has a line that is not a hunk header where one must be");
        };
        let mut edits: Vec<Edit<'_>> = Vec::new();
        let (mut seen_old, mut seen_new) = (0_usize, 0_usize);
        loop {
            // The marker that says the line before it has no final line feed may come after the hunk's last counted line.
            if let Some(next) = lines.peek()
                && next.starts_with('\\')
            {
                lines.next();
                let Some(last) = edits.last_mut() else {
                    return Err("has a no-newline marker with no line before it");
                };
                last.no_final_newline = true;
                continue;
            }
            if seen_old >= old_count && seen_new >= new_count {
                break;
            }
            let Some(body) = lines.next() else {
                return Err("ends inside a hunk");
            };
            let (mark, text) = body.split_at_checked(1).unwrap_or(("", ""));
            let kind = match mark {
                " " => {
                    seen_old += 1;
                    seen_new += 1;
                    Kind::Context
                }
                "-" => {
                    seen_old += 1;
                    Kind::Remove
                }
                "+" => {
                    seen_new += 1;
                    Kind::Add
                }
                _ => return Err("has a hunk line that is not context, a removal or an addition"),
            };
            if seen_old > old_count || seen_new > new_count {
                return Err("has a hunk with more lines than its header says");
            }
            edits.push(Edit {
                kind,
                text,
                no_final_newline: false,
            });
        }
        hunks.push(Hunk {
            // `-n,0` inserts after line n; otherwise the hunk starts at line n.
            old_at: if old_count == 0 {
                old_start
            } else {
                old_start
                    .checked_sub(1)
                    .ok_or("has a hunk that starts at line 0")?
            },
            edits,
        });
    }
    if hunks.is_empty() {
        return Err("has no hunk");
    }
    Ok(hunks)
}

/// Apply the unified diff `patch` to `old`: the new text, or the rule the patch broke.
pub fn apply(old: &str, patch: &str) -> Result<String, &'static str> {
    let hunks = hunks_of(patch)?;
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let mut out = String::with_capacity(old.len());
    let mut at = 0_usize;
    for hunk in &hunks {
        if hunk.old_at < at {
            return Err("has hunks that overlap or are out of order");
        }
        for line in old_lines
            .get(at..hunk.old_at)
            .ok_or("has a hunk beyond the end of the text it applies to")?
        {
            out.push_str(line);
        }
        let mut cursor = hunk.old_at;
        for edit in &hunk.edits {
            if matches!(edit.kind, Kind::Context | Kind::Remove) {
                let line = old_lines
                    .get(cursor)
                    .ok_or("has a line beyond the end of the text it applies to")?;
                let (body, had_newline) = match line.strip_suffix('\n') {
                    Some(body) => (body, true),
                    None => (*line, false),
                };
                if body != edit.text {
                    return Err("has a context or removed line that is not the text's line");
                }
                if had_newline == edit.no_final_newline {
                    return Err("is wrong about whether a line ends with a line feed");
                }
                cursor += 1;
            }
            if matches!(edit.kind, Kind::Context | Kind::Add) {
                out.push_str(edit.text);
                if !edit.no_final_newline {
                    out.push('\n');
                }
            }
        }
        at = cursor;
    }
    for line in old_lines.get(at..).unwrap_or_default() {
        out.push_str(line);
    }
    Ok(out)
}
