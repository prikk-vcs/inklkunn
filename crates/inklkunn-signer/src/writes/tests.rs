#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use super::*;

#[test]
fn a_path_into_a_seed_directory_never_reaches_an_error() {
    let said = "error: author signing is required: no seed at /dev/shm/inklkunn/inklkunn-seeds-0123456789abcdef0123456789abcdef/author.seed. Create one with `prikk key generate --out /dev/shm/inklkunn/inklkunn-seeds-ffffffffffffffffffffffffffffffff/x.seed` first";
    let clean = scrub(said);
    assert!(
        !clean.contains("inklkunn-seeds-") && !clean.contains("/dev/shm"),
        "{clean}"
    );
    assert!(clean.contains("no seed at <seed directory>."), "{clean}");
    assert!(
        clean.contains("--out <seed directory>` first"),
        "the rest of the sentence stays: {clean}"
    );
    // Nothing to scrub, nothing changes; and a quoted path ends at the quote.
    assert_eq!(
        scrub("error: precondition not met"),
        "error: precondition not met"
    );
    assert_eq!(
        scrub("cannot read '/a/inklkunn-seeds-ab/x' now"),
        "cannot read '<seed directory>' now"
    );
    assert_eq!(scrub("inklkunn-seeds-ab"), "<seed directory>");
    assert_eq!(scrub(""), "");
}

#[test]
fn a_refused_write_carries_prikks_words_without_the_seed_path() {
    // Exercise `outcome` itself with a failed run whose stderr names a seed file.
    let run = Run {
        status: std::process::Command::new("false")
            .status()
            .expect("false runs"),
        stdout: Vec::new(),
        stderr: b"error: no seed at /run/inklkunn-seeds-00ff/author.seed\n".to_vec(),
        timed_out: false,
        output_exceeded: false,
    };
    let e = outcome(no_binary(), "commit", run, 1 << 30).unwrap_err();
    let shown = e.to_string();
    assert!(
        !shown.contains("inklkunn-seeds-") && !shown.contains("/run/"),
        "{shown}"
    );
    assert!(shown.contains("<seed directory>"), "{shown}");
}

/// A binary that is not there: a call that is not a format error never asks for the version.
fn no_binary() -> &'static std::ffi::OsStr {
    std::ffi::OsStr::new("/nonexistent/prikk")
}

fn run_of(wait_status: i32, stderr: &str) -> Run {
    use std::os::unix::process::ExitStatusExt;
    Run {
        status: std::process::ExitStatus::from_raw(wait_status),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
        output_exceeded: false,
    }
}

#[test]
fn a_call_that_died_of_its_memory_ceiling_is_the_ceiling_and_never_a_refusal() {
    // The shapes the driver already knows (PK-9), through the one classifier: an allocator abort, a segfault,
    // a loader failure, bubblewrap running out of memory, a signal reported as 128 + n.
    for (what, run) in [
        (
            "an allocator abort",
            run_of(6, "memory allocation of 629145648 bytes failed"),
        ),
        ("a segfault", run_of(11, "")),
        ("a signal under bwrap", run_of(139 << 8, "")),
        (
            "a loader failure",
            run_of(
                127 << 8,
                "error while loading shared libraries: cannot allocate TLS",
            ),
        ),
        ("bwrap out of memory", run_of(1 << 8, "Out of memory")),
    ] {
        match outcome(no_binary(), "verify", run, 1 << 30) {
            Err(SignerError::CeilingExceeded { verb, bytes }) => {
                assert_eq!((verb.as_str(), bytes), ("verify", 1 << 30), "{what}");
            }
            other => panic!("{what}: {other:?}"),
        }
    }
    // A refusal is still a refusal, and the same text is not mistaken for a ceiling when the run succeeded.
    assert!(matches!(
        outcome(
            no_binary(),
            "commit",
            run_of(1 << 8, "error: precondition not met: x"),
            1 << 30
        ),
        Err(SignerError::Refused(_))
    ));
    assert!(
        outcome(
            no_binary(),
            "commit",
            run_of(0, "memory allocation of 8 bytes failed"),
            1 << 30
        )
        .is_ok()
    );
    // Its words tell an operator it is neither refused nor failed.
    let shown = SignerError::CeilingExceeded {
        verb: "seal".into(),
        bytes: 1,
    }
    .to_string();
    assert!(
        shown.contains("memory ceiling") && shown.contains("not refused"),
        "{shown}"
    );
}

