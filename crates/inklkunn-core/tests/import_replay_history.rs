//! What the replay does with each kind of record (RFC 007 D-2, D-3; handoff I-T3), with the real signer and a real prikk: a text writes the page,
//! a rename moves it, a removal removes it, a record whose text the source hid is not replayed and a rename or a removal of a page the replay
//! does not have moves and removes nothing, the source's facts (an author from `users.yaml`, a hidden author, flags) are the message's, the files
//! with no history come in as one change, a failure the process survives leaves nothing, a creation that is alive is not discarded, and the
//! participation counts do not count an import. Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/replay.rs"]
mod replay;

use std::path::Path;

use inklkunn_core::message::{Kind, Message, SourceAuthorKind};
use inklkunn_core::proposal::{Period, operator_metrics};
use inklkunn_core::{DiscardRefusal, NoSweepAudit, discard_unfinished_creation};
use inklkunn_store::{ActStore, MemoryStore, Slug, WikiStore};
use replay::{
    Dir, dir_in, fixture, leftovers, messages, plan_of, prikk_on_path, read_text, run, world,
};

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// One line of a history file.
fn record(
    rev: &str,
    parent: Option<&str>,
    kind: &str,
    at: &str,
    extra: serde_json::Value,
) -> String {
    let mut v = serde_json::json!({
        "rev": rev,
        "parent": parent,
        "type": kind,
        "at": at,
    });
    if let (Some(base), Some(more)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in more {
            base.insert(k.clone(), x.clone());
        }
    }
    v.to_string()
}

fn text(title: &str, id: &str, body: &str) -> String {
    format!("---\nid: {id}\ntitle: {title}\n---\n\n{body}\n")
}

/// A bundle with every kind of record: a page that is edited, renamed and edited again; one that is created, loses its text to the source's
/// moderation and is removed; one whose only text is hidden and which is then removed; the people of `users.yaml`.
fn synthetic(tag: &str) -> Dir {
    let d = dir_in(&std::env::temp_dir(), tag);
    put(
        &d.0,
        "wiki-bundle.yaml",
        "format: portable-wiki-bundle\nversion: '0.4'\nlicense: CC0-1.0\nsource:\n  name: Orchard Wiki\n  url: https://orchard.example.org/\n",
    );
    put(&d.0, "users.yaml", "- id: u2\n  name: Bo\n  kind: bot\n");
    put(&d.0, "pages/Beta.md", &text("Beta", "alpha", "v3"));
    let alpha = [
        record(
            "r1",
            None,
            "create",
            "2020-01-01T10:00:00Z",
            serde_json::json!({"author": {"id": "ada", "name": "Ada", "kind": "person"}, "summary": "first", "content": text("Alpha", "alpha", "v1")}),
        ),
        record(
            "r2",
            Some("r1"),
            "edit",
            "2020-01-02T10:00:00Z",
            serde_json::json!({"author": {"id": "u2"}, "minor": true, "content": text("Alpha", "alpha", "v2")}),
        ),
        record(
            "r3",
            Some("r2"),
            "rename",
            "2020-01-03T10:00:00Z",
            serde_json::json!({"author": {"name": "Ada"}, "from": "Alpha", "to": "Beta"}),
        ),
        record(
            "r4",
            Some("r3"),
            "edit",
            "2020-01-05T10:00:00Z",
            serde_json::json!({"author": {"name": "Hidden Person"}, "suppressed": ["author"], "automated": true, "summary": "tidy", "content": text("Beta", "alpha", "v3")}),
        ),
    ];
    put(&d.0, "history/alpha.jsonl", &(alpha.join("\n") + "\n"));
    let gamma = [
        record(
            "g1",
            None,
            "create",
            "2020-01-02T12:00:00Z",
            serde_json::json!({"author": {"name": "Cy"}, "content": text("Gamma", "gamma", "g"), "inklkunn": {"path": "pages/Gamma.md"}}),
        ),
        record(
            "g2",
            Some("g1"),
            "edit",
            "2020-01-03T12:00:00Z",
            serde_json::json!({"suppressed": ["content"], "inklkunn": {"path": "pages/Gamma.md"}}),
        ),
        record(
            "g3",
            Some("g2"),
            "delete",
            "2020-01-04T12:00:00Z",
            serde_json::json!({"author": {"name": "Cy"}, "inklkunn": {"path": "pages/Gamma.md"}}),
        ),
    ];
    put(&d.0, "history/gamma.jsonl", &(gamma.join("\n") + "\n"));
    let delta = [
        record(
            "d1",
            None,
            "create",
            "2020-01-01T09:00:00Z",
            serde_json::json!({"suppressed": ["content"], "inklkunn": {"path": "pages/Delta.md"}}),
        ),
        record(
            "d2",
            Some("d1"),
            "delete",
            "2020-01-06T09:00:00Z",
            serde_json::json!({"inklkunn": {"path": "pages/Delta.md"}}),
        ),
    ];
    put(&d.0, "history/delta.jsonl", &(delta.join("\n") + "\n"));
    d
}

