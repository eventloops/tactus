//! Extended notes: `docs/internals/effects.md`

use std::collections::BTreeSet;

pub const CLIPPY_TOML: &str = "clippy.toml";

pub const ALLOWLIST_TOML: &str = "effects/allowlist.toml";

pub const WRAPPERS_TOML: &str = "effects/wrappers.toml";

pub const EFFECT_SITES_JSON: &str = "effect_sites.json";

pub const RESIDUE_CLASSES_JSON: &str = "effects/residue-classes.json";

pub const RESIDUE_HISTOGRAM_JSON: &str = "effects/residue-histogram.json";

pub const FUNNEL_MODULES_JSON: &str = "effects/funnel-modules.json";

pub const REGENERATE: &str = "UPSTROKE_REGENERATE_EFFECT_ARTIFACTS";

pub const GOVERNED_LINTS: &[&str] = &[
    "disallowed_methods",
    "disallowed_types",
    "disallowed_macros",
    "style",
    "all",
    "warnings",
];

pub const USED_GOVERNED_LINTS: &[&str] = &[
    "clippy::disallowed_methods",
    "clippy::disallowed_types",
    "clippy::disallowed_macros",
];

/// Renamed clippy lints that a governed one still answers to.
///
/// `PR74-GOV-001`. A rename is not a spelling variant a reader may ignore:
/// clippy still RESOLVES these, so an allow of one suppresses the governed lint
/// and a deny of one fails the build. Measured on both toolchains this
/// repository supports -- stable clippy 0.1.97 and the 1.85.0 pin's clippy
/// 0.1.85 -- with identical results: `#![allow(clippy::disallowed_method)]`
/// emits `renamed_and_removed_lints` and suppresses, and
/// `#![deny(clippy::disallowed_method)]` emits it and errors.
///
/// **Only these two, and only tool-qualified.** Measured in the same run:
/// `clippy::disallowed_macro` is `unknown_lints` and governs nothing -- there
/// is no such rename -- and the bare `disallowed_method` without the `clippy::`
/// segment is `unknown_lints` too. A list that guessed at either would report
/// an allowance where the compiler sees none.
const GOVERNED_LINT_ALIASES: &[(&str, &str)] = &[
    ("disallowed_method", "disallowed_methods"),
    ("disallowed_type", "disallowed_types"),
];

/// The bare lint name an attribute entry refers to, if it is governed.
///
/// `clippy::disallowed_methods` and `disallowed_methods` are the same lint;
/// `clippy::too_many_arguments` is not governed and answers `None`.
///
/// **And `clippy::r#disallowed_methods` is the same lint too.** `PR74-GOVERNED\
/// -LINT-TOKEN-001`: this took the last `::` segment verbatim, so a raw
/// identifier -- which rustc accepts anywhere an identifier goes, and means the
/// identifier it spells -- was a different name here and the lint went
/// unrecognised in both readers. Measured: `#![allow(clippy::r#disallowed_\
/// methods)]` suppresses the lint, and this answered `None` for it.
///
/// The segment must be a whole identifier and nothing else. `disallowed_\
/// methods_extra` is a different lint, `rr#disallowed_methods` opens no raw
/// identifier, and a segment with anything trailing it is not a lint name this
/// reader will guess at.
#[must_use]
pub fn normalize_lint(entry: &str) -> Option<&'static str> {
    // **The whole path, and only the shapes the compiler resolves.**
    //
    // Taking `rsplit("::").next()` and validating that alone left the leading
    // segments unread, so `allow(\u{00A0}clippy::disallowed_methods)` -- a file
    // rustc REFUSES, because U+00A0 is not Rust whitespace -- normalised to the
    // governed lint. `PR74-GOV-002` is the larger half of the same mistake: ANY
    // path ending in a governed name was that lint, and
    // `rustdoc::disallowed_methods`, `rustc::disallowed_methods` and
    // `clippy::extra::disallowed_methods` all compile while governing nothing.
    // The wrongly-green shape is a real allow followed by a FAKE deny --
    // measured, the file compiles CLEAN with the lint suppressed, and the
    // reader called it a denial, which is what the two censuses act on.
    //
    // So: a bare governed name, or exactly `clippy::<name>`. Two segments at
    // most and the first must be `clippy`. Anything else answers `None`, and
    // an entry that names the lint without normalising to it is a REFUSAL
    // rather than silence -- `read_attribute` is where that happens, so a
    // foreign namespace is loud instead of skipped.
    let mut segments: Vec<&str> = Vec::new();
    for segment in trim_rust(entry).split("::") {
        let segment = trim_rust(segment);
        let (name, end) = identifier_token(segment, 0)?;
        if end != segment.len() {
            return None;
        }
        segments.push(name);
    }
    let bare = match segments.as_slice() {
        [name] => *name,
        // The aliases are tool-qualified only, which is why the mapping lives
        // in this arm rather than above the match.
        ["clippy", name] => GOVERNED_LINT_ALIASES
            .iter()
            .find(|(alias, _)| alias == name)
            .map_or(*name, |(_, canonical)| *canonical),
        _ => return None,
    };
    GOVERNED_LINTS.iter().copied().find(|name| *name == bare)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernedAllow {
    pub line: usize,
    pub inner: bool,
    pub module_level: bool,
    pub lints: Vec<String>,
    pub written: Vec<String>,
    pub keywords: Vec<&'static str>,
    pub reasoned: bool,
}

