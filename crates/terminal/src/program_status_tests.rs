//! Rev 0.3 rules, one table per concern (`IN-0049` receiver LLD).

use base64::Engine as _;

use super::*;

fn body(text: &str) -> Option<Osc7501> {
    parse(&[b"7501", text.as_bytes()])
}

fn report(id: &str, state: Option<ProgramState>) -> Option<Osc7501> {
    Some(Osc7501::Report {
        id: id.into(),
        state,
    })
}

fn b64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

#[test]
fn the_probe_is_a_lone_question_mark() {
    assert_eq!(body("?"), Some(Osc7501::Probe));
    assert_eq!(probe_reply(StringTerm::Bel), b"\x1b]7501;?\x07");
    assert_eq!(probe_reply(StringTerm::St), b"\x1b]7501;?\x1b\\");
}

#[test]
fn every_state_parses_and_clear_is_none() {
    for (text, state) in [
        ("idle", Some(ProgramState::Idle)),
        ("working", Some(ProgramState::Working)),
        ("done", Some(ProgramState::Done)),
        ("blocked", Some(ProgramState::Blocked)),
        ("error", Some(ProgramState::Error)),
        ("clear", None),
    ] {
        assert_eq!(body(&format!("state={text}")), report("", state), "{text}");
    }
}

#[test]
fn a_missing_or_unknown_state_drops_the_report() {
    for text in ["", "app=claude", "state=", "state=running", "state=Working"] {
        assert_eq!(body(text), None, "{text:?}");
    }
}

#[test]
fn a_second_parameter_drops_the_report() {
    assert_eq!(parse(&[b"7501", b"state=working", b"x"]), None);
    assert_eq!(parse(&[b"7501"]), None);
}

#[test]
fn malformed_pairs_are_skipped_unknown_keys_ignored_and_the_last_repeat_wins() {
    assert_eq!(
        body("noequals:Bad=x:state=working:future=1:app=claude"),
        report("", Some(ProgramState::Working))
    );
    assert_eq!(
        body("state=working:state=done"),
        report("", Some(ProgramState::Done))
    );
    // A value with a byte outside the charset is a malformed pair, so the
    // earlier valid `state` stands.
    assert_eq!(
        body("state=done:state=wo rking"),
        report("", Some(ProgramState::Done))
    );
    // A pair outside the charset is skipped, not fatal: the report stands.
    assert_eq!(
        body("state=done:msg=!!!!"),
        report("", Some(ProgramState::Done))
    );
    // A key longer than 16 bytes is malformed.
    assert_eq!(
        body("abcdefghijklmnopq=1:state=idle"),
        report("", Some(ProgramState::Idle))
    );
}

#[test]
fn ids_follow_the_segment_rules() {
    assert_eq!(
        body("state=working:id=agent/sub-1.a+b_c"),
        report("agent/sub-1.a+b_c", Some(ProgramState::Working))
    );
    let eight = ["a"; 8].join("/");
    assert!(body(&format!("state=idle:id={eight}")).is_some());
    let segment_32 = "x".repeat(32);
    assert!(body(&format!("state=idle:id={segment_32}")).is_some());

    let nine = ["a"; 9].join("/");
    let segment_33 = "x".repeat(33);
    let long = format!("{0}/{0}/{0}/{1}", "x".repeat(32), "x".repeat(30)); // 129 bytes
    assert_eq!(long.len(), 129);
    for id in [
        "".to_string(),
        "a//b".into(),
        "/a".into(),
        "a/".into(),
        nine,
        segment_33,
        long,
        "a,b".into(),
    ] {
        assert_eq!(body(&format!("state=idle:id={id}")), None, "{id:?}");
    }
}