#[test]
fn each_kind_of_record_does_what_the_plan_said_and_the_sources_facts_are_the_messages() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let bundle = synthetic("synthetic");
    let plan = plan_of(&bundle.0, None);
    let w = world("each");
    let done = run(&w, &plan, "orchard", "CC0-1.0", &|_| {}).expect("imported");
    // r1, r2, g1, r3, g3, r4: six blocks, in the source's order across pages; the removal of the page whose only text was hidden moves and
    // removes nothing (there is nothing there), and is counted; the pages end as the bundle's files.
    assert_eq!(done.revisions, 6, "{done:?}");
    assert_eq!(done.not_applied, 1, "{done:?}");
    assert!(!done.first_change);
    assert_eq!(
        (done.final_changes, done.final_written, done.final_removed),
        (0, 0, 0)
    );
    assert_eq!(
        read_text(&done.repo.join("pages/Beta.md")),
        text("Beta", "alpha", "v3")
    );
    for gone in ["pages/Alpha.md", "pages/Gamma.md", "pages/Delta.md"] {
        assert!(!done.repo.join(gone).exists(), "{gone}");
    }
    let blocks = messages(&w, &done.record.id);
    let summaries: Vec<&str> = blocks.iter().map(|m| m.summary().as_str()).collect();
    assert_eq!(
        summaries,
        [
            "Create the wiki orchard",
            "Import a revision of Alpha",
            "Import a revision of Alpha",
            "Import a revision of Gamma",
            "Import the move of Alpha to Beta",
            "Import the removal of Gamma",
            "Import a revision of Beta",
        ]
    );
    assert!(blocks.iter().skip(1).all(|m| m.kind() == Kind::Import));
    let facts: Vec<_> = blocks
        .iter()
        .skip(1)
        .map(|m| m.source().unwrap().clone())
        .collect();
    // r1: a person, a summary.
    assert_eq!(facts[0].rev(), "r1");
    assert_eq!(facts[0].at(), 1_577_872_800);
    assert_eq!(
        facts[0].author().map(|a| (a.name(), a.kind())),
        Some(("Ada", SourceAuthorKind::Person))
    );
    assert_eq!(facts[0].summary(), Some("first"));
    // r2: an author by id alone is the name and the kind `users.yaml` gives; minor; no summary given and none hidden.
    assert_eq!(
        facts[1].author().map(|a| (a.name(), a.kind())),
        Some(("Bo", SourceAuthorKind::Bot))
    );
    assert!(facts[1].is_minor() && !facts[1].is_automated());
    assert!(facts[1].summary().is_none() && !facts[1].summary_hidden());
    // r4: the author is hidden at the source, so no name is carried, and automated.
    let last = facts.last().unwrap();
    assert_eq!(last.rev(), "r4");
    assert!(last.author().is_none() && last.author_hidden());
    assert!(last.is_automated());
    assert_eq!(last.summary(), Some("tidy"));
    assert!(!blocks.iter().any(|m| m.to_text().contains("Hidden Person")));
    // Every change of the import carries the notice of the bundle, with the source named as text.
    for m in blocks.iter().skip(1) {
        let n = m.notice().expect("the notice");
        assert_eq!(n.source_name(), Some("Orchard Wiki"));
        assert_eq!(n.source_address(), Some("https://orchard.example.org/"));
        assert_eq!(n.checks(), inklkunn_core::message::ChecksState::None);
        assert_eq!(n.license(), inklkunn_core::message::LicenseWord::Manifest);
    }
    // The order is the source's time, never the order of the files.
    let times: Vec<i64> = facts.iter().map(|f| f.at()).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
}