#[must_use]
pub fn blank_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let mut depth = 1usize;
                i += 2;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                    } else {
                        if bytes[i] == b'\n' {
                            out.push(b'\n');
                        }
                        i += 1;
                    }
                }
            }
            b'r' | b'b' if i == 0 || !is_ident_byte(bytes[i - 1]) => match literal_end(bytes, i) {
                Some(end) => {
                    out.extend_from_slice(&bytes[i..end]);
                    i = end;
                }
                None => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            b'"' => {
                let end = literal_end(bytes, i).unwrap_or(bytes.len());
                out.extend_from_slice(&bytes[i..end]);
                i = end;
            }
            b'\'' => match char_literal_end(bytes, i) {
                Some(end) => {
                    out.extend_from_slice(&bytes[i..end]);
                    i = end;
                }
                None => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn char_literal_end(bytes: &[u8], from: usize) -> Option<usize> {
    if bytes.get(from) != Some(&b'\'') {
        return None;
    }
    let mut at = from + 1;
    if bytes.get(at) == Some(&b'\\') {
        at += 2;
        let limit = (from + 13).min(bytes.len());
        while at < limit && bytes[at] != b'\'' {
            at += 1;
        }
        return (bytes.get(at) == Some(&b'\'')).then_some(at + 1);
    }
    let width = match *bytes.get(at)? {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    at += width;
    (bytes.get(at) == Some(&b'\'')).then_some(at + 1)
}

fn literal_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut j = from;
    if bytes.get(j) == Some(&b'b') {
        j += 1;
    }
    let raw = bytes.get(j) == Some(&b'r');
    if raw {
        j += 1;
    }
    let hash_start = j;
    while bytes.get(j) == Some(&b'#') {
        j += 1;
    }
    let hashes = j - hash_start;
    if bytes.get(j) != Some(&b'"') || (!raw && hashes > 0) {
        return None;
    }
    j += 1;
    if raw {
        let close: Vec<u8> = std::iter::once(b'"')
            .chain(std::iter::repeat_n(b'#', hashes))
            .collect();
        while j < bytes.len() && !bytes[j..].starts_with(&close) {
            j += 1;
        }
        return Some((j + close.len()).min(bytes.len()));
    }
    while j < bytes.len() && bytes[j] != b'"' {
        j += if bytes[j] == b'\\' { 2 } else { 1 };
    }
    Some((j + 1).min(bytes.len()))
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn blank_comments_and_strings(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = vec![b' '; bytes.len()];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            out[index] = b'\n';
        }
    }
    let keep = |out: &mut Vec<u8>, from: usize, to: usize| {
        out[from..to].copy_from_slice(&bytes[from..to]);
    };

    let mut i = 0;
    let mut code_start = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                keep(&mut out, code_start, i);
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                code_start = i;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                keep(&mut out, code_start, i);
                let mut depth = 1;
                i += 2;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                code_start = i;
            }
            b'r' | b'b' => {
                let mut j = i;
                if bytes[j] == b'b' {
                    j += 1;
                }
                let raw = j < bytes.len() && bytes[j] == b'r';
                if raw {
                    j += 1;
                }
                let hash_start = j;
                while j < bytes.len() && bytes[j] == b'#' {
                    j += 1;
                }
                let hashes = j - hash_start;
                if j < bytes.len() && bytes[j] == b'"' && (raw || hashes == 0) {
                    keep(&mut out, code_start, i);
                    j += 1;
                    if raw {
                        let close: Vec<u8> = std::iter::once(b'"')
                            .chain(std::iter::repeat_n(b'#', hashes))
                            .collect();
                        while j < bytes.len() && !bytes[j..].starts_with(&close) {
                            j += 1;
                        }
                        j = (j + close.len()).min(bytes.len());
                    } else {
                        while j < bytes.len() && bytes[j] != b'"' {
                            j += if bytes[j] == b'\\' { 2 } else { 1 };
                        }
                        j = (j + 1).min(bytes.len());
                    }
                    i = j;
                    code_start = i;
                } else {
                    i += 1;
                }
            }
            b'"' => {
                keep(&mut out, code_start, i);
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                code_start = i;
            }
            b'\'' => match char_literal_end(bytes, i) {
                Some(end) => {
                    keep(&mut out, code_start, i);
                    i = end;
                    code_start = i;
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }
    keep(&mut out, code_start, bytes.len());
    String::from_utf8_lossy(&out).into_owned()
}

#[must_use]
pub fn production_region(source: &str) -> String {
    let blanked = blank_comments_and_strings(source);
    match blanked.find("#[cfg(test)]") {
        Some(cut) => source[..cut].to_owned(),
        None => source.to_owned(),
    }
}

#[must_use]
pub fn production_code(source: &str) -> String {
    const ATTR: &[u8] = b"#[cfg(test)]";
    let blanked = blank_comments_and_strings(source);
    let bytes = blanked.as_bytes();
    let mut out = bytes.to_vec();
    let mut from = 0;
    while let Some(at) = bytes
        .get(from..)
        .and_then(|rest| rest.windows(ATTR.len()).position(|at| at == ATTR))
        .map(|found| from + found)
    {
        let mut start = at + ATTR.len();
        loop {
            while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
                start += 1;
            }
            if bytes.get(start) == Some(&b'#') {
                let open = if bytes.get(start + 1) == Some(&b'!') {
                    start + 2
                } else {
                    start + 1
                };
                if bytes.get(open) == Some(&b'[') {
                    if let Some(close) = matching(bytes, open, b'[', b']') {
                        start = close + 1;
                        continue;
                    }
                }
            }
            break;
        }
        let end = configured_item_end(bytes, start);
        for byte in &mut out[at..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
        from = end.max(at + ATTR.len());
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn configured_item_end(bytes: &[u8], start: usize) -> usize {
    let return_start = configured_function_return_start(bytes, start);
    let mut depth = 0usize;
    let mut index = start;
    while let Some(&byte) = bytes.get(index) {
        if depth == 0
            && return_start.is_some_and(|at| index >= at)
            && starts_named_function_item(bytes, index)
        {
            return start;
        }
        match byte {
            b'{' if depth == 0 => {
                let Some(close) = matching(bytes, index, b'{', b'}') else {
                    return start;
                };
                let mut after = close + 1;
                while bytes.get(after).is_some_and(u8::is_ascii_whitespace) {
                    after += 1;
                }
                return if bytes.get(after) == Some(&b';') {
                    after + 1
                } else {
                    close + 1
                };
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => return index,
            b')' | b']' | b'}' => depth -= 1,
            b';' if depth == 0 => return index + 1,
            b',' if depth == 0 && return_start.is_none_or(|at| index < at) => return index + 1,
            _ => {}
        }
        index += 1;
    }
    start
}

fn starts_named_function_item(bytes: &[u8], at: usize) -> bool {
    if at
        .checked_sub(1)
        .and_then(|before| bytes.get(before))
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'#'))
    {
        return false;
    }
    let Some(rest) = bytes.get(at..).and_then(|rest| rest.strip_prefix(b"fn")) else {
        return false;
    };
    rest.first().is_some_and(u8::is_ascii_whitespace)
        && rest
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
}

fn configured_function_return_start(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    loop {
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let rest = bytes.get(cursor..)?;
        let length = rest
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
            .count();
        let word = rest.get(..length)?;
        cursor += length;
        match word {
            b"fn" => break,
            b"pub" => {
                while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                    cursor += 1;
                }
                if bytes.get(cursor) == Some(&b'(') {
                    let close = matching(bytes, cursor, b'(', b')')?;
                    cursor = close + 1;
                }
            }
            b"async" | b"const" | b"unsafe" | b"extern" => {}
            _ => return None,
        }
    }
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    let name_start = cursor;
    while bytes
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        cursor += 1;
    }
    if cursor == name_start {
        return None;
    }
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'(') {
        return None;
    }
    let close = matching(bytes, cursor, b'(', b')')?;
    cursor = close + 1;
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    bytes
        .get(cursor..)
        .is_some_and(|rest| rest.starts_with(b"->"))
        .then_some(cursor + 2)
}

#[must_use]
pub fn governed_allows(source: &str) -> Vec<GovernedAllow> {
    let blanked = blank_comments_and_strings(source);
    let bytes = blanked.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let Some(opener) = attribute_opener(&blanked, i) else {
            i += 1;
            continue;
        };
        let Some(close) = matching(bytes, opener.open, b'[', b']') else {
            i += 1;
            continue;
        };
        let attribute = &blanked[opener.open + 1..close];
        let mut lints = Vec::new();
        let mut written = Vec::new();
        let mut keywords: Vec<&'static str> = Vec::new();
        let mut reasoned = false;
        for keyword in ["allow", "expect"] {
            for group in calls_named(attribute, keyword) {
                let before = lints.len();
                for entry in group.split(',') {
                    let entry = trim_rust(entry);
                    if entry.is_empty() {
                        continue;
                    }
                    if entry.starts_with("reason") {
                        reasoned = true;
                        continue;
                    }
                    written.push(entry.to_owned());
                    if let Some(name) = normalize_lint(entry) {
                        lints.push(name.to_owned());
                    }
                }
                if lints.len() > before && !keywords.contains(&keyword) {
                    keywords.push(keyword);
                }
            }
        }
        if !lints.is_empty() {
            found.push(GovernedAllow {
                line: blanked[..i].matches('\n').count() + 1,
                inner: opener.inner,
                module_level: is_module_level(&blanked, i, close, opener.inner),
                lints,
                written,
                keywords,
                reasoned,
            });
        }
        i = close + 1;
    }
    found
}

/// The contents of every `keyword(…)` call in `text`, left to right.
///
/// `PR74-GOVERNED-LINT-TOKEN-001`. This used to be a substring search that
/// required the **very next byte** after the keyword to be `(`. Rust requires
/// no such thing: `allow (…)`, `allow /* why */ (…)` and the same across a line
/// break are ordinary attributes, and every one of them was invisible to
/// [`governed_allows`] -- so a file could allow a governed lint with no
/// `effects/allowlist.toml` row and no census would say so.
///
/// So the scan walks **tokens**. An identifier is read whole, which is also
/// what makes `allowance(…)` a call named `allowance` rather than a hit on
/// `allow` -- the old code needed a hand-written word-boundary test for that,
/// and this needs none. A raw identifier is the identifier it spells, so
/// `r#allow(…)` is found here exactly as `allow(…)` is.
///
/// **Fail closed.** A `keyword(` whose parenthesis never closes cannot be
/// resolved, and the rest of the attribute is taken as its contents rather than
/// dropped: an allowance nobody can parse is not an allowance nobody has. Such
/// a file does not compile, so the arm is unreachable from a green tree -- it
/// exists so that the unreadable case is loud instead of absent.
fn calls_named<'a>(text: &'a str, keyword: &str) -> Vec<&'a str> {
    let bytes = text.as_bytes();
    let mut groups = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let Some((name, after)) = identifier_token(text, at) else {
            at += 1;
            continue;
        };
        at = after;
        if name != keyword {
            continue;
        }
        let open = skip_blank(text, after);
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let close = matching(bytes, open, b'(', b')').unwrap_or(bytes.len());
        groups.push(&text[open + 1..close]);
        at = close.saturating_add(1).max(open + 1);
    }
    groups
}

/// The identifier token at `at`, normalised, and the index just past it.
///
/// A raw identifier is the identifier it spells -- `r#allow` is `allow` and
/// `r#disallowed_methods` is `disallowed_methods`, measured against
/// `clippy-driver` rather than assumed -- so the `r#` is consumed here instead
/// of being left for a comparison downstream to trip over.
///
/// `None` when `at` does not open an identifier at all, `r#` included: `r#`
/// followed by nothing an identifier may start with is not a token, and a
/// caller that cannot tokenise an attribute treats it as unreadable rather than
/// as saying nothing. ASCII, like every other scan in this file; a lint name is
/// ASCII and a non-ASCII head answers `None`, which is the closed direction.
fn identifier_token(text: &str, at: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    let start = if bytes.get(at) == Some(&b'r') && bytes.get(at + 1) == Some(&b'#') {
        at + 2
    } else {
        at
    };
    if !bytes
        .get(start)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        return None;
    }
    let mut end = start + 1;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    Some((&text[start..end], end))
}

