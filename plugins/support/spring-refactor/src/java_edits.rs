//! Split only edits already returned by JDT. Keep raw gaps (including comments)
//! so compaction never turns into a text search or discards a server change.
use openidea_plugin::{
    TextEdit,
    text::{offset, position},
};
use similar::{Algorithm, DiffTag};
use std::time::{Duration, Instant};

fn pieces(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut end = 0;
    for token in spring_text::java::tokens(text) {
        if end < token.at {
            out.push((end, &text[end..token.at]));
        }
        out.push((token.at, token.s));
        end = token.end();
    }
    if end < text.len() {
        out.push((end, &text[end..]));
    }
    out
}

pub fn refine_java_edit(text: &str, edit: &TextEdit) -> Result<Vec<TextEdit>, &'static str> {
    let start = offset(text, edit.range.start).ok_or("Invalid Java rename range")?;
    let end = offset(text, edit.range.end).ok_or("Invalid Java rename range")?;
    if start > end || position(text, start) != edit.range.start || position(text, end) != edit.range.end {
        return Err("Invalid Java rename range");
    }
    let before = &text[start..end];
    let a = pieces(before);
    let b = pieces(&edit.text);
    let a_tokens: Vec<_> = a.iter().map(|(_, value)| *value).collect();
    let b_tokens: Vec<_> = b.iter().map(|(_, value)| *value).collect();
    let ops = similar::capture_diff_slices_deadline(
        Algorithm::Myers,
        &a_tokens,
        &b_tokens,
        Some(Instant::now() + Duration::from_millis(100)),
    );
    let at = |tokens: &[(usize, &str)], index, length| tokens.get(index).map_or(length, |(at, _)| *at);
    let mut out = Vec::new();
    for op in ops {
        if op.tag() == DiffTag::Equal {
            continue;
        }
        let old = op.old_range();
        let new = op.new_range();
        out.push(TextEdit {
            range: spring_text::range(
                text,
                start + at(&a, old.start, before.len()),
                start + at(&a, old.end, before.len()),
            ),
            text: edit.text[at(&b, new.start, edit.text.len())..at(&b, new.end, edit.text.len())].to_owned(),
        });
    }
    // Some servers return only a changed identifier suffix. Widen that edit
    // with unchanged text so declaration checks still use whole symbol ranges.
    let word = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$');
    for refined in &mut out {
        let from = offset(text, refined.range.start).unwrap();
        let to = offset(text, refined.range.end).unwrap();
        let (mut left, mut right) = (from, to);
        while left > 0 && word(text.as_bytes()[left - 1]) {
            left -= 1;
        }
        while right < text.len() && word(text.as_bytes()[right]) {
            right += 1;
        }
        refined.text = format!("{}{}{}", &text[left..from], refined.text, &text[to..right]);
        refined.range = spring_text::range(text, left, right);
    }
    let window_start = out.first().and_then(|e| offset(text, e.range.start)).unwrap_or(start).min(start);
    let window_end = out.last().and_then(|e| offset(text, e.range.end)).unwrap_or(end).max(end);
    let mut previous = window_start;
    for refined in &out {
        let from = offset(text, refined.range.start).unwrap();
        let to = offset(text, refined.range.end).unwrap();
        if from < previous {
            return Err("Invalid Java rename range");
        }
        previous = to;
    }
    let mut expected = text[window_start..window_end].to_owned();
    expected.replace_range(start - window_start..end - window_start, &edit.text);
    let mut replay = text[window_start..window_end].to_owned();
    for refined in out.iter().rev() {
        let from = offset(text, refined.range.start).unwrap() - window_start;
        let to = offset(text, refined.range.end).unwrap() - window_start;
        replay.replace_range(from..to, &refined.text);
    }
    if replay != expected {
        return Err("Java rename edit refinement changed the result");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compacted_java_edit_retains_exact_comments_and_utf16_ranges() {
        let text = "/*😀*/ String oldName;\n// keep oldName\nString getOldName() { return oldName; }";
        let after = "/*😀*/ String newName;\n// keep oldName\nString getNewName() { return newName; }";
        let edit = TextEdit { range: spring_text::range(text, 0, text.len()), text: after.into() };
        let refined = refine_java_edit(text, &edit).unwrap();
        assert_eq!(refined.len(), 3);
        assert_eq!(refined[0].range.start.character, 14);
        assert_eq!(
            refined.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec!["newName", "getNewName", "newName"]
        );
    }

    #[test]
    fn identifier_suffix_insert_is_widened_without_changing_the_result() {
        let text = "/*😀*/ int port;";
        let at = text.find("port").unwrap();
        let edit = TextEdit { range: spring_text::range(text, at + 4, at + 4), text: "Name".into() };
        let refined = refine_java_edit(text, &edit).unwrap();
        assert_eq!(refined, vec![TextEdit { range: spring_text::range(text, at, at + 4), text: "portName".into() }]);
    }
}