#[test]
fn files_with_no_history_come_in_as_one_change_with_their_sidecars_and_a_level_two_bundle_has_no_other()
 {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // BookStack's export has no history; the hand-written bundle has sidecars that declare licences.
    for (name, bundle, license, files) in [
        ("book", "bookstack/bundle", Some("CC0-1.0"), 9_usize),
        ("sidecar", "sidecars/bundle", None, 5),
    ] {
        let plan = plan_of(&fixture(bundle), license);
        let w = world(name);
        let done =
            run(&w, &plan, name, license.unwrap_or("CC-BY-SA-4.0"), &|_| {}).expect("imported");
        assert!(
            done.first_change && done.revisions == 0 && done.final_changes == 0,
            "{name}: {done:?}"
        );
        // The creation and one change: every file in it.
        let blocks = messages(&w, &done.record.id);
        assert_eq!(blocks.len(), 2, "{name}");
        let first = blocks.get(1).unwrap();
        assert_eq!(first.kind(), Kind::Import);
        assert!(
            first.source().is_none(),
            "{name}: the change has no revision"
        );
        assert!(first.notice().is_some());
        let mut seen = 0;
        for f in &plan.plan.files {
            assert_eq!(
                std::fs::read(done.repo.join(&f.path)).unwrap(),
                std::fs::read(fixture(bundle).join(&f.path)).unwrap(),
                "{name}: {}",
                f.path
            );
            seen += 1;
        }
        assert_eq!(seen, files, "{name}");
    }
    // The sidecars carry the licences as they were: nothing is rewritten.
    let w = world("sidecar-licences");
    let plan = plan_of(&fixture("sidecars/bundle"), None);
    let done = run(&w, &plan, "lic", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    assert!(
        read_text(&done.repo.join("attachments/plan.svg.meta.yaml")).contains("license: CC0-1.0")
    );
    assert!(
        read_text(&done.repo.join("attachments/notes.txt.meta.yaml"))
            .contains("license: CC-BY-4.0")
    );
}

#[test]
fn a_bundle_with_its_own_help_page_replaces_the_stock_one_and_one_without_keeps_it() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // inklkunn's own export always has a `pages/Help.md`: the page of that path is the bundle's, and one page.
    let w = world("guidewiki");
    let plan = plan_of(&fixture("inklkunn/bundle"), None);
    let done = run(&w, &plan, "guidewiki", "CC0-1.0", &|_| {}).expect("imported");
    assert_eq!(
        read_text(&done.repo.join("pages/Help.md")),
        read_text(&fixture("inklkunn/bundle/pages/Help.md"))
    );
    // A bundle with no page of that path keeps the wiki's own help page (the one `wiki create` makes).
    let bundle = synthetic("nohelp");
    let w2 = world("plainwiki");
    let plan = plan_of(&bundle.0, None);
    let done = run(&w2, &plan, "plainwiki", "CC0-1.0", &|_| {}).expect("imported");
    assert!(read_text(&done.repo.join("pages/Help.md")).contains("Welcome to plainwiki"));
}