/// Whether `character` is whitespace **to the Rust lexer**.
///
/// `PR74-GOVERNED-LINT-TOKEN-003`. This is `Pattern_White_Space`, the set
/// rustc's own lexer uses, and it is written out because it is NOT
/// `char::is_whitespace` -- the two disagree in both directions, and each
/// disagreement is a defect in one of the two possible flavours. Measured
/// against `clippy-driver`, and the table lives in
/// `effects::tests::the_governed_lint_readers_separate_tokens_the_way_the_\
/// lexer_does`:
///
/// * `U+200E` and `U+200F` are Rust whitespace and are **not** `White_Space`.
///   `char::is_whitespace` refuses a separator the compiler accepts, so a
///   prologue that really does allow the lint reads as unreadable -- and
///   `U+200E` is the one that survives `rustfmt` under a `rustfmt::skip`.
/// * `U+00A0`, `U+1680`, `U+2000`, `U+2003`, `U+202F`, `U+205F`, `U+3000` are
///   `White_Space` and are **not** Rust whitespace. `char::is_whitespace` steps
///   over a character rustc refuses to compile, and the reader reports a level
///   for a file that has none. That is the wrongly-green direction.
const fn is_rust_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{0085}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// The first index at or after `at` that is not Rust whitespace.
///
/// Advances by **characters**, because a separator need not be one byte:
/// stepping a byte at a time would land inside `U+200E` and slice a panic out
/// of the next `&text[..]`. Comments are already spaces by the time either
/// reader runs, so between them this is the whole of what may separate two
/// tokens -- an attribute's name from its delimiter, or the three tokens of an
/// introducer.
///
/// An index that is past the end or inside a character is returned unchanged:
/// a caller that has lost the boundary gets no progress rather than a panic.
fn skip_blank(text: &str, mut at: usize) -> usize {
    loop {
        if at >= text.len() || !text.is_char_boundary(at) {
            return at;
        }
        let Some(character) = text[at..].chars().next() else {
            return at;
        };
        if !is_rust_whitespace(character) {
            return at;
        }
        at += character.len_utf8();
    }
}

/// `text` without the Rust whitespace at either end.
///
/// `str::trim` is `char::is_whitespace`, which is the wrong set here for both
/// of the reasons [`is_rust_whitespace`] gives.
fn trim_rust(text: &str) -> &str {
    let start = skip_blank(text, 0);
    let mut end = text.len();
    while end > start {
        let Some(character) = text[start..end].chars().next_back() else {
            break;
        };
        if !is_rust_whitespace(character) {
            break;
        }
        end -= character.len_utf8();
    }
    &text[start..end]
}

/// One attribute introducer: the `#`, an optional `!`, and the `[`.
struct AttributeOpener {
    /// `#![…]` rather than `#[…]`.
    inner: bool,
    /// The index of the opening bracket itself.
    open: usize,
}

/// The attribute introducer beginning at `at`, if one does.
///
/// `PR74-GOVERNED-LINT-TOKEN-002`. `#`, `!` and `[` are three TOKENS and Rust
/// puts no adjacency requirement on them: `# ! [allow(…)]`,
/// `#/* why */![allow(…)]` and the same across line breaks are ordinary
/// attributes, each measured and each suppressing the lint. Both readers
/// required the bytes to be adjacent, so a denial taken back through a
/// separated introducer read as a denial, and an allowance written that way was
/// invisible to the placement census.
///
/// `at` is the `#` and stays the `#`: the returned `open` is the bracket's own
/// index, so a caller's line number, byte offsets and placement decision are
/// computed from exactly the positions they always were.
///
/// **Fail closed.** `None` whenever what follows is not an introducer, which is
/// also what keeps `r#allow` from being read as one -- the `#` there is
/// followed by an identifier rather than by `!` or `[`.
fn attribute_opener(text: &str, at: usize) -> Option<AttributeOpener> {
    let bytes = text.as_bytes();
    if bytes.get(at) != Some(&b'#') {
        return None;
    }
    let mut cursor = skip_blank(text, at + 1);
    let inner = bytes.get(cursor) == Some(&b'!');
    if inner {
        cursor = skip_blank(text, cursor + 1);
    }
    if bytes.get(cursor) != Some(&b'[') {
        return None;
    }
    Some(AttributeOpener {
        inner,
        open: cursor,
    })
}

/// The index of the bracket closing the one at `open`, or `None`.
fn matching(bytes: &[u8], open: usize, opener: u8, closer: u8) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        if *byte == opener {
            depth += 1;
        } else if *byte == closer {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn is_module_level(blanked: &str, hash: usize, close: usize, inner: bool) -> bool {
    if inner {
        // Nothing but whitespace and complete attributes may precede it.
        //
        // Walked FORWARDS from the first byte. The backward walk this replaced
        // searched for a literal `#![` or `#[`, which is the adjacency
        // assumption `PR74-GOVERNED-LINT-TOKEN-002` is about, and it trimmed
        // with `str::trim_end`, whose whitespace is not the lexer's. Forwards,
        // the same [`attribute_opener`] both readers use decides what an
        // attribute is, so all three agree by construction.
        let mut at = 0;
        loop {
            at = skip_blank(blanked, at);
            if at >= hash {
                return at == hash;
            }
            let Some(opener) = attribute_opener(blanked, at) else {
                return false;
            };
            let Some(end) = matching(blanked.as_bytes(), opener.open, b'[', b']') else {
                return false;
            };
            at = end + 1;
        }
    }
    // Outer: skip further attributes and whitespace, then require `mod`.
    let mut at = close + 1;
    loop {
        at = skip_blank(blanked, at);
        let rest = &blanked[at.min(blanked.len())..];
        if rest.starts_with('#') {
            let Some(opener) = attribute_opener(blanked, at) else {
                return false;
            };
            let Some(end) = matching(blanked.as_bytes(), opener.open, b'[', b']') else {
                return false;
            };
            at = end + 1;
            continue;
        }
        for visibility in ["pub(crate)", "pub(super)", "pub", ""] {
            let candidate = trim_rust(rest.strip_prefix(visibility).unwrap_or(rest));
            if candidate.starts_with("mod ") {
                return true;
            }
            if visibility.is_empty() {
                return false;
            }
        }
        return false;
    }
}

pub const FROZEN_LEGACY_ALLOWLIST: &[&str] = &[
    "src/engine/coordinator.rs",
    "src/engine/resume.rs",
    "src/engine/attempt.rs",
    "src/engine/preflight.rs",
    "src/workspace.rs",
    "src/gates.rs",
    "src/review.rs",
    "src/agent/claude.rs",
    "src/agent/codex.rs",
    "src/agent/copilot.rs",
    "src/agent/bin.rs",
    "src/capacity.rs",
    "src/export.rs",
    "src/main.rs",
    "src/answer.rs",
    "src/config.rs",
    "src/connect.rs",
    "src/route.rs",
    "src/status.rs",
    "src/validate.rs",
    "src/events/mod.rs",
    "src/events/log/premove.rs",
    "src/engine/tests.rs",
    "examples/probe.rs",
];

pub const TOPOLOGY_MODULES: &[&str] = &[
    "src/topology/",
    "src/runner/",
    "src/workspace_manager.rs",
    "src/workspace_manager/",
    "src/engine/topology.rs",
    "src/engine/topology/",
];

#[must_use]
pub fn legacy_growth<'a>(frozen: &[&str], current: &[&'a str]) -> Vec<&'a str> {
    let frozen: BTreeSet<&str> = frozen.iter().copied().collect();
    current
        .iter()
        .copied()
        .filter(|path| !frozen.contains(path))
        .collect()
}

#[must_use]
pub fn topology_modules_among<'a>(paths: &[&'a str]) -> Vec<&'a str> {
    paths
        .iter()
        .copied()
        .filter(|path| {
            TOPOLOGY_MODULES
                .iter()
                .any(|banned| path.starts_with(banned) || *path == *banned)
        })
        .collect()
}

pub const CLASSIFIED_MODULES: &[&str] = &[
    "src/workspace_manager.rs",
    "src/workspace_manager/containment.rs",
    "src/workspace_manager/hooks.rs",
    "src/workspace_manager/naming.rs",
    "src/workspace_manager/object.rs",
    "src/workspace_manager/parsers.rs",
    "src/workspace_manager/residue.rs",
    "src/workspace_manager/snapshot_ref.rs",
    "src/workspace_manager/worktree.rs",
    "src/rundir.rs",
    "src/rundir/classify.rs",
    "src/rundir/discovery.rs",
    "src/rundir/names.rs",
    "src/rundir/ownership.rs",
    "src/rundir/retention.rs",
    "src/interaction.rs",
    "src/util.rs",
    "src/events/log.rs",
    "src/runner/host.rs",
    "src/runner/host/environment.rs",
    "src/runner/host/naming.rs",
    "src/runner/host/probe.rs",
    "src/runner/invocation.rs",
    "src/runner/container.rs",
    "src/runner/container/view.rs",
    "src/engine/coordinator.rs",
    "src/engine/resume.rs",
    "src/engine/attempt.rs",
    "src/engine/preflight.rs",
    "src/workspace.rs",
    "src/gates.rs",
    "src/review.rs",
    "src/agent/proc.rs",
    "src/agent/proc/ambient.rs",
    "src/agent/proc/drain.rs",
    "src/agent/proc/input.rs",
    "src/agent/proc/pipe_io.rs",
    "src/agent/proc/worker.rs",
    "src/agent/proc/hooks.rs",
    "src/agent/bin.rs",
    "src/agent/claude.rs",
    "src/agent/codex.rs",
    "src/agent/copilot.rs",
    "src/capacity.rs",
    "src/export.rs",
    "src/main.rs",
    "src/answer.rs",
    "src/config.rs",
    "src/connect.rs",
    "src/route.rs",
    "src/status.rs",
    "src/validate.rs",
    "src/events/mod.rs",
    "src/events/log/premove.rs",
];