#[test]
fn text_fields_are_validated_and_dropped_whole_when_unsafe() {
    let ok = format!(
        "state=working:title={}:msg={}",
        b64("Build"),
        b64("compiling")
    );
    assert_eq!(body(&ok), report("", Some(ProgramState::Working)));
    // Padding is optional.
    let unpadded = b64("ab").trim_end_matches('=').to_string();
    assert!(body(&format!("state=done:msg={unpadded}")).is_some());

    let too_long = "x".repeat(2049);
    let title_long = "x".repeat(193);
    for bad in [
        // In the value charset, not base64.
        "state=done:msg=Zm9v,".to_string(),
        "state=done:msg=a".into(), // not base64
        format!("state=done:msg={}", b64("line\nbreak")),
        format!("state=done:msg={}", b64("tab\there")),
        format!("state=done:msg={}", b64("del\u{7f}")),
        format!("state=done:msg={}", b64("c1\u{85}")),
        format!(
            "state=done:msg={}",
            base64::engine::general_purpose::STANDARD.encode([0xff, 0xfe])
        ),
        format!("state=done:msg={}", b64(&too_long)),
        format!("state=done:title={}", b64(&title_long)),
    ] {
        assert_eq!(body(&bad), None, "{bad:?}");
    }
}

#[test]
fn a_body_past_the_sequence_limit_is_dropped() {
    let pad = |len: usize| format!("state=done:zz={}", "a".repeat(len - "state=done:zz=".len()));
    assert!(body(&pad(MAX_BODY)).is_some());
    assert_eq!(body(&pad(MAX_BODY + 1)), None);
}

#[test]
fn a_report_replaces_its_record_and_the_summary_takes_the_highest() {
    let mut table = RecordTable::default();
    assert_eq!(table.summary(), None);
    table.apply(String::new(), Some(ProgramState::Working));
    table.apply("sub".into(), Some(ProgramState::Done));
    assert_eq!(table.summary(), Some(ProgramState::Working));
    table.apply("sub".into(), Some(ProgramState::Blocked));
    assert_eq!(table.summary(), Some(ProgramState::Blocked));
    table.apply("sub".into(), Some(ProgramState::Idle));
    table.apply(String::new(), Some(ProgramState::Error));
    assert_eq!(table.summary(), Some(ProgramState::Error));
    // Priority: blocked > error > working > done > idle.
    let mut order = [
        ProgramState::Idle,
        ProgramState::Blocked,
        ProgramState::Done,
        ProgramState::Error,
        ProgramState::Working,
    ];
    order.sort();
    assert_eq!(
        order,
        [
            ProgramState::Idle,
            ProgramState::Done,
            ProgramState::Working,
            ProgramState::Error,
            ProgramState::Blocked,
        ]
    );
}

#[test]
fn clear_removes_the_record_and_its_subtree_only() {
    let mut table = RecordTable::default();
    for id in ["a", "a/b", "a/b/c", "ab"] {
        table.apply(id.into(), Some(ProgramState::Working));
    }
    table.apply("ab".into(), Some(ProgramState::Done));
    table.apply("a".into(), None);
    assert_eq!(
        table.summary(),
        Some(ProgramState::Done),
        "only `ab` is left"
    );
    table.apply(String::new(), Some(ProgramState::Blocked));
    table.apply(String::new(), None);
    assert_eq!(table.summary(), None, "a bare clear empties the table");
}

#[test]
fn the_least_recently_updated_record_is_evicted_at_the_cap() {
    let mut table = RecordTable::default();
    table.apply("first".into(), Some(ProgramState::Blocked));
    for index in 1..MAX_RECORDS {
        table.apply(format!("r{index}"), Some(ProgramState::Idle));
    }
    // Touching `first` makes `r1` the oldest.
    table.apply("first".into(), Some(ProgramState::Blocked));
    table.apply("newcomer".into(), Some(ProgramState::Idle));
    assert_eq!(table.records.len(), MAX_RECORDS);
    assert!(table.records.contains_key("first"));
    assert!(!table.records.contains_key("r1"));
    // Replacing an existing id at the cap evicts nothing.
    table.apply("r2".into(), Some(ProgramState::Done));
    assert_eq!(table.records.len(), MAX_RECORDS);
}

#[test]
fn a_new_prompt_keeps_only_done_and_error() {
    let mut table = RecordTable::default();
    for (id, state) in [
        ("w", ProgramState::Working),
        ("b", ProgramState::Blocked),
        ("i", ProgramState::Idle),
        ("d", ProgramState::Done),
        ("e", ProgramState::Error),
    ] {
        table.apply(id.into(), Some(state));
    }
    table.prompt_started();
    let mut kept: Vec<_> = table.records.keys().cloned().collect();
    kept.sort();
    assert_eq!(kept, ["d", "e"]);
    assert_eq!(table.summary(), Some(ProgramState::Error));
}