#[test]
fn a_file_that_changed_since_the_plan_fails_the_import_in_the_middle_and_nothing_of_it_remains() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let copy = dir_in(&std::env::temp_dir(), "changed-after-plan");
    copy_tree(&fixture("mediawiki/bundle"), &copy.0);
    let plan = plan_of(&copy.0, Some("CC-BY-SA-4.0"));
    // The history of the page whose revision comes last in the order: blocks of other pages are sealed before it is read.
    let (h, _) = *plan.schedule.last().unwrap();
    let last = plan.histories.get(h).unwrap().file.clone();
    let path = copy.0.join(&last);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.push(b'\n');
    std::fs::write(&path, bytes).unwrap();
    let w = world("changed");
    let seen = std::cell::RefCell::new(0_usize);
    let failed = run(&w, &plan, "changed", "CC-BY-SA-4.0", &|s| {
        if matches!(s, inklkunn_core::import::bundle::ImportStep::Revision(_)) {
            *seen.borrow_mut() += 1;
        }
    })
    .expect_err("the file changed");
    assert!(*seen.borrow() >= 1, "blocks were sealed before it failed");
    let said = failed.to_string();
    assert!(
        said.contains("did not make") || said.contains("was not made"),
        "{said}"
    );
    assert!(said.contains("not the file that was planned"), "{said}");
    assert!(said.contains("nothing was left on disk"), "{said}");
    // A failure the process survives removes everything the attempt made, the sealed blocks and the keys with it.
    assert_eq!(leftovers(&w), Vec::<String>::new());
    assert!(w.store.list().unwrap().is_empty());
    assert!(w.layout.creation_marks().unwrap().is_empty());
}

#[test]
fn a_creation_that_is_alive_is_not_discarded_and_an_import_counts_for_no_participation() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let plan = plan_of(&fixture("inklkunn/bundle"), None);
    let w = world("alive");
    let seen_alive = std::cell::Cell::new(0_usize);
    let done = run(&w, &plan, "alive", "CC0-1.0", &|s| {
        if s == inklkunn_core::import::bundle::ImportStep::Revision(1) {
            let id = w.layout.creation_marks().unwrap().remove(0);
            // The process is alive: refused, and nothing is removed.
            assert_eq!(
                discard_unfinished_creation(&*w.store, &w.layout, &*w.signer, &id, &NoSweepAudit),
                Err(DiscardRefusal::Alive)
            );
            assert!(w.layout.repo_dir(&id).is_dir());
            seen_alive.set(seen_alive.get() + 1);
        }
    })
    .expect("imported");
    assert_eq!(seen_alive.get(), 1);
    // A wiki whose record was stored is never discarded (its mark is gone, and so is any way to ask).
    assert_eq!(
        discard_unfinished_creation(
            &*w.store,
            &w.layout,
            &*w.signer,
            &done.record.id,
            &NoSweepAudit
        ),
        Err(DiscardRefusal::NoUnfinishedCreation)
    );
    // The participation counts are the proposals' and the decisions': an import is none of them, so a wiki that is only an import has none.
    let store: &MemoryStore = &w.store;
    let m = operator_metrics(
        store,
        &done.record,
        Period {
            from: 0,
            to: i64::MAX,
        },
    )
    .unwrap();
    assert_eq!(m.submitted, 0);
    assert_eq!(m.distinct_proposers, 0);
    assert_eq!(m.first_time_proposers, 0);
    assert!(m.by_state.values().all(|n| *n == 0), "{:?}", m.by_state);
    // …whatever the replay recorded: every act is a system act of `Kind: import`, none a proposal.
    let acts = w.store.system_acts(&done.record.id).unwrap();
    assert!(acts.len() > 2);
    assert!(
        acts.iter()
            .skip(1)
            .all(|a| Message::parse(&a.act.message).unwrap().kind() == Kind::Import)
    );
    // The slug is the wiki's, and one wiki only.
    assert!(w.store.by_slug(&Slug::new("alive").unwrap()).is_ok());
}
