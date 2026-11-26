//! What an import may write below `governance/` (RFC 007 D-8; handoff I-T4): **the interwiki map, and nothing else there**, the map's bytes
//! judged on the tip the change is written on as an acceptance judges them. The service's own path is run over the signer stand-in, so no
//! test needs prikk.

use super::*;
use crate::acceptance::SystemSigned;
use crate::governance::INTERWIKI_PATH;

const MAP: &str = "docs: \"https://docs.example.org/read?page={title}\"\n";

fn file(path: &str, text: &str) -> PreparedFile {
    PreparedFile {
        path: PathBuf::from(path),
        content: Some(text.as_bytes().to_vec()),
    }
}

fn request(
    kind: SystemActKind,
    files: Vec<PreparedFile>,
    renames: Vec<(PathBuf, PathBuf)>,
) -> SystemRequest {
    SystemRequest {
        kind,
        summary: "Import".to_owned(),
        files,
        renames,
        imported_from: (kind == SystemActKind::Import)
            .then(|| ImportSource::new("bundle sha256:00").unwrap()),
        source: None,
        notice: None,
        planned_against: None,
    }
}

/// An import's request is run, and what the service said is returned with what reached the signer.
fn run_it(
    tag: &str,
    request: SystemRequest,
    blind: bool,
) -> (
    Result<SystemSigned, SystemActFailure>,
    Option<PreparedChange>,
) {
    let (rig, accept, acts) = world(tag);
    let acts = if blind {
        acts
    } else {
        acts.with_reads(rig.reads.clone())
    };
    let got = acts.run(&rig.wiki, &SystemLabel::inklkunn(), request);
    let seen = accept.seen.lock().unwrap().clone();
    (got, seen)
}

#[track_caller]
fn refused(why_has: &str, request: SystemRequest) {
    let (got, seen) = run_it("iw-refused", request, false);
    assert!(
        matches!(&got, Err(SystemActFailure::Invalid(why)) if why.contains(why_has)),
        "{got:?}"
    );
    assert!(seen.is_none(), "the signer was asked");
}

#[test]
fn an_import_writes_the_map_with_pages_and_the_signer_is_asked_for_exactly_those_files() {
    let (got, seen) = run_it(
        "iw-ok",
        request(
            SystemActKind::Import,
            vec![
                file("pages/Seeds.md", "---\ntitle: Seeds\n---\n\nSeeds.\n"),
                file(INTERWIKI_PATH, MAP),
            ],
            Vec::new(),
        ),
        false,
    );
    assert!(got.is_ok(), "{got:?}");
    let seen = seen.expect("the signer was asked");
    let mut written: Vec<String> = seen
        .files
        .iter()
        .map(|f| f.path.display().to_string())
        .collect();
    written.sort();
    assert_eq!(written, vec![INTERWIKI_PATH, "pages/Seeds.md"]);
}

#[test]
fn an_import_writes_no_other_path_below_governance_the_bundles_own_rules_never_come_in() {
    for path in [
        "governance/policy.yaml",
        "governance/constitution.yaml",
        "governance/other.yaml",
        "governance/interwiki.yaml/inner.yaml",
        "governance",
        "./governance/policy.yaml",
        "Governance/policy.yaml",
    ] {
        refused(
            "no path below governance/ but the interwiki map",
            request(
                SystemActKind::Import,
                vec![file(path, "rungs: {}\n")],
                Vec::new(),
            ),
        );
    }
    // Nor beside the map, nor as a removal or a move of it.
    refused(
        "no path below governance/ but the interwiki map",
        request(
            SystemActKind::Import,
            vec![
                file(INTERWIKI_PATH, MAP),
                file("governance/policy.yaml", "rungs: {}\n"),
            ],
            Vec::new(),
        ),
    );
    refused(
        "no path below governance/ but the interwiki map",
        request(
            SystemActKind::Import,
            vec![PreparedFile {
                path: PathBuf::from(INTERWIKI_PATH),
                content: None,
            }],
            Vec::new(),
        ),
    );
    refused(
        "no path below governance/ but the interwiki map",
        request(
            SystemActKind::Import,
            vec![file("pages/A.md", "---\ntitle: A\n---\n\nA.\n")],
            vec![(PathBuf::from(INTERWIKI_PATH), PathBuf::from("pages/B.md"))],
        ),
    );
}