#[test]
fn only_the_whole_resource_message_of_a_failed_stage_is_a_ceiling_hit() {
    let report = |status: &str, message: &str| {
        serde_json::json!({
            "schema_version": "verify-report-v1",
            "verdict": {"ok": false, "failed_conditions": []},
            "stages": [
                {"stage": "objects", "status": status, "message": message},
                {"stage": "refs", "status": "evaluated"}
            ]
        })
        .to_string()
        .into_bytes()
    };
    // The measured shape, and Rust's text for a raw ENOMEM.
    assert!(resource_failure_in(&report(
        "failed",
        "i/o error: out of memory"
    )));
    assert!(resource_failure_in(&report(
        "failed",
        " i/o error: Cannot allocate memory (os error 12) "
    )));
    // Damage that merely mentions the words, a different status, no message, a different prefix, no report.
    for (what, bytes) in [
        (
            "a page named like the failure",
            report(
                "failed",
                "integrity error: pages/out of memory.md is damaged",
            ),
        ),
        (
            "the words inside another i/o error",
            report("failed", "i/o error: out of memory for pages/x.md"),
        ),
        (
            "a stage that was not run",
            report("not_evaluated", "i/o error: out of memory"),
        ),
        (
            "a different prefix",
            report("failed", "integrity error: out of memory"),
        ),
        ("not json", b"error: out of memory".to_vec()),
        ("an empty report", b"{}".to_vec()),
    ] {
        assert!(!resource_failure_in(&bytes), "{what}");
    }
}

#[test]
fn the_queue_is_read_from_the_plain_text_of_status_strictly() {
    let text = |records: &str, torn: &str, queue: &str| {
        format!(
            "prikk repository: /x/.prikk\nactive WAL records: {records}\ntrailing partial WAL bytes: {torn}\nheads/main RefState: abc\ncurrent branch: heads/main\nqueued patches: {queue}\nstatus: multi-operation text diff minimization and plugins not yet implemented\n"
        )
    };
    assert_eq!(
        parse_status_text(&text("1", "0", "1 targeting heads/main")),
        Ok(StatusText {
            wal_records: 1,
            torn_bytes: 0,
            queued: 1,
            target: Some("heads/main".to_owned())
        })
    );
    assert_eq!(
        parse_status_text(&text("0", "0", "0")),
        Ok(StatusText {
            wal_records: 0,
            torn_bytes: 0,
            queued: 0,
            target: None
        })
    );
    for (what, bad) in [
        (
            "a count that is not a number",
            text("one", "0", "1 targeting heads/main"),
        ),
        (
            "a queue of words",
            text("1", "0", "some targeting heads/main"),
        ),
        ("a torn count that is not a number", text("1", "-1", "0")),
        (
            "no queue line",
            "active WAL records: 1\ntrailing partial WAL bytes: 0\n".to_owned(),
        ),
        (
            "no log line",
            "trailing partial WAL bytes: 0\nqueued patches: 0\n".to_owned(),
        ),
        (
            "the queue twice",
            format!(
                "{}queued patches: 0\n",
                text("1", "0", "1 targeting heads/main")
            ),
        ),
        ("nothing", String::new()),
    ] {
        assert!(parse_status_text(&bad).is_err(), "{what}");
    }
}

