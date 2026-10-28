#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn an_event_keeps_one_bounded_line_whatever_it_was_given() {
    let e = AuditEvent::new(
        1,
        "account:x\ny",
        "ac\tcept",
        &format!("proposal:w:{}", "p".repeat(500)),
        AuditOutcome::Denied,
        "line\u{2028}break\u{85}",
    );
    for field in [&e.principal, &e.action, &e.resource, &e.detail] {
        assert!(
            !field
                .chars()
                .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}'),
            "{field:?}"
        );
        assert!(field.chars().count() <= FIELD_MAX_CHARS);
    }
    assert_eq!(e.principal, "account:x?y");
    assert_eq!(e.resource.chars().count(), FIELD_MAX_CHARS);
    // Ordinary text, in any script, is kept as it is.
    assert_eq!(clean_field("日本語 (にほんご)"), "日本語 (にほんご)");
}

#[test]
fn outcomes_round_trip() {
    for o in [
        AuditOutcome::Allowed,
        AuditOutcome::Denied,
        AuditOutcome::Failed,
    ] {
        assert_eq!(AuditOutcome::parse(o.as_str()), Some(o));
    }
    assert_eq!(AuditOutcome::parse("Allowed"), None);
}

#[test]
fn the_memory_sink_appends_in_order_and_has_no_other_operation() {
    let sink = MemoryAudit::new();
    assert!(sink.events().is_empty());
    for n in 0..3 {
        sink.record(&AuditEvent::new(
            n,
            "anonymous",
            "export",
            "wiki:w",
            AuditOutcome::Denied,
            "",
        ))
        .unwrap();
    }
    let at: Vec<i64> = sink.events().iter().map(|e| e.at).collect();
    assert_eq!(at, vec![0, 1, 2]);
}