#[must_use]
pub fn externally_reachable_fns(source: &str) -> Vec<String> {
    let region = blank_comments_and_strings(&production_region(source));
    let bytes = region.as_bytes();
    let mut names = BTreeSet::new();
    let mut trait_impl_spans = Vec::new();
    let mut public_trait_spans = Vec::new();

    let mut t = 0;
    while let Some(hit) = region[t..].find("trait ") {
        let start = t + hit;
        t = start + "trait ".len();
        if start > 0 && is_ident_byte(bytes[start - 1]) {
            continue;
        }
        if !declares_visibility(&region[..start]) {
            continue;
        }
        let Some(brace) = find_header_brace(&region, t) else {
            continue;
        };
        if let Some(end) = matching(bytes, brace, b'{', b'}') {
            public_trait_spans.push((brace, end));
        }
    }

    let mut i = 0;
    while let Some(hit) = region[i..].find("impl") {
        let start = i + hit;
        i = start + 4;
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        if !before_ok || !region[i..].starts_with([' ', '<']) {
            continue;
        }
        let Some(brace) = find_header_brace(&region, i) else {
            continue;
        };
        let header = &region[i..brace];
        if !header.contains(" for ") {
            continue;
        }
        if let Some(end) = matching(bytes, brace, b'{', b'}') {
            trait_impl_spans.push((brace, end));
        }
    }

    for (index, _) in region.match_indices("fn ") {
        if index > 0 && is_ident_byte(bytes[index - 1]) {
            continue;
        }
        let Some(name) = region[index + 3..]
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let visible = declares_visibility(&region[..index]);
        let in_trait_impl = trait_impl_spans
            .iter()
            .any(|(open, close)| index > *open && index < *close);
        let is_default_body = public_trait_spans
            .iter()
            .any(|(open, close)| index > *open && index < *close)
            && find_header_brace(&region, index).is_some();
        if visible || in_trait_impl || is_default_body {
            names.insert(name.to_owned());
        }
    }
    names.into_iter().collect()
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn declares_visibility(prefix: &str) -> bool {
    let mut rest = prefix.trim_end();
    for modifier in ["unsafe", "const", "async"] {
        for _ in 0..3 {
            rest = rest.strip_suffix(modifier).unwrap_or(rest).trim_end();
        }
    }
    rest.ends_with("pub") || rest.ends_with("pub(crate)") || rest.ends_with("pub(super)")
}

fn find_header_brace(region: &str, from: usize) -> Option<usize> {
    let bytes = region.as_bytes();
    let mut angle = 0i32;
    let mut paren = 0i32;
    for (index, byte) in bytes.iter().enumerate().skip(from) {
        match byte {
            b'<' => angle += 1,
            b'>' => angle -= 1,
            b'(' => paren += 1,
            b')' => paren -= 1,
            b';' if angle <= 0 && paren <= 0 => return None,
            b'{' if angle <= 0 && paren <= 0 => return Some(index),
            _ => {}
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DenialFixture {
    pub shape: &'static str,
    pub source: &'static str,
    pub lint: &'static str,
    pub resolves_to: &'static str,
}

pub const DENIAL_FIXTURES: &[DenialFixture] = &[
    DenialFixture {
        shape: "renamed-import",
        source: "use std::fs::write as scribble;\n\
                 pub fn go(p: &str) { let _ = scribble(p, \"x\"); }\n",
        lint: "clippy::disallowed_methods",
        resolves_to: "std::fs::write",
    },
    DenialFixture {
        shape: "re-export",
        source: "pub mod hatch { pub use std::fs::write; }\n\
                 pub fn go(p: &str) { let _ = hatch::write(p, \"x\"); }\n",
        lint: "clippy::disallowed_methods",
        resolves_to: "std::fs::write",
    },
    DenialFixture {
        shape: "function-value",
        source: "pub fn go(p: &str) {\n\
                 \x20   let f = std::fs::write::<&str, &str>;\n\
                 \x20   let _ = f(p, \"x\");\n\
                 }\n",
        lint: "clippy::disallowed_methods",
        resolves_to: "std::fs::write",
    },
    DenialFixture {
        shape: "legacy-wrapper call",
        source: "pub fn go(p: &std::path::Path) {\n\
                 \x20   let _ = upstroke::util::write_text(p, \"x\");\n\
                 }\n",
        lint: "clippy::disallowed_methods",
        resolves_to: "upstroke::util::write_text",
    },
    DenialFixture {
        shape: "method call",
        source: "pub fn go(p: &std::path::Path) -> std::io::Result<()> {\n\
                 \x20   let f = std::fs::File::open(p)?;\n\
                 \x20   f.sync_all()\n\
                 }\n",
        lint: "clippy::disallowed_methods",
        resolves_to: "std::fs::File::sync_all",
    },
    DenialFixture {
        shape: "macro-expanded code",
        source: "pub fn go() { println!(\"escaped\"); }\n",
        lint: "clippy::disallowed_macros",
        resolves_to: "std::println",
    },
    DenialFixture {
        shape: "type",
        source: "pub fn go() { let _ = std::process::Command::new(\"git\"); }\n",
        lint: "clippy::disallowed_types",
        resolves_to: "std::process::Command",
    },
];

pub const DENIAL_CONTROL: &str = "pub fn go(p: &std::path::Path) -> bool {\n\
                                  \x20   let _ = upstroke::util::tail(\"x\", 1);\n\
                                  \x20   p.exists()\n\
                                  }\n";

#[cfg(test)]
pub(crate) mod census_domain {
    use std::path::PathBuf;

    pub(crate) fn production_calls(code: &str, name: &str, form: Call) -> usize {
        let needle = format!("{name}(");
        code.match_indices(&needle)
            .filter(|(at, _)| {
                code[..*at]
                    .chars()
                    .next_back()
                    .is_none_or(|before| !before.is_alphanumeric() && before != '_')
            })
            .filter(|(at, _)| !code[..*at].trim_end().ends_with("fn"))
            .filter(|(at, _)| {
                let dotted = code[..*at].trim_end().ends_with('.');
                match form {
                    Call::Free => !dotted,
                    Call::Method => dotted,
                }
            })
            .count()
    }

    #[derive(Clone, Copy)]
    pub(crate) enum Call {
        Free,
        Method,
    }

    pub(crate) fn whole_file_test_modules(
        source_root: &std::path::Path,
        files: &[PathBuf],
        floor: usize,
    ) -> std::collections::BTreeSet<PathBuf> {
        let declarations = declared_whole_file_test_modules(source_root, files);
        assert!(
            declarations.len() >= floor,
            "only {} test-only `mod …;` declarations were derived and the floor is {floor}; \
             the derivation is finding nothing",
            declarations.len()
        );
        let mut modules = std::collections::BTreeSet::new();
        let mut edges: Vec<(PathBuf, PathBuf)> = Vec::new();
        for declaration in &declarations {
            let resolved = sole_present(&declaration.candidates, &|path| path.is_file())
                .unwrap_or_else(|present| {
                    panic!(
                        "`{}` declares `mod {};` under {} and {present} of {:?} exist. A skip \
                         path naming no file is a skip that has stopped meaning anything",
                        declaration.declared_in.display(),
                        declaration.name,
                        declaration.render_guard(),
                        declaration.candidates
                    )
                })
                .clone();
            assert!(
                modules.insert(resolved.clone()),
                "two declarations resolve to `{}`; one of them is deriving a skip for a file it \
                 does not declare",
                resolved.display()
            );
            edges.push((declaration.declared_in.clone(), resolved));
        }
        assert!(
            declaration_cycle(&edges).is_none(),
            "the module declarations are cyclic, so no file's guard can be trusted: {:?}",
            declaration_cycle(&edges)
        );
        assert!(
            modules
                .iter()
                .any(|path| path.file_stem().is_none_or(|stem| stem != "tests")),
            "every module derived here is called `tests.rs`, which is exactly what the file-name \
             rule this replaces also finds. The derivation has degraded to the rule it exists to \
             be better than: {modules:?}"
        );
        modules
    }

    pub(crate) fn declaration_cycle(edges: &[(PathBuf, PathBuf)]) -> Option<Vec<PathBuf>> {
        use std::collections::{BTreeMap, BTreeSet};

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Colour {
            White,
            Grey,
            Black,
        }

        let mut adjacency: BTreeMap<&PathBuf, Vec<&PathBuf>> = BTreeMap::new();
        let mut nodes: BTreeSet<&PathBuf> = BTreeSet::new();
        for (from, to) in edges {
            adjacency.entry(from).or_default().push(to);
            nodes.insert(from);
            nodes.insert(to);
        }

        let mut colour: BTreeMap<&PathBuf, Colour> =
            nodes.iter().map(|node| (*node, Colour::White)).collect();
        for start in &nodes {
            if colour.get(start) != Some(&Colour::White) {
                continue;
            }
            colour.insert(start, Colour::Grey);
            let mut stack: Vec<(&PathBuf, usize)> = vec![(start, 0)];
            while let Some((node, taken)) = stack.pop() {
                let outgoing: &[&PathBuf] = adjacency
                    .get(node)
                    .map_or(&[][..], |edges| edges.as_slice());
                let Some(next) = outgoing.get(taken).copied() else {
                    colour.insert(node, Colour::Black);
                    continue;
                };
                stack.push((node, taken + 1));
                match colour.get(next) {
                    Some(Colour::Grey) => {
                        let path: Vec<&PathBuf> = stack.iter().map(|(at, _)| *at).collect();
                        let from = path.iter().position(|at| *at == next).unwrap_or(0);
                        let mut cycle: Vec<PathBuf> =
                            path[from..].iter().map(|at| (*at).clone()).collect();
                        cycle.push(next.clone());
                        return Some(cycle);
                    }
                    Some(Colour::Black) => {}
                    _ => {
                        colour.insert(next, Colour::Grey);
                        stack.push((next, 0));
                    }
                }
            }
        }
        None
    }

    pub(crate) fn sole_present<'a>(
        candidates: &'a [PathBuf; 2],
        exists: &dyn Fn(&std::path::Path) -> bool,
    ) -> Result<&'a PathBuf, usize> {
        let present: Vec<&PathBuf> = candidates
            .iter()
            .filter(|candidate| exists(candidate))
            .collect();
        match present.as_slice() {
            [only] => Ok(only),
            other => Err(other.len()),
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum CandidateRefusal {
        OutsideThePackage {
            declared_in: PathBuf,
            package_dir: PathBuf,
        },
    }

    impl std::fmt::Display for CandidateRefusal {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::OutsideThePackage {
                    declared_in,
                    package_dir,
                } => write!(
                    f,
                    "`{}` is not inside `{}`, so the target inventory read for that package \
                     does not say whether it is a crate root",
                    declared_in.display(),
                    package_dir.display()
                ),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum InventoryRefusal {
        NotRun {
            manifest: PathBuf,
            why: String,
        },
        Failed {
            manifest: PathBuf,
            status: String,
            stderr: String,
        },
        Unreadable {
            manifest: PathBuf,
            why: String,
        },
        NoPackage {
            manifest: PathBuf,
        },
        NoTargets {
            manifest: PathBuf,
        },
    }

    impl std::fmt::Display for InventoryRefusal {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::NotRun { manifest, why } => write!(
                    f,
                    "`cargo metadata` for `{}` could not be started ({why}), so the crate roots \
                     are unknown and this census will not guess them from file names",
                    manifest.display()
                ),
                Self::Failed {
                    manifest,
                    status,
                    stderr,
                } => write!(
                    f,
                    "`cargo metadata` for `{}` exited {status}: {stderr}",
                    manifest.display()
                ),
                Self::Unreadable { manifest, why } => write!(
                    f,
                    "`cargo metadata` for `{}` did not answer with the document this reads \
                     ({why})",
                    manifest.display()
                ),
                Self::NoPackage { manifest } => write!(
                    f,
                    "`cargo metadata` named no package whose manifest is `{}`",
                    manifest.display()
                ),
                Self::NoTargets { manifest } => write!(
                    f,
                    "the package at `{}` declares no target, so it has no crate root",
                    manifest.display()
                ),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct CrateRoots {
        package_dir: PathBuf,
        roots: std::collections::BTreeSet<PathBuf>,
    }

    impl CrateRoots {
        pub(crate) fn from_metadata_json(
            json: &str,
            manifest: &std::path::Path,
        ) -> Result<Self, InventoryRefusal> {
            let refuse = |why: &str| InventoryRefusal::Unreadable {
                manifest: manifest.to_path_buf(),
                why: why.to_owned(),
            };
            let document: serde_json::Value =
                serde_json::from_str(json).map_err(|error| refuse(&error.to_string()))?;
            let packages = document
                .get("packages")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| refuse("no `packages` array"))?;
            let package = packages
                .iter()
                .find(|package| {
                    package
                        .get("manifest_path")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|path| std::path::Path::new(path) == manifest)
                })
                .ok_or_else(|| InventoryRefusal::NoPackage {
                    manifest: manifest.to_path_buf(),
                })?;
            let targets = package
                .get("targets")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| refuse("the package has no `targets` array"))?;
            let mut roots = std::collections::BTreeSet::new();
            for target in targets {
                let source = target
                    .get("src_path")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| refuse("a target carries no `src_path`"))?;
                roots.insert(PathBuf::from(source));
            }
            if roots.is_empty() {
                return Err(InventoryRefusal::NoTargets {
                    manifest: manifest.to_path_buf(),
                });
            }
            Ok(Self {
                package_dir: manifest
                    .parent()
                    .ok_or_else(|| refuse("the manifest path has no directory"))?
                    .to_path_buf(),
                roots,
            })
        }

        pub(crate) fn package_dir(&self) -> &std::path::Path {
            &self.package_dir
        }

        pub(crate) fn roots(&self) -> impl Iterator<Item = &std::path::Path> {
            self.roots.iter().map(PathBuf::as_path)
        }

        pub(crate) fn is_root(&self, path: &std::path::Path) -> bool {
            self.roots.contains(path)
        }

        pub(crate) fn is_root_relative(&self, relative: &str) -> bool {
            let mut candidate = self.package_dir.clone();
            for part in relative.split('/') {
                candidate.push(part);
            }
            self.is_root(&candidate)
        }
    }

    pub(crate) fn module_directory(
        roots: &CrateRoots,
        declared_in: &std::path::Path,
    ) -> Result<PathBuf, CandidateRefusal> {
        let parent = declared_in
            .parent()
            .expect("a source file has a directory")
            .to_path_buf();
        let stem = declared_in.file_stem().expect("a source file has a name");
        if roots.is_root(declared_in) {
            return Ok(parent);
        }
        if !declared_in.starts_with(roots.package_dir()) {
            return Err(CandidateRefusal::OutsideThePackage {
                declared_in: declared_in.to_path_buf(),
                package_dir: roots.package_dir().to_path_buf(),
            });
        }
        if stem == "mod" {
            return Ok(parent);
        }
        Ok(parent.join(stem))
    }

    pub(crate) fn candidates_for(
        roots: &CrateRoots,
        declared_in: &std::path::Path,
        inline_path: &[String],
        name: &str,
    ) -> Result<[PathBuf; 2], CandidateRefusal> {
        let mut dir = module_directory(roots, declared_in)?;
        for enclosing in inline_path {
            dir.push(enclosing);
        }
        Ok([
            dir.join(format!("{name}.rs")),
            dir.join(name).join("mod.rs"),
        ])
    }

    pub(crate) fn contained_in(base: &std::path::Path, candidate: &std::path::Path) -> bool {
        let Ok(rest) = candidate.strip_prefix(base) else {
            return false;
        };
        rest.components().count() > 0
            && rest
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct TestModuleDeclaration {
        pub(crate) declared_in: PathBuf,
        pub(crate) name: String,
        pub(crate) inline_path: Vec<String>,
        pub(crate) guard: String,
        pub(crate) candidates: [PathBuf; 2],
    }

    impl TestModuleDeclaration {
        fn render_guard(&self) -> String {
            if self.inline_path.is_empty() {
                format!("`cfg({})`", self.guard)
            } else {
                format!(
                    "`cfg({})` through `{}`",
                    self.guard,
                    self.inline_path.join("::")
                )
            }
        }
    }

    pub(crate) fn declared_whole_file_test_modules(
        source_root: &std::path::Path,
        files: &[PathBuf],
    ) -> Vec<TestModuleDeclaration> {
        let roots = crate::effects::tests::crate_roots();
        assert!(
            roots.roots().any(|root| root.starts_with(source_root)),
            "no target of the package at `{}` lives under `{}`, so its inventory does not \
             describe the tree this census was handed: {:?}",
            roots.package_dir().display(),
            source_root.display(),
            roots.roots().collect::<Vec<_>>()
        );
        let mut found = Vec::new();
        for path in files {
            let source = std::fs::read_to_string(path).expect("read source");
            let declarations = scan_module_declarations(&source)
                .unwrap_or_else(|refusal| panic!("{}: {refusal}", path.display()));
            let parent = path.parent().expect("a source file has a directory");
            for declaration in declarations {
                if !declaration.test_only {
                    continue;
                }
                let candidates =
                    candidates_for(roots, path, &declaration.inline_path, &declaration.name)
                        .unwrap_or_else(|refusal| panic!("{refusal}"));
                for candidate in &candidates {
                    assert!(
                        contained_in(parent, candidate),
                        "`{}` declares `mod {};` and the candidate `{}` leaves `{}`",
                        path.display(),
                        declaration.name,
                        candidate.display(),
                        parent.display()
                    );
                }
                found.push(TestModuleDeclaration {
                    declared_in: path.clone(),
                    name: declaration.name,
                    inline_path: declaration.inline_path,
                    guard: declaration.guard,
                    candidates,
                });
            }
        }
        found
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct ScannedDeclaration {
        pub(crate) name: String,
        pub(crate) inline_path: Vec<String>,
        pub(crate) guard: String,
        pub(crate) test_only: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum ScanRefusal {
        UnclosedAttribute {
            line: usize,
        },
        UnbalancedBraces {
            line: usize,
        },
        MalformedDeclaration {
            line: usize,
        },
        UnreadablePredicate {
            line: usize,
            written: String,
            why: String,
        },
        UnsupportedPathAttribute {
            line: usize,
            name: String,
        },
        UnsupportedInnerCfg {
            line: usize,
        },
        DuplicateDeclaration {
            line: usize,
            name: String,
        },
        ModuleShapedMacroBody {
            line: usize,
            macro_name: String,
        },
    }

    impl std::fmt::Display for ScanRefusal {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::UnclosedAttribute { line } => {
                    write!(f, "line {line}: an attribute is never closed")
                }
                Self::UnbalancedBraces { line } => {
                    write!(
                        f,
                        "line {line}: a `}}` closes a block that was never opened"
                    )
                }
                Self::MalformedDeclaration { line } => write!(
                    f,
                    "line {line}: a `mod` declaration has no name, or no `;` or `{{` after it"
                ),
                Self::UnreadablePredicate { line, written, why } => write!(
                    f,
                    "line {line}: `cfg({written})` cannot be decided against `test`: {why}"
                ),
                Self::UnsupportedPathAttribute { line, name } => write!(
                    f,
                    "line {line}: `mod {name}` carries a `path` attribute, which this \
                     derivation refuses rather than resolves"
                ),
                Self::UnsupportedInnerCfg { line } => write!(
                    f,
                    "line {line}: an inner `#![cfg(…)]` gates the module it is written in, \
                     which this derivation does not model"
                ),
                Self::DuplicateDeclaration { line, name } => {
                    write!(
                        f,
                        "line {line}: `mod {name};` is declared twice in one module"
                    )
                }
                Self::ModuleShapedMacroBody { line, macro_name } => write!(
                    f,
                    "line {line}: the body of `{macro_name}!` holds a module-shaped token \
                     sequence. A macro body is token trees, not items, and whether the \
                     expansion declares a module is not readable from here"
                ),
            }
        }
    }

    pub(crate) fn scan_module_declarations(
        source: &str,
    ) -> Result<Vec<ScannedDeclaration>, ScanRefusal> {
        struct Scope {
            open_depth: usize,
            name: String,
            preds: Vec<Predicate>,
            declared: std::collections::BTreeSet<String>,
        }

        let blanked = super::blank_comments_and_strings(source);
        debug_assert_eq!(blanked.len(), source.len());
        let bytes = blanked.as_bytes();
        let line_of = |at: usize| blanked[..at].matches('\n').count() + 1;

        let mut found = Vec::new();
        let mut scopes: Vec<Scope> = Vec::new();
        let mut top_level: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut pending: Vec<Predicate> = Vec::new();
        let mut pending_path = false;
        let mut depth = 0_usize;
        let mut i = 0;

        while i < bytes.len() {
            let byte = bytes[i];
            if byte.is_ascii_whitespace() {
                i += 1;
                continue;
            }

            if byte == b'#' {
                let inner = bytes.get(i + 1) == Some(&b'!');
                let open = if inner { i + 2 } else { i + 1 };
                if bytes.get(open) != Some(&b'[') {
                    i += 1;
                    continue;
                }
                let Some(close) = super::matching(bytes, open, b'[', b']') else {
                    return Err(ScanRefusal::UnclosedAttribute { line: line_of(i) });
                };
                let raw = &source[open + 1..close];
                let name = raw
                    .trim_start()
                    .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                    .next()
                    .unwrap_or_default();
                match name {
                    "cfg" => {
                        let written = raw
                            .trim()
                            .strip_prefix("cfg")
                            .map(str::trim_start)
                            .and_then(|rest| rest.strip_prefix('('))
                            .and_then(|rest| rest.strip_suffix(')'))
                            .unwrap_or_default()
                            .trim();
                        let pred = parse_predicate(written).map_err(|why| {
                            ScanRefusal::UnreadablePredicate {
                                line: line_of(i),
                                written: written.to_owned(),
                                why,
                            }
                        })?;
                        if inner {
                            return Err(ScanRefusal::UnsupportedInnerCfg { line: line_of(i) });
                        }
                        pending.push(pred);
                    }
                    "path" => pending_path = true,
                    "cfg_attr" if raw.contains("path") => pending_path = true,
                    _ => {}
                }
                i = close + 1;
                continue;
            }

            if let Some(invocation) = macro_at(bytes, i) {
                let MacroInvocation { name, open, close } = invocation;
                if let Some(shaped) = module_shaped_between(bytes, open + 1, close) {
                    return Err(ScanRefusal::ModuleShapedMacroBody {
                        line: line_of(shaped),
                        macro_name: name,
                    });
                }
                pending.clear();
                pending_path = false;
                i = close + 1;
                continue;
            }

            if let Some(shape) = module_at(bytes, i) {
                let ModuleShape {
                    name_at,
                    name,
                    body,
                } = shape;
                if name.is_empty() {
                    return Err(ScanRefusal::MalformedDeclaration { line: line_of(i) });
                }
                if pending_path {
                    return Err(ScanRefusal::UnsupportedPathAttribute {
                        line: line_of(i),
                        name,
                    });
                }
                let mut preds: Vec<Predicate> = scopes
                    .iter()
                    .flat_map(|scope| scope.preds.iter().cloned())
                    .collect();
                preds.extend(pending.iter().cloned());
                match body {
                    Some(brace) => {
                        scopes.push(Scope {
                            open_depth: depth,
                            name,
                            preds: std::mem::take(&mut pending),
                            declared: std::collections::BTreeSet::new(),
                        });
                        depth += 1;
                        i = brace + 1;
                    }
                    None => {
                        let declared = match scopes.last_mut() {
                            Some(scope) => &mut scope.declared,
                            None => &mut top_level,
                        };
                        if !declared.insert(name.clone()) {
                            return Err(ScanRefusal::DuplicateDeclaration {
                                line: line_of(name_at),
                                name,
                            });
                        }
                        let effective = Predicate::all(preds);
                        found.push(ScannedDeclaration {
                            name,
                            inline_path: scopes.iter().map(|scope| scope.name.clone()).collect(),
                            guard: effective.render(),
                            test_only: entails_test(&effective),
                        });
                        pending.clear();
                        i = bytes[name_at..]
                            .iter()
                            .position(|byte| *byte == b';')
                            .map_or(bytes.len(), |at| name_at + at + 1);
                    }
                }
                pending_path = false;
                continue;
            }

            pending.clear();
            pending_path = false;
            if byte == b'{' {
                depth += 1;
                i += 1;
                continue;
            }
            if byte == b'}' {
                if depth == 0 {
                    return Err(ScanRefusal::UnbalancedBraces { line: line_of(i) });
                }
                depth -= 1;
                scopes.retain(|scope| scope.open_depth < depth);
                i += 1;
                continue;
            }
            if super::is_ident_byte(byte) {
                i = word(bytes, i).end;
                continue;
            }
            i += 1;
        }
        Ok(found)
    }

    struct MacroInvocation {
        name: String,
        open: usize,
        close: usize,
    }

    fn macro_at(bytes: &[u8], at: usize) -> Option<MacroInvocation> {
        let name = word(bytes, at);
        let after_name = name.end;
        if name.text.is_empty() {
            return None;
        }
        if !name.raw && is_keyword(name.text) {
            return None;
        }
        let bang = whitespace(bytes, after_name);
        if bytes.get(bang) != Some(&b'!') {
            return None;
        }
        let mut cursor = whitespace(bytes, bang + 1);
        if !name.raw && name.text == b"macro_rules" {
            let defined = word(bytes, cursor);
            if !defined.text.is_empty() {
                cursor = whitespace(bytes, defined.end);
            }
        }
        let (opener, closer) = match bytes.get(cursor) {
            Some(b'(') => (b'(', b')'),
            Some(b'[') => (b'[', b']'),
            Some(b'{') => (b'{', b'}'),
            _ => return None,
        };
        let close = super::matching(bytes, cursor, opener, closer)?;
        Some(MacroInvocation {
            name: String::from_utf8_lossy(name.text).into_owned(),
            open: cursor,
            close,
        })
    }

    fn module_shaped_between(bytes: &[u8], from: usize, to: usize) -> Option<usize> {
        let mut at = from;
        while at < to {
            if !super::is_ident_byte(bytes[at]) {
                at += 1;
                continue;
            }
            let keyword = word(bytes, at);
            if !keyword.raw && keyword.text == b"mod" {
                let name_at = whitespace(bytes, keyword.end);
                if name_at > keyword.end {
                    let declared = word(bytes, name_at);
                    if !declared.text.is_empty()
                        && matches!(
                            bytes.get(whitespace(bytes, declared.end)),
                            Some(b';' | b'{')
                        )
                    {
                        return Some(at);
                    }
                }
            }
            at = keyword.end;
        }
        None
    }

    fn identifier(bytes: &[u8], from: usize) -> (usize, &[u8]) {
        let mut end = from;
        while end < bytes.len() && super::is_ident_byte(bytes[end]) {
            end += 1;
        }
        (end, &bytes[from..end])
    }

    struct Word<'a> {
        end: usize,
        raw: bool,
        text: &'a [u8],
    }

    fn word(bytes: &[u8], from: usize) -> Word<'_> {
        if bytes.get(from) == Some(&b'r')
            && bytes.get(from + 1) == Some(&b'#')
            && bytes
                .get(from + 2)
                .is_some_and(|byte| super::is_ident_byte(*byte))
        {
            let (end, text) = identifier(bytes, from + 2);
            return Word {
                end,
                raw: true,
                text,
            };
        }
        let (end, text) = identifier(bytes, from);
        Word {
            end,
            raw: false,
            text,
        }
    }

    const KEYWORDS: &[&[u8]] = &[
        b"as",
        b"break",
        b"const",
        b"continue",
        b"crate",
        b"dyn",
        b"else",
        b"enum",
        b"extern",
        b"false",
        b"fn",
        b"for",
        b"if",
        b"impl",
        b"in",
        b"let",
        b"loop",
        b"match",
        b"mod",
        b"move",
        b"mut",
        b"pub",
        b"ref",
        b"return",
        b"self",
        b"Self",
        b"static",
        b"struct",
        b"super",
        b"trait",
        b"true",
        b"type",
        b"unsafe",
        b"use",
        b"where",
        b"while",
        b"async",
        b"await",
        b"dyn",
        b"abstract",
        b"become",
        b"box",
        b"do",
        b"final",
        b"macro",
        b"override",
        b"priv",
        b"typeof",
        b"unsized",
        b"virtual",
        b"yield",
        b"try",
        b"gen",
    ];

    fn is_keyword(text: &[u8]) -> bool {
        KEYWORDS.contains(&text)
    }

    fn whitespace(bytes: &[u8], from: usize) -> usize {
        let mut at = from;
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        at
    }

    struct ModuleShape {
        name_at: usize,
        name: String,
        body: Option<usize>,
    }

    fn module_at(bytes: &[u8], at: usize) -> Option<ModuleShape> {
        let mut token = word(bytes, at);
        if !token.raw && token.text == b"pub" {
            let after = whitespace(bytes, token.end);
            let cursor = if bytes.get(after) == Some(&b'(') {
                super::matching(bytes, after, b'(', b')')? + 1
            } else {
                after
            };
            token = word(bytes, whitespace(bytes, cursor));
        }
        if token.raw || token.text != b"mod" {
            return None;
        }
        let after_keyword = whitespace(bytes, token.end);
        if after_keyword == token.end {
            return None;
        }
        let declared = word(bytes, after_keyword);
        let name_end = declared.end;
        let name = String::from_utf8_lossy(declared.text).into_owned();
        let terminator = whitespace(bytes, name_end);
        match bytes.get(terminator) {
            Some(b'{') => Some(ModuleShape {
                name_at: after_keyword,
                name,
                body: Some(terminator),
            }),
            Some(b';') => Some(ModuleShape {
                name_at: after_keyword,
                name,
                body: None,
            }),
            _ => Some(ModuleShape {
                name_at: after_keyword,
                name: String::new(),
                body: None,
            }),
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Predicate {
        Test,
        Other(String),
        All(Vec<Predicate>),
        Any(Vec<Predicate>),
        Not(Box<Predicate>),
    }

    impl Predicate {
        fn all(parts: Vec<Predicate>) -> Self {
            if parts.len() == 1 {
                parts.into_iter().next().unwrap_or(Self::All(Vec::new()))
            } else {
                Self::All(parts)
            }
        }

        pub(crate) fn render(&self) -> String {
            fn join(parts: &[Predicate]) -> String {
                parts
                    .iter()
                    .map(Predicate::render)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            match self {
                Self::Test => "test".to_owned(),
                Self::Other(written) => written.clone(),
                Self::All(parts) if parts.is_empty() => "true".to_owned(),
                Self::All(parts) => format!("all({})", join(parts)),
                Self::Any(parts) => format!("any({})", join(parts)),
                Self::Not(inner) => format!("not({})", inner.render()),
            }
        }
    }

    pub(crate) fn entails_test(predicate: &Predicate) -> bool {
        matches!(decide_without_test(predicate), Some(false))
    }

    fn decide_without_test(predicate: &Predicate) -> Option<bool> {
        match predicate {
            Predicate::Test => Some(false),
            Predicate::Other(_) => None,
            Predicate::Not(inner) => decide_without_test(inner).map(|value| !value),
            Predicate::All(parts) => {
                let mut every_part_is_true = true;
                for part in parts {
                    match decide_without_test(part) {
                        Some(false) => return Some(false),
                        Some(true) => {}
                        None => every_part_is_true = false,
                    }
                }
                every_part_is_true.then_some(true)
            }
            Predicate::Any(parts) => {
                let mut every_part_is_false = true;
                for part in parts {
                    match decide_without_test(part) {
                        Some(true) => return Some(true),
                        Some(false) => {}
                        None => every_part_is_false = false,
                    }
                }
                every_part_is_false.then_some(false)
            }
        }
    }

    pub(crate) fn parse_predicate(written: &str) -> Result<Predicate, String> {
        let text = written.trim();
        if text.is_empty() {
            return Err("the predicate is empty".to_owned());
        }
        let name_end = text
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .unwrap_or(text.len());
        let (name, rest) = text.split_at(name_end);
        let rest = rest.trim_start();
        if !rest.starts_with('(') {
            if name.is_empty() {
                return Err(format!("`{text}` does not begin with a name"));
            }
            if rest.is_empty() {
                return Ok(if name == "test" {
                    Predicate::Test
                } else {
                    Predicate::Other(name.to_owned())
                });
            }
            let Some(value) = rest.strip_prefix('=') else {
                return Err(format!("`{text}` is neither an atom nor a combinator"));
            };
            if value.trim().is_empty() {
                return Err(format!("`{name}` is compared with nothing"));
            }
            return Ok(Predicate::Other(text.to_owned()));
        }
        let inner = split_arguments(rest)?;
        let parts = inner
            .into_iter()
            .map(parse_predicate)
            .collect::<Result<Vec<_>, _>>()?;
        match name {
            "all" => Ok(Predicate::All(parts)),
            "any" => Ok(Predicate::Any(parts)),
            "not" => match <[Predicate; 1]>::try_from(parts) {
                Ok([only]) => Ok(Predicate::Not(Box::new(only))),
                Err(parts) => Err(format!("`not` takes one predicate, not {}", parts.len())),
            },
            other => Err(format!("`{other}(…)` is not a predicate combinator")),
        }
    }

    fn split_arguments(text: &str) -> Result<Vec<&str>, String> {
        let bytes = text.as_bytes();
        let mut depth = 0_usize;
        let mut close = None;
        let mut quoted = false;
        for (at, byte) in bytes.iter().enumerate() {
            match byte {
                b'"' => quoted = !quoted,
                b'(' if !quoted => depth += 1,
                b')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(at);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            return Err(format!("`{text}` has an unbalanced parenthesis"));
        };
        if !text[close + 1..].trim().is_empty() {
            return Err(format!("`{text}` has text after its closing parenthesis"));
        }
        let body = &text[1..close];
        if body.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut parts = Vec::new();
        let mut depth = 0_usize;
        let mut quoted = false;
        let mut from = 0;
        for (at, byte) in body.bytes().enumerate() {
            match byte {
                b'"' => quoted = !quoted,
                b'(' if !quoted => depth += 1,
                b')' if !quoted => depth -= 1,
                b',' if !quoted && depth == 0 => {
                    parts.push(&body[from..at]);
                    from = at + 1;
                }
                _ => {}
            }
        }
        let last = &body[from..];
        if !last.trim().is_empty() {
            parts.push(last);
        }
        Ok(parts)
    }
}

#[cfg(test)]
pub(crate) mod lint_levels {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Resolution {
        pub(crate) level: Option<&'static str>,
        pub(crate) refused_downgrade: bool,
        /// The prologue says something about this lint that the reader cannot
        /// resolve to one answer for every supported target — a `cfg_attr`
        /// whose condition names a target, a feature or `test`, or an
        /// attribute shape the reader does not understand.
        ///
        /// It is a **refusal**, not a level: `level` is `None` whenever it is
        /// set, so every census that asks "has this module closed the hole"
        /// is told no. The field exists so a test can tell a prologue that
        /// says nothing apart from one that says something unprovable; a
        /// census does not need to, because both must be loud.
        pub(crate) ambiguous: bool,
    }

    #[must_use]
    pub(crate) fn file_level_lint_resolution(source: &str, lint: &str) -> Resolution {
        let blanked = super::blank_comments_and_strings(source);
        let bytes = blanked.as_bytes();
        let mut applied: Vec<Applied> = Vec::new();
        let mut readable = true;
        let mut at = 0;
        loop {
            at = super::skip_blank(&blanked, at);
            if at >= bytes.len() {
                break;
            }
            // The prologue ends at the first token that is not an inner
            // attribute. Which bytes are one is `super::attribute_opener`'s
            // answer, the same one the placement scan gets, so the two readers
            // cannot disagree about where an attribute begins.
            let Some(opener) = super::attribute_opener(&blanked, at) else {
                break;
            };
            if !opener.inner {
                break;
            }
            let Some(close) = super::matching(bytes, opener.open, b'[', b']') else {
                // An inner attribute whose brackets do not close. Everything
                // after it is unread, so the answer is a refusal rather than
                // whatever the attributes before it happened to say.
                readable = false;
                break;
            };
            readable &= read_attribute(
                &blanked[opener.open + 1..close],
                lint,
                Truth::Always,
                &mut applied,
            );
            at = close + 1;
        }
        resolve(&applied, readable)
    }

    /// One level-setting attribute the prologue applies to the lint, with the
    /// condition the reader proved it is under.
    #[derive(Debug, Clone, Copy)]
    struct Applied {
        level: &'static str,
        truth: Truth,
    }

    /// A `cfg` condition's truth over the targets this repository supports.
    ///
    /// The reader proves exactly two things — `all()` is the empty conjunction
    /// and is true everywhere, `any()` is the empty disjunction and is false
    /// everywhere — and composes `all`/`any`/`not` over them. **It models no
    /// target list at all.** A list is a place to be wrong, and the way to be
    /// wrong here is to report a module as guarded on a target where it is not,
    /// so `windows`, `target_os = "…"`, `feature = "…"` and `test` are each
    /// [`Truth::Unknown`] and stay that way.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Truth {
        Always,
        Never,
        Unknown,
    }

    impl Truth {
        /// Both conditions, which is what one `cfg_attr` inside another means.
        fn both(self, other: Self) -> Self {
            match (self, other) {
                (Self::Never, _) | (_, Self::Never) => Self::Never,
                (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
                _ => Self::Always,
            }
        }

        /// Either condition.
        fn either(self, other: Self) -> Self {
            match (self, other) {
                (Self::Always, _) | (_, Self::Always) => Self::Always,
                (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
                _ => Self::Never,
            }
        }

        /// The complement. An unprovable condition has an unprovable one.
        fn negate(self) -> Self {
            match self {
                Self::Always => Self::Never,
                Self::Never => Self::Always,
                Self::Unknown => Self::Unknown,
            }
        }
    }

    /// [`Truth`] of one `cfg` predicate.
    fn evaluate(predicate: &str) -> Truth {
        let Some((head, body)) = split_call(predicate) else {
            // A leaf: `test`, `windows`, `feature = "strict"`. Not modelled.
            return Truth::Unknown;
        };
        match head {
            "all" => split_top_level(body)
                .into_iter()
                .fold(Truth::Always, |truth, term| truth.both(evaluate(term))),
            "any" => split_top_level(body)
                .into_iter()
                .fold(Truth::Never, |truth, term| truth.either(evaluate(term))),
            "not" => match split_top_level(body).as_slice() {
                [one] => evaluate(one).negate(),
                // `not` takes exactly one predicate. Anything else is a shape
                // this reader has not understood.
                _ => Truth::Unknown,
            },
            _ => Truth::Unknown,
        }
    }

    /// Read one attribute's contents — already blanked — and append every level
    /// it applies to `lint`, each under the condition it is written beneath.
    ///
    /// `truth` is the condition inherited from the `cfg_attr` chain above it,
    /// `Truth::Always` at the top. Returns **false** when the attribute names
    /// the lint somewhere this reader could not resolve: an absence and a
    /// refusal are not the same answer and the caller must not confuse them.
    fn read_attribute(
        attribute: &str,
        lint: &str,
        truth: Truth,
        applied: &mut Vec<Applied>,
    ) -> bool {
        const LEVELS: [&str; 5] = ["allow", "expect", "warn", "deny", "forbid"];
        let attribute = super::trim_rust(attribute);
        let Some((head, body)) = split_call(attribute) else {
            // Not a call: `#![no_std]`, `#![doc = "…"]`. It sets no level, and
            // the string a `doc =` carries is already blanked.
            return !mentions(attribute, lint);
        };
        if head == "cfg_attr" {
            // `#![cfg_attr(P, a, b)]` applies EVERY attribute after the
            // predicate, not just the first.
            let mut terms = split_top_level(body);
            if terms.is_empty() {
                return !mentions(body, lint);
            }
            let condition = truth.both(evaluate(terms.remove(0)));
            let mut readable = true;
            for term in terms {
                readable &= read_attribute(term, lint, condition, applied);
            }
            return readable;
        }
        if let Some(level) = LEVELS.iter().copied().find(|level| *level == head) {
            let entries = split_top_level(body);
            if entries.iter().any(|entry| names_lint(entry, lint)) {
                applied.push(Applied { level, truth });
                return true;
            }
            // An entry that MENTIONS the lint without parsing as its name is a
            // shape this reader has not understood, and "this attribute says
            // nothing about that lint" is the wrong answer for it.
            // `allow(\u{00A0}clippy::disallowed_methods)` is the live case:
            // U+00A0 is not Rust whitespace, rustc refuses the file, and
            // silence here would let a census read the module as merely
            // unannotated. A refusal is the loud answer and the true one.
            return !entries.iter().any(|entry| mentions(entry, lint));
        }
        // Some other attribute. It governs no lint level, so it matters only if
        // it names this lint somewhere the reader has not understood — and then
        // it is a refusal, because the alternative is a reader guessing that
        // nothing was stated.
        !mentions(attribute, lint)
    }

    /// The level the applied attributes leave in force, in source order.
    ///
    /// Only [`Truth::Always`] attributes decide it: a [`Truth::Never`] one is
    /// not in the file on any target and a [`Truth::Unknown`] one refuses the
    /// whole answer. **A refusal carries no level**, so the two censuses that
    /// consult this reader are told the module has stated nothing — which is
    /// the direction that is loud.
    fn resolve(applied: &[Applied], readable: bool) -> Resolution {
        let mut level: Option<&'static str> = None;
        let mut refused_downgrade = false;
        let mut ambiguous = !readable;
        for entry in applied {
            match entry.truth {
                Truth::Never => {}
                Truth::Unknown => ambiguous = true,
                // Ordered, and `forbid` is sticky. A weaker level after a
                // `forbid` is `E0453`, which is the file not compiling rather
                // than a level; anything else replaces what came before it.
                Truth::Always => {
                    if level == Some("forbid") {
                        if matches!(entry.level, "allow" | "warn" | "expect") {
                            refused_downgrade = true;
                        }
                    } else {
                        level = Some(entry.level);
                    }
                }
            }
        }
        if ambiguous {
            return Resolution {
                level: None,
                refused_downgrade: false,
                ambiguous: true,
            };
        }
        Resolution {
            level,
            refused_downgrade,
            ambiguous: false,
        }
    }

    /// `head` and the contents of `head(…)`, when `text` is exactly that call.
    ///
    /// `head` must be a bare identifier and the closing parenthesis must be the
    /// last thing in `text`, so `allowance(…)` is a call named `allowance` and
    /// `deny(a) something` is not a call at all. Both refusals are deliberate:
    /// this is the structural half of the reader, and a prefix match is what it
    /// replaced.
    fn split_call(text: &str) -> Option<(&str, &str)> {
        let text = super::trim_rust(text);
        let (head, after) = super::identifier_token(text, 0)?;
        let open = super::skip_blank(text, after);
        if text.as_bytes().get(open) != Some(&b'(') {
            return None;
        }
        let close = super::matching(text.as_bytes(), open, b'(', b')')?;
        if !super::trim_rust(&text[close + 1..]).is_empty() {
            return None;
        }
        Some((head, &text[open + 1..close]))
    }

    /// `list` split on the commas that are not inside a nested group.
    ///
    /// `all(unix, windows), deny(L)` is two terms, not three. Empty terms are
    /// dropped, so `all()` yields none — which is what makes the empty
    /// conjunction true and the empty disjunction false.
    fn split_top_level(list: &str) -> Vec<&str> {
        let mut terms = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (index, byte) in list.bytes().enumerate() {
            match byte {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = depth.saturating_sub(1),
                b',' if depth == 0 => {
                    terms.push(super::trim_rust(&list[start..index]));
                    start = index + 1;
                }
                _ => {}
            }
        }
        terms.push(super::trim_rust(&list[start..]));
        terms.retain(|term| !term.is_empty());
        terms
    }

    /// Whether `text` names `lint` as a word rather than as part of a longer one.
    ///
    /// The bare name, because an attribute may write either spelling, and
    /// bounded on both sides so `disallowed_methods_extra` is not this lint.
    fn mentions(text: &str, lint: &str) -> bool {
        let canonical = match super::normalize_lint(lint) {
            Some(name) => name,
            None => lint.rsplit("::").next().unwrap_or(lint),
        };
        let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
        // The lint's own name AND every alias that resolves to it: a shape this
        // reader could not parse is no less about the lint for having been
        // written in the spelling clippy renamed.
        std::iter::once(canonical)
            .chain(
                super::GOVERNED_LINT_ALIASES
                    .iter()
                    .filter(|(_, to)| *to == canonical)
                    .map(|(alias, _)| *alias),
            )
            .any(|needle| {
                text.match_indices(needle).any(|(at, _)| {
                    !text.as_bytes()[..at].last().copied().is_some_and(word)
                        && !text
                            .as_bytes()
                            .get(at + needle.len())
                            .copied()
                            .is_some_and(word)
                })
            })
    }

    #[must_use]
    pub(crate) fn file_level_lint_state(source: &str, lint: &str) -> Option<&'static str> {
        file_level_lint_resolution(source, lint).level
    }

    fn names_lint(entry: &str, lint: &str) -> bool {
        if entry == lint {
            return true;
        }
        match (super::normalize_lint(entry), super::normalize_lint(lint)) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