#[test]
fn the_trust_list_is_read_strictly() {
    // W3, W5: `prikk trust maintainer list --format json` (measured on prikk 0.55.0). The ids and the public keys come
    // from it; a list that is not exactly that shape is refused and nothing is concluded from it.
    let key = |id: &str, c: char| {
        format!(
            r#"{{"key_id": "{id}", "public_key": "{}"}}"#,
            c.to_string().repeat(64)
        )
    };
    let list = |schema: &str, keys: &[String]| {
        format!(
            r#"{{"schema_version": "{schema}", "keys": [{}]}}"#,
            keys.join(", ")
        )
    };
    let good = parse_trust_list(list("trust-list-v1", &[key("a", '1'), key("b", '2')]).as_bytes())
        .unwrap();
    assert_eq!(
        good,
        [
            TrustedKey {
                key_id: "a".to_owned(),
                public_key: "1".repeat(64)
            },
            TrustedKey {
                key_id: "b".to_owned(),
                public_key: "2".repeat(64)
            }
        ]
    );
    assert!(
        parse_trust_list(list("trust-list-v1", &[]).as_bytes())
            .unwrap()
            .is_empty()
    );
    for (what, bad) in [
        ("another schema", list("trust-list-v2", &[key("a", '1')])),
        (
            "a duplicated id",
            list("trust-list-v1", &[key("a", '1'), key("a", '2')]),
        ),
        ("an empty id", list("trust-list-v1", &[key("", '1')])),
        (
            "a public key in capitals",
            list("trust-list-v1", &[key("a", 'A')]),
        ),
        (
            "a short public key",
            r#"{"schema_version": "trust-list-v1", "keys": [{"key_id": "a", "public_key": "abc"}]}"#
                .to_owned(),
        ),
        (
            "no public key",
            r#"{"schema_version": "trust-list-v1", "keys": [{"key_id": "a"}]}"#.to_owned(),
        ),
        (
            "no keys",
            r#"{"schema_version": "trust-list-v1"}"#.to_owned(),
        ),
        (
            "not a report",
            "adopted maintainer keys, in adoption order:".to_owned(),
        ),
    ] {
        assert!(
            matches!(
                parse_trust_list(bad.as_bytes()),
                Err(SignerError::Damaged(_))
            ),
            "{what}"
        );
    }
}

// ---- who sealed which block: `block_seals` where prikk has it (0.57.0, PK-39), the text where it does not

const BLOCK_A: &str = "ab";
const BLOCK_B: &str = "cd";

fn id(half: &str) -> String {
    half.repeat(32)
}

