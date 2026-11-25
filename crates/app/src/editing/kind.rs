//! What a file is (specs/oneterm, "M9 in detail"): text or binary, and the type that colours it,
//! by its name, else by the program its first line names (`#!/usr/bin/env python3`).

use quadra_code::{Tokenizer, tokenizer_for};

/// What a file holds: text, or bytes that are not shown (their count).
pub(crate) enum Content {
    Text(String),
    Binary(u64),
}

impl Content {
    /// Bytes as text, unless a NUL is in their first 8 KB or they are not UTF-8.
    pub fn of(bytes: Vec<u8>) -> Content {
        let size = bytes.len() as u64;
        if bytes[..bytes.len().min(8192)].contains(&0) {
            return Content::Binary(size);
        }
        String::from_utf8(bytes).map_or(Content::Binary(size), Content::Text)
    }
}

/// The name its colours go by: the file's when it names a type, else the program of a first
/// line that starts with `#!` (after `env`), else the file's.
pub(crate) fn kind(name: &str, text: &str) -> String {
    if tokenizer_for(name) != Tokenizer::Plain {
        return name.to_string();
    }
    let Some(line) = text.lines().next().and_then(|it| it.strip_prefix("#!")) else {
        return name.to_string();
    };
    let mut words = line.split_whitespace().filter(|it| !it.starts_with('-'));
    let program = |word: &str| word.rsplit('/').next().unwrap_or(word).to_string();
    let first = words.next().map(program).unwrap_or_default();
    let program = if first == "env" { words.next().map(program).unwrap_or_default() } else { first };
    if tokenizer_for(&program) == Tokenizer::Plain { name.to_string() } else { program }
}

/// The type a kind is, as the header says it: `yaml`, `python`, `text`.
pub(crate) fn label(kind: &str) -> &'static str {
    match tokenizer_for(kind) {
        Tokenizer::Plain => "text",
        Tokenizer::V => "v",
        Tokenizer::Yaml => "yaml",
        Tokenizer::Markdown => "markdown",
        Tokenizer::Lang(spec) => spec.names.split(' ').next().unwrap_or("text"),
    }
}

#[cfg(test)]
#[test]
fn types_by_name_then_by_the_first_line() {
    assert_eq!(kind("config.yaml", "#!/bin/sh"), "config.yaml");
    assert_eq!(kind("run", "#!/usr/bin/env python3\nprint(1)"), "python3");
    assert_eq!(kind("deploy", "#!/bin/bash -e\n"), "bash");
    assert_eq!(kind("notes", "plain words"), "notes");
    assert_eq!(kind("x", "#!/usr/bin/env -S unknown"), "x");
    assert_eq!(
        (label("config.yaml"), label("python3"), label("bash"), label("notes")),
        ("yaml", "python", "sh", "text")
    );
    assert!(matches!(Content::of(b"a\0b".to_vec()), Content::Binary(3)));
    assert!(matches!(Content::of(vec![0xff, 0xfe, b'a']), Content::Binary(3)));
    assert!(matches!(Content::of(b"text\n".to_vec()), Content::Text(t) if t == "text\n"));
}
