#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;

use inklkunn_store::{AuditEvent, AuditOutcome, AuditSink};

use super::*;

fn env(dir: &std::path::Path) -> impl Fn(&str) -> Option<OsString> + use<> {
    let map: HashMap<String, OsString> =
        HashMap::from([("INKLKUNN_DATA_DIR".to_owned(), dir.as_os_str().to_owned())]);
    move |k| map.get(k).cloned()
}

#[test]
fn the_trail_is_exported_oldest_first_one_object_a_line_and_without_any_secret() {
    let dir = std::env::temp_dir().join(format!("inklkunn-audit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    {
        let store = SqliteStore::open(&dir.join("store.sqlite")).unwrap();
        for n in 0..3 {
            store
                .record(&AuditEvent::new(
                    n,
                    "system:operator-cli",
                    "keys-rotate",
                    "wiki:w",
                    AuditOutcome::Allowed,
                    &format!("generation {n}"),
                ))
                .unwrap();
        }
    }
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(export(&env(&dir), &mut out, &mut err), 0);
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    for (i, line) in lines.iter().enumerate() {
        // The keys are fixed and in this order (RFC 002 D-9): a reader that parses it needs no schema.
        let start = format!(
            "{{\"seq\":{},\"at\":{i},\"principal\":\"system:operator-cli\",\"action\":\"keys-rotate\",",
            i + 1
        );
        assert!(line.starts_with(&start), "{line}");
        let end = format!("\"outcome\":\"allowed\",\"detail\":\"generation {i}\"}}");
        assert!(line.ends_with(&end), "{line}");
    }
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("exported 3 audit events")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_store_or_a_data_directory_it_says_so_and_creates_nothing() {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(export(&|_| None, &mut out, &mut err), 1);
    assert!(
        String::from_utf8(err.clone())
            .unwrap()
            .contains("INKLKUNN_DATA_DIR")
    );
    let dir = std::env::temp_dir().join(format!("inklkunn-audit-none-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    err.clear();
    assert_eq!(export(&env(&dir), &mut out, &mut err), 1);
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("no audit trail to export")
    );
    assert!(out.is_empty());
    assert!(!dir.exists(), "reading the trail never makes a store");
}