#[test]
fn block_seals_are_read_strictly_and_a_report_without_them_is_an_older_prikk() {
    let report =
        |seals: &str| format!(r#"{{"schema_version":"verify-report-v1","block_seals":{seals}}}"#);
    let one = |block: &str, keys: &str| format!(r#"{{"block_id":"{block}","sealed_by":{keys}}}"#);
    // The field is absent: an older prikk, not an empty list.
    assert_eq!(
        parse_block_seals(br#"{"schema_version":"verify-report-v1"}"#),
        Ok(None)
    );
    // One pair per key a block's seal records; a block with none yields none.
    let good = report(&format!(
        "[{},{},{}]",
        one(&id(BLOCK_A), r#"["m1","m-2_x"]"#),
        one(&id(BLOCK_B), r#"["m1"]"#),
        one(&id("ef"), "[]")
    ));
    assert_eq!(
        parse_block_seals(good.as_bytes()),
        Ok(Some(vec![
            (id(BLOCK_A), "m1".to_owned()),
            (id(BLOCK_A), "m-2_x".to_owned()),
            (id(BLOCK_B), "m1".to_owned()),
        ]))
    );
    assert_eq!(parse_block_seals(report("[]").as_bytes()), Ok(Some(vec![])));
    for (what, bad) in [
        ("not a list", report(r#"{"a":1}"#)),
        ("not JSON", "block_seals".to_owned()),
        ("no block id", report(r#"[{"sealed_by":["m1"]}]"#)),
        (
            "a short block id",
            report(&format!("[{}]", one("abcd", r#"["m1"]"#))),
        ),
        (
            "an upper-case block id",
            report(&format!("[{}]", one(&id("AB"), r#"["m1"]"#))),
        ),
        (
            "no sealed_by",
            report(&format!(r#"[{{"block_id":"{}"}}]"#, id(BLOCK_A))),
        ),
        (
            "a key id with a space",
            report(&format!("[{}]", one(&id(BLOCK_A), r#"["bad key"]"#))),
        ),
        (
            "an empty key id",
            report(&format!("[{}]", one(&id(BLOCK_A), r#"[""]"#))),
        ),
        (
            "a key id that is not text",
            report(&format!("[{}]", one(&id(BLOCK_A), "[1]"))),
        ),
        (
            "a block twice",
            report(&format!(
                "[{},{}]",
                one(&id(BLOCK_A), r#"["m1"]"#),
                one(&id(BLOCK_A), r#"["m2"]"#)
            )),
        ),
    ] {
        assert!(parse_block_seals(bad.as_bytes()).is_err(), "{what}");
    }
}

/// A repository directory and a `prikk` that is a script: `verify --format json` prints `json`, `verify` prints
/// `text`, and every call is counted in `calls`.
struct SealsFixture {
    dir: PathBuf,
}

impl SealsFixture {
    fn new(tag: &str, json: &str, text: &str, json_exit: i32) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("inklkunn-seals-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo/.prikk")).unwrap();
        std::fs::write(dir.join("report.json"), json).unwrap();
        std::fs::write(dir.join("report.txt"), text).unwrap();
        let script = format!(
            "#!/bin/sh\nD='{d}'\necho \"$*\" >> \"$D/calls\"\n\
             if [ \"$1 $2 $3\" = 'verify --format json' ]; then cat \"$D/report.json\"; exit {json_exit}; fi\n\
             if [ \"$1\" = verify ]; then cat \"$D/report.txt\"; exit 0; fi\nexit 2\n",
            d = dir.display()
        );
        let path = dir.join("prikk");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        SealsFixture { dir }
    }

    fn signers(&self) -> Result<Sealers, SignerError> {
        let writes = Writes::new(self.dir.join("prikk"), Sandbox::Unconfined);
        // A script just written can be refused with `ETXTBSY` while a sibling test forks.
        let mut r = writes.sealed_block_signers(&self.dir.join("repo"));
        for _ in 0..50 {
            if !matches!(&r, Err(SignerError::Unavailable(m)) if m.contains("Text file busy")) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            r = writes.sealed_block_signers(&self.dir.join("repo"));
        }
        r
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default()
    }
}

impl Drop for SealsFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const EVALUATED: &str = r#"[{"stage":"objects","status":"evaluated"}]"#;

fn verify_json(extra: &str, ok: bool, stages: &str) -> String {
    let verdict = if ok {
        r#"{"ok":true,"failed_conditions":[]}"#
    } else {
        r#"{"ok":false,"failed_conditions":[{"id":"publication-trust","message":"x"}]}"#
    };
    format!(
        r#"{{"schema_version":"verify-report-v1","verdict":{verdict},"stages":{stages},"failed_objects":[]{extra}}}"#
    )
}

#[test]
fn a_prikk_that_reports_block_seals_is_read_there_and_its_text_is_never_asked() {
    let seals = format!(
        r#","block_seals":[{{"block_id":"{}","sealed_by":["old-key"],"trusted_by":null}}]"#,
        id(BLOCK_A)
    );
    // The report of a **failing** verification (a retired key): the text lists no sealed block then, the report does.
    let f = SealsFixture::new(
        "report",
        &verify_json(&seals, false, EVALUATED),
        &format!("sealed-block {}: a-different-key\n", id(BLOCK_B)),
        1,
    );
    assert_eq!(
        f.signers(),
        Ok(Sealers {
            pairs: vec![(id(BLOCK_A), "old-key".to_owned())],
            as_recorded: true
        })
    );
    assert_eq!(
        f.calls(),
        "verify --format json\n",
        "the text was not asked"
    );
}

#[test]
fn a_prikk_whose_report_has_no_block_seals_is_read_in_the_text() {
    let f = SealsFixture::new(
        "text",
        &verify_json("", true, EVALUATED),
        &format!(
            "verified repository: /x\nsealed-block {}: m1\n",
            id(BLOCK_B)
        ),
        0,
    );
    assert_eq!(
        f.signers(),
        Ok(Sealers {
            pairs: vec![(id(BLOCK_B), "m1".to_owned())],
            as_recorded: false
        })
    );
    assert_eq!(f.calls(), "verify --format json\nverify\n");
}

#[test]
fn a_report_that_is_not_complete_or_not_well_formed_gives_no_list() {
    let seals = format!(
        r#","block_seals":[{{"block_id":"{}","sealed_by":["old-key"]}}]"#,
        id(BLOCK_A)
    );
    // A stage that halted: the list may be short, and a short list would read as "nothing sealed late".
    let halted = SealsFixture::new(
        "halted",
        &verify_json(&seals, false, r#"[{"stage":"objects","status":"halted"}]"#),
        "",
        1,
    );
    assert!(
        matches!(halted.signers(), Err(SignerError::Unavailable(why)) if why.contains("did not run to the end")),
        "{:?}",
        halted.signers()
    );
    // A list in a shape nobody knows is damage, not "no seals".
    let malformed = SealsFixture::new(
        "malformed",
        &verify_json(r#","block_seals":"none""#, true, EVALUATED),
        &format!("sealed-block {}: m1\n", id(BLOCK_A)),
        0,
    );
    assert!(
        matches!(malformed.signers(), Err(SignerError::Damaged(_))),
        "{:?}",
        malformed.signers()
    );
    // A stage that failed for want of memory is the ceiling, not a list.
    let memory = SealsFixture::new(
        "memory",
        &verify_json(
            &seals,
            false,
            r#"[{"stage":"objects","status":"failed","message":"i/o error: out of memory"}]"#,
        ),
        "",
        1,
    );
    assert!(
        matches!(memory.signers(), Err(SignerError::CeilingExceeded { .. })),
        "{:?}",
        memory.signers()
    );
}

/// A failing report whose verdict names `conditions` (condition ids) over `stages`.
fn verify_json_failing(extra: &str, conditions: &[&str], stages: &str) -> String {
    let conditions: Vec<String> = conditions
        .iter()
        .map(|id| format!(r#"{{"id":"{id}","message":"x"}}"#))
        .collect();
    format!(
        r#"{{"schema_version":"verify-report-v1","verdict":{{"ok":false,"failed_conditions":[{}]}},"stages":{stages},"failed_objects":[]{extra}}}"#,
        conditions.join(",")
    )
}

#[test]
fn a_list_that_prikk_may_have_left_short_is_never_read_as_no_late_seals() {
    // Measured on the published 0.57.0 (m3-measurements, section 8): with a block or a ref container that cannot be read,
    // `block_seals` is short or empty while every stage is `evaluated`; a failed stage leaves it empty. The list is read
    // only where it is whole (a verification that holds, or fails on publication trust alone); anywhere else it is
    // *unavailable*, so that `sees_untrusted_keys` is never true over a list that may leave a block out.
    let seals = format!(
        r#","block_seals":[{{"block_id":"{}","sealed_by":["old-key"],"trusted_by":null}}]"#,
        id(BLOCK_A)
    );
    const BLOCKED: &str = r#"[{"stage":"refs","status":"failed","message":"integrity error: x"},{"stage":"later","status":"not_evaluated","blocked_by":"refs"}]"#;
    // prikk's own rule (reply of 2026-11-14, PK-39): a damaged Block is missing from the list and named in
    // `failed_objects`, whatever the verdict says of it.
    let named = verify_json_failing(&seals, &["publication-trust"], EVALUATED).replace(
        r#""failed_objects":[]"#,
        r#""failed_objects":[{"object_type":"block","class":"corrupt"}]"#,
    );
    let shapes: [(&str, String); 6] = [
        ("damaged-block-named", named),
        (
            "unreadable-container",
            verify_json_failing(&seals, &["item-failure"], EVALUATED),
        ),
        (
            "failed-stage",
            verify_json_failing(&seals, &["stage-failure", "item-failure"], BLOCKED),
        ),
        (
            "trust-and-container",
            verify_json_failing(&seals, &["publication-trust", "item-failure"], EVALUATED),
        ),
        (
            "unknown-condition",
            verify_json_failing(&seals, &["a-newer-condition"], EVALUATED),
        ),
        (
            "empty-list",
            verify_json_failing(r#","block_seals":[]"#, &["item-failure"], EVALUATED),
        ),
    ];
    for (tag, json) in shapes {
        let f = SealsFixture::new(
            tag,
            &json,
            &format!("sealed-block {}: m1\n", id(BLOCK_B)),
            1,
        );
        let r = f.signers();
        assert!(
            matches!(&r, Err(SignerError::Unavailable(why)) if why.contains("may be incomplete")),
            "{tag}: {r:?}"
        );
        assert_eq!(
            f.calls(),
            "verify --format json\n",
            "{tag}: the text, which lists less, is no fallback"
        );
    }
    // A retired key fails the verdict on publication trust alone, and its list is whole: that is the case the list is for.
    let retired = SealsFixture::new(
        "retired",
        &verify_json_failing(&seals, &["publication-trust"], EVALUATED),
        "",
        1,
    );
    assert_eq!(
        retired.signers(),
        Ok(Sealers {
            pairs: vec![(id(BLOCK_A), "old-key".to_owned())],
            as_recorded: true
        })
    );
    // An older prikk's report (no `block_seals`) that fails the same way is read in the text, as before M8: the text
    // claims no more than "the keys prikk still trusts" (`as_recorded: false`).
    let older = SealsFixture::new(
        "older",
        &verify_json_failing("", &["item-failure"], EVALUATED),
        &format!("sealed-block {}: m1\n", id(BLOCK_B)),
        1,
    );
    assert_eq!(
        older.signers(),
        Ok(Sealers {
            pairs: vec![(id(BLOCK_B), "m1".to_owned())],
            as_recorded: false
        })
    );
}

// ---- `prikk uncommit` (0.57.0, PK-43): a stand-in prikk for the three things it can answer

struct UncommitFixture {
    dir: PathBuf,
}

const PATCH: &str = "3aa5ec44bf476bc227a0ea4f8d55a4dc6d507819dd9c23d625cb555b83fe8156";

impl UncommitFixture {
    /// A `prikk` whose `uncommit <patch> --plan-only` prints `plan` to stdout and `plan_err` to stderr and exits with
    /// `plan_exit`, and whose `uncommit <patch>` prints `done`; every call is appended to `calls`.
    fn new(tag: &str, plan: &str, plan_err: &str, plan_exit: i32, done: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("inklkunn-uncommit-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo/.prikk")).unwrap();
        std::fs::write(dir.join("plan.txt"), plan).unwrap();
        std::fs::write(dir.join("plan.err"), plan_err).unwrap();
        std::fs::write(dir.join("done.txt"), done).unwrap();
        let script = format!(
            "#!/bin/sh\nD='{d}'\necho \"$*\" >> \"$D/calls\"\n\
             if [ \"$1\" = uncommit ] && [ \"$3\" = --plan-only ]; then cat \"$D/plan.txt\"; cat \"$D/plan.err\" >&2; exit {plan_exit}; fi\n\
             if [ \"$1\" = uncommit ]; then cat \"$D/done.txt\"; exit 0; fi\nexit 2\n",
            d = dir.display()
        );
        let path = dir.join("prikk");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        UncommitFixture { dir }
    }

    fn writes(&self) -> Writes {
        Writes::new(self.dir.join("prikk"), Sandbox::Unconfined)
    }

    fn repo(&self) -> PathBuf {
        self.dir.join("repo")
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.dir.join("calls")).unwrap_or_default()
    }

    /// Run `f` again while a script just written is refused with `ETXTBSY` (a sibling test forks).
    fn retry<T>(&self, mut f: impl FnMut() -> Result<T, SignerError>) -> Result<T, SignerError> {
        let mut r = f();
        for _ in 0..50 {
            if !matches!(&r, Err(SignerError::Unavailable(m)) if m.contains("Text file busy")) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            r = f();
        }
        r
    }
}

impl Drop for UncommitFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const PLAN_TEXT: &str = "commit 3aa5ec44bf476bc227a0ea4f8d55a4dc6d507819dd9c23d625cb555b83fe8156 (sequence 1 on heads/main)\nmessage: m\n  edit-text pages/Home.md\nplan only -- nothing written\n";

#[test]
fn a_prikk_that_has_no_uncommit_is_an_answer_and_one_that_refuses_is_a_refusal() {
    // Before 0.57.0: exit 2, exactly this text. Not an error: the signer asks, and the answer is "no such verb".
    let none = UncommitFixture::new("none", "", "error: unknown command: uncommit\n", 2, "");
    assert_eq!(
        none.retry(|| none.writes().uncommit_plan(&none.repo(), PATCH)),
        Ok(None)
    );
    // 0.57.0 refusing for a reason of its own (the patch is not the newest, say) is a refusal with prikk's words.
    let refused = UncommitFixture::new(
        "refused",
        "",
        "error: precondition not met: patch X has others queued after it\n",
        1,
        "",
    );
    let r = refused.retry(|| refused.writes().uncommit_plan(&refused.repo(), PATCH));
    assert!(
        matches!(&r, Err(SignerError::Refused(why)) if why.contains("others queued after it")),
        "{r:?}"
    );
    // The same exit 2 with other words is not "no such verb": a usage error is not a missing capability.
    let usage = UncommitFixture::new(
        "usage",
        "",
        "error: uncommit: \"x\" is not a patch id\n",
        2,
        "",
    );
    let r = usage.retry(|| usage.writes().uncommit_plan(&usage.repo(), PATCH));
    assert!(!matches!(r, Ok(None)), "{r:?}");
}

#[test]
fn the_plan_of_uncommit_is_for_the_patch_asked_about_and_only_a_patch_id_is_ever_passed() {
    let ok = UncommitFixture::new("plan", PLAN_TEXT, "", 0, "");
    let plan = ok
        .retry(|| ok.writes().uncommit_plan(&ok.repo(), PATCH))
        .unwrap()
        .expect("a plan");
    assert_eq!(
        (plan.patch_id.as_str(), plan.target.as_str()),
        (PATCH, "heads/main")
    );
    assert_eq!(ok.calls(), format!("uncommit {PATCH} --plan-only\n"));
    // A plan for another patch is damage, not an offer.
    let other = "ab".repeat(32);
    let r = ok.writes().uncommit_plan(&ok.repo(), &other);
    assert!(matches!(r, Err(SignerError::Damaged(_))), "{r:?}");
    // Anything that is not a patch id never reaches an argument vector.
    let calls = ok.calls();
    for bad in ["", "abc", "--help", &PATCH.to_uppercase(), "x; rm -rf /"] {
        assert!(
            matches!(
                ok.writes().uncommit_plan(&ok.repo(), bad),
                Err(SignerError::Refused(_))
            ),
            "{bad}"
        );
        assert!(
            matches!(
                ok.writes().uncommit(&ok.repo(), bad),
                Err(SignerError::Refused(_))
            ),
            "{bad}"
        );
    }
    assert!(!ok.calls().contains("rm -rf"), "{}", ok.calls());
    assert_eq!(
        ok.calls().lines().count(),
        calls.lines().count(),
        "none of those ran prikk"
    );
}

#[test]
fn a_take_back_is_done_only_when_prikk_says_where_to_undo_it() {
    let said = "the queue is empty\ncommit taken back; undo with `prikk doctor --recovery-restore 4422e5ef2a44a122` (it refuses once anything has changed since)\n";
    let ok = UncommitFixture::new("done", PLAN_TEXT, "", 0, said);
    let run = ok.retry(|| ok.writes().uncommit(&ok.repo(), PATCH));
    assert_eq!(run, Ok("4422e5ef2a44a122".to_owned()));
    // Exit 0 with anything else is not "taken back".
    let silent = UncommitFixture::new("silent", PLAN_TEXT, "", 0, "all done\n");
    let r = silent.retry(|| silent.writes().uncommit(&silent.repo(), PATCH));
    assert!(matches!(r, Err(SignerError::Damaged(_))), "{r:?}");
}

/// A stand-in `prikk` that answers `--version` with `version` (and nothing else), in its own directory.
struct VersionOnly {
    dir: PathBuf,
}

impl VersionOnly {
    fn new(tag: &str, version: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("inklkunn-version-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prikk");
        std::fs::write(
            &path,
            format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'prikk {version}'; exit 0; fi\nexit 2\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        // A script just written can be refused with `ETXTBSY` while a sibling test forks.
        for _ in 0..100 {
            if inklkunn_prikk::probe_version(path.as_os_str()).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        VersionOnly { dir }
    }

    fn binary(&self) -> std::ffi::OsString {
        self.dir.join("prikk").into_os_string()
    }
}

impl Drop for VersionOnly {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_repository_of_a_format_prikk_does_not_read_is_the_operators_mismatch_and_never_a_refusal_or_a_recovery()
 {
    // Measured: prikk 0.55.0 and 0.56.0 on a repository made by 0.57.0 (format 8), for every command:
    // `error: unsupported format version: 8`, exit 1.
    for version in ["0.55.0", "0.56.0"] {
        let old = VersionOnly::new(&format!("old-{version}"), version);
        let e = outcome(
            &old.binary(),
            "commit",
            run_of(1 << 8, "error: unsupported format version: 8\n"),
            1 << 30,
        )
        .unwrap_err();
        assert!(
            matches!(e, SignerError::UnsupportedFormat { version: 8 }),
            "{version}: {e:?}"
        );
        let shown = e.to_string();
        assert!(
            shown.contains("format 8") && shown.contains("nothing was done"),
            "{shown}"
        );
        assert!(!shown.contains("recover"), "no case for recovery: {shown}");
    }
    // Any other sentence is a refusal as before, whatever it names.
    for other in [
        "error: page \"unsupported format version: 8\" is not there\n",
        "error: unsupported format version: 8 (and more)\n",
        "error: unsupported format version: x\n",
    ] {
        let e = outcome(no_binary(), "commit", run_of(1 << 8, other), 1 << 30).unwrap_err();
        assert!(matches!(e, SignerError::Refused(_)), "{other}: {e:?}");
    }
}

#[test]
fn the_same_sentence_from_a_prikk_that_reads_the_format_is_a_damaged_or_unknown_marker() {
    // M17 (finding R-a): `.prikk/FORMAT` written as `8` without its newline makes prikk 0.57.0 itself say
    // `unsupported format version: 8`; `9` is a number no table knows. Neither is "nothing is damaged".
    let new = VersionOnly::new("new", "0.57.0");
    let old = VersionOnly::new("old", "0.55.0");
    let say = |binary: &std::ffi::OsStr, said: &str| {
        outcome(binary, "commit", run_of(1 << 8, said), 1 << 30).unwrap_err()
    };
    let e = say(&new.binary(), "error: unsupported format version: 8\n");
    assert!(
        matches!(e, SignerError::FormatMarker { version: Some(8) }),
        "{e:?}"
    );
    let e = say(&new.binary(), "error: unsupported format version: 9\n");
    assert!(
        matches!(e, SignerError::FormatMarker { version: Some(9) }),
        "{e:?}"
    );
    let shown = e.to_string();
    assert!(
        shown.contains("format 9") && !shown.contains('8'),
        "{shown}"
    );
    assert!(shown.contains(".prikk/FORMAT"), "{shown}");
    assert!(
        !shown.contains("nothing in the repository is damaged"),
        "{shown}"
    );
    // A number the table does not know is a marker on an old prikk too; a binary that does not say its version
    // is never taken to refuse anything.
    let e = say(&old.binary(), "error: unsupported format version: 9\n");
    assert!(
        matches!(e, SignerError::FormatMarker { version: Some(9) }),
        "{e:?}"
    );
    let e = say(no_binary(), "error: unsupported format version: 8\n");
    assert!(
        matches!(e, SignerError::FormatMarker { version: Some(8) }),
        "{e:?}"
    );
    // The driver's conditions: exit 1 (not 2), no report on stdout, the number without a leading zero.
    for (what, run) in [
        (
            "exit 2",
            run_of(2 << 8, "error: unsupported format version: 8\n"),
        ),
        (
            "a leading zero",
            run_of(1 << 8, "error: unsupported format version: 08\n"),
        ),
        (
            "a signal",
            run_of(9, "error: unsupported format version: 8\n"),
        ),
    ] {
        let e = outcome(&new.binary(), "commit", run, 1 << 30).unwrap_err();
        assert!(
            !matches!(
                e,
                SignerError::FormatMarker { .. } | SignerError::UnsupportedFormat { .. }
            ),
            "{what}: {e:?}"
        );
    }
    let mut with_report = run_of(1 << 8, "error: unsupported format version: 8\n");
    with_report.stdout = br#"{"schema_version":"refusal-v1","refusal":"x","reason":null,"message":"unsupported format version: 8"}"#.to_vec();
    let e = outcome(&new.binary(), "commit", with_report, 1 << 30).unwrap_err();
    assert!(
        !matches!(
            e,
            SignerError::FormatMarker { .. } | SignerError::UnsupportedFormat { .. }
        ),
        "a refusal-v1 report: {e:?}"
    );
}