#[test]
fn the_maps_path_is_the_exact_string_every_other_spelling_of_it_is_refused() {
    // `Path` equality folds `//` and `./` away, so a comparison by components would have taken these for the map and the signer would
    // have been asked to write them as spelled (correction W-8). Only `governance/interwiki.yaml` itself is the map.
    for spelled in [
        "governance//interwiki.yaml",
        "governance/./interwiki.yaml",
        "./governance/interwiki.yaml",
        "governance/interwiki.yaml/",
        "governance///interwiki.yaml",
    ] {
        // As a write, alone.
        refused(
            "no path below governance/ but the interwiki map",
            request(SystemActKind::Import, vec![file(spelled, MAP)], Vec::new()),
        );
        // As the `to` of a rename.
        refused(
            "no path below governance/ but the interwiki map",
            request(
                SystemActKind::Import,
                vec![file("pages/A.md", "---\ntitle: A\n---\n\nA.\n")],
                vec![(PathBuf::from("pages/B.md"), PathBuf::from(spelled))],
            ),
        );
        // Beside the map itself: two governance paths are never one write.
        refused(
            "no path below governance/ but the interwiki map",
            request(
                SystemActKind::Import,
                vec![file(INTERWIKI_PATH, MAP), file(spelled, MAP)],
                Vec::new(),
            ),
        );
    }
}

#[test]
fn the_maps_bytes_are_judged_at_the_service_as_an_acceptance_judges_them() {
    // A template that is not https, a second placeholder, a key that is no prefix, a file that is not text.
    for bad in [
        "docs: \"http://docs.example.org/{title}\"\n",
        "docs: \"https://docs.example.org/{title}/{title}\"\n",
        "\"two words\": \"https://docs.example.org/{title}\"\n",
        "version: 1\n",
    ] {
        refused(
            "the interwiki map of the import is refused",
            request(
                SystemActKind::Import,
                vec![file(INTERWIKI_PATH, bad)],
                Vec::new(),
            ),
        );
    }
    refused(
        "the interwiki map of the import is refused",
        request(
            SystemActKind::Import,
            vec![PreparedFile {
                path: PathBuf::from(INTERWIKI_PATH),
                content: Some(vec![0xff, 0xfe]),
            }],
            Vec::new(),
        ),
    );
    // A prefix that is a namespace of a page the same change writes (the first change of an import has no other title to say so).
    refused(
        "the interwiki map of the import is refused",
        request(
            SystemActKind::Import,
            vec![
                file(
                    "pages/Help/Sowing.md",
                    "---\ntitle: Help/Sowing\n---\n\nSow.\n",
                ),
                file(
                    INTERWIKI_PATH,
                    "help: \"https://help.example.org/{title}\"\n",
                ),
            ],
            Vec::new(),
        ),
    );
    // The refusal names the file and the rule, never what the file said.
    let (got, _) = run_it(
        "iw-words",
        request(
            SystemActKind::Import,
            vec![file(
                INTERWIKI_PATH,
                "secret-prefix: \"http://never-repeated.example.org/{title}\"\n",
            )],
            Vec::new(),
        ),
        false,
    );
    let why = match got {
        Err(SystemActFailure::Invalid(why)) => why,
        other => panic!("{other:?}"),
    };
    assert!(why.contains(INTERWIKI_PATH), "{why}");
    assert!(
        !why.contains("never-repeated") && !why.contains("secret-prefix"),
        "{why}"
    );
}

#[test]
fn a_service_that_cannot_read_the_wiki_does_not_wave_a_map_through() {
    let (got, seen) = run_it(
        "iw-blind",
        request(
            SystemActKind::Import,
            vec![file(INTERWIKI_PATH, MAP)],
            Vec::new(),
        ),
        true,
    );
    assert!(
        matches!(got, Err(SystemActFailure::Unavailable(_))),
        "{got:?}"
    );
    assert!(seen.is_none());
}
