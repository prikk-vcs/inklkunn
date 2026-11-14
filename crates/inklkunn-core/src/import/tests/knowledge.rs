//! The import of another inklkunn's bundle and its knowledge files (RFC 005 part 2 D-12, handoff 005 K-T11): read only
//! when the manifest declares the extension, each file read with the format's own reader, the whole set judged as an
//! acceptance judges a change that is not a curation proposal, at the service's own path. No test needs prikk.

use super::*;
use crate::knowledge::{
    Bearing, Claim, ClaimId, CommunityStatus, Expression, Link, Source, SourceId, SourceLink,
    Standing, StatementKind,
};

const MANIFEST: &str = "format: portable-wiki-bundle\nversion: '0.4'\nextensions_used:\n  - name: inklkunn-knowledge\n    version: \"1\"\n    directory: extensions/inklkunn-knowledge\n";

const PAGE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n";

fn claim_id() -> ClaimId {
    ClaimId::parse("019a0000-0000-7000-8000-000000000c01").unwrap()
}

fn source_id() -> SourceId {
    SourceId::parse("019a1111-0000-7000-8000-000000000501").unwrap()
}

/// The wording `p1#b1` has in `PAGE`.
fn said() -> crate::knowledge::Digest {
    let found = inklkunn_markup::blocks("Said. ^b1");
    crate::knowledge::block_digest(found.first().unwrap())
}

fn claim() -> Claim {
    let mut c = Claim::new(claim_id());
    c.kind = StatementKind::Observation;
    c.expressions = vec![Expression::new("p1", "b1").unwrap()];
    c.sources = vec![SourceLink {
        source: source_id(),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: said(),
    }];
    c
}

fn source() -> Source {
    let mut s = Source::new(source_id());
    s.identity.title = Some("The Bridge Survey".to_owned());
    s
}

fn claim_path() -> String {
    crate::knowledge::claim_path(&claim_id())
}

fn source_path() -> String {
    crate::knowledge::source_path(&source_id())
}

/// A bundle of an inklkunn wiki: the manifest, a page, a claim and its source, and what the extension keeps outside.
fn bundle(d: &Path, claim: &Claim, source: &Source) {
    put(d, "wiki-bundle.yaml", MANIFEST.as_bytes());
    put(d, "pages/One.md", PAGE.as_bytes());
    put(d, "knowledge/.keep", b"");
    put(d, &claim_path(), claim.to_yaml().as_bytes());
    put(d, &source_path(), source.to_yaml().as_bytes());
    put(d, "extensions/inklkunn-knowledge/threads/x.jsonl", b"{}\n");
}

#[test]
fn a_bundle_that_declares_the_extension_has_its_claim_and_source_files_planned_and_the_extensions_directory_named()
 {
    let d = dir("k-plan");
    bundle(&d.0, &claim(), &source());
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    let paths: Vec<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    let expected = [claim_path(), source_path(), "pages/One.md".to_owned()];
    assert_eq!(
        paths,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "the keep-file is no knowledge file"
    );
    assert_eq!((plan.counts(), plan.knowledge_count()), ((1, 0), 2));
    let skipped: Vec<&str> = plan.not_imported.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(skipped, vec!["extensions", "wiki-bundle.yaml"]);
    let why = &plan
        .not_imported
        .iter()
        .find(|(n, _)| n == "extensions")
        .unwrap()
        .1;
    assert!(why.contains("claim threads"), "{why}");
    // Not "the exporting service's own": any extension's data may be there, and none of it is read, the manifest's
    // `directory:` included.
    assert!(
        why.contains("extensions the exporting service used") && why.contains("none of it is read"),
        "{why}"
    );
    assert!(
        plan.report()
            .contains("1 page(s), 0 attachment(s), 2 claim or source file(s)"),
        "{}",
        plan.report()
    );
}

#[test]
fn knowledge_is_read_only_when_the_manifest_declares_the_extension_this_build_reads() {
    for (tag, manifest, said) in [
        ("none", None, "does not declare"),
        (
            "other-manifest",
            Some("format: portable-wiki-bundle\nversion: '0.4'\n"),
            "does not declare",
        ),
        (
            "other-name",
            Some("extensions_used:\n  - name: someone-elses\n    version: \"1\"\n"),
            "does not declare",
        ),
        (
            "v2",
            Some("extensions_used:\n  - name: inklkunn-knowledge\n    version: \"2\"\n"),
            "version 2",
        ),
        // The version is shown as the manifest wrote it, never as if it had written none.
        (
            "float",
            Some("extensions_used:\n  - name: inklkunn-knowledge\n    version: 1.5\n"),
            "version 1.5",
        ),
        (
            "float-one",
            Some("extensions_used:\n  - name: inklkunn-knowledge\n    version: 1.0\n"),
            "version 1.0",
        ),
        (
            "empty",
            Some("extensions_used:\n  - name: inklkunn-knowledge\n    version: \"\"\n"),
            "version (empty)",
        ),
        (
            "no-version",
            Some("extensions_used:\n  - name: inklkunn-knowledge\n"),
            "version (none)",
        ),
        (
            "unreadable",
            Some("extensions_used: [: :\n"),
            "does not declare",
        ),
    ] {
        let d = dir(&format!("k-undeclared-{tag}"));
        bundle(&d.0, &claim(), &source());
        match manifest {
            Some(text) => put(&d.0, "wiki-bundle.yaml", text.as_bytes()),
            None => std::fs::remove_file(d.0.join("wiki-bundle.yaml")).unwrap(),
        }
        let plan = plan_folder(&d.0, &nothing(), false).unwrap();
        assert!(plan.is_importable(), "{tag}: {}", plan.report());
        assert_eq!(plan.knowledge_count(), 0, "{tag}");
        let why = &plan
            .not_imported
            .iter()
            .find(|(n, _)| n == "knowledge")
            .unwrap_or_else(|| panic!("{tag}: knowledge is listed as not imported"))
            .1;
        assert!(why.contains(said), "{tag}: {why}");
    }
}

#[test]
fn a_knowledge_file_that_does_not_read_or_is_not_a_claim_or_source_is_refused_by_rule_and_never_by_its_words()
 {
    let d = dir("k-bad");
    bundle(&d.0, &claim(), &source());
    // A claim that does not read: the words in it must not come back in the report.
    put(
        &d.0,
        "knowledge/claims/019a0000-0000-7000-8000-000000000c02.yaml",
        b"kind: observation\nsecret: the-quiet-words\nexpressions: []\n",
    );
    put(&d.0, "knowledge/notes/readme.txt", b"the-quiet-words");
    put(
        &d.0,
        "knowledge/claims/not-an-id.yaml",
        b"kind: observation\n",
    );
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(!plan.is_importable());
    assert_eq!(plan.refused.len(), 3, "{}", plan.report());
    let report = plan.report();
    assert!(!report.contains("the-quiet-words"), "{report}");
    assert!(
        report.contains("knowledge/notes/readme.txt: is not a claim or a source file"),
        "{report}"
    );
    assert!(
        report.contains(
            "019a0000-0000-7000-8000-000000000c02.yaml: the file has a key the format does not have"
        ),
        "{report}"
    );
    assert!(report.contains("not-an-id.yaml"), "{report}");
}

#[test]
fn what_the_wiki_has_of_knowledge_is_taken_into_account_like_a_page() {
    let d = dir("k-exists");
    bundle(&d.0, &claim(), &source());
    let has = nothing().with_file(&claim_path(), claim().to_yaml().as_bytes());
    let taken = plan_folder(&d.0, &has, false).unwrap();
    assert_eq!(taken.refused.len(), 1, "{}", taken.report());
    assert!(
        taken
            .refused
            .first()
            .unwrap()
            .reason
            .contains("already exists")
    );
    let again = plan_folder(&d.0, &has, true).unwrap();
    assert_eq!(again.unchanged, vec![claim_path()]);
}

/// A wiki whose fake repository has `PAGE`, and the service's own path over a signer stand-in, able to read it.
fn world_with_reads(tag: &str) -> (Rig, Arc<Recorder>, SystemActs) {
    let (rig, accept, acts) = world(tag);
    let acts = acts.with_reads(rig.reads.clone());
    (rig, accept, acts)
}

fn planned(tag: &str, rig: &Rig, claim: &Claim, source: &Source) -> (Dir, ImportPlan) {
    let d = dir(tag);
    bundle(&d.0, claim, source);
    let plan = plan_folder(&d.0, &Existing::at(&rig.fake.block_id()), false).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    (d, plan)
}

#[test]
fn an_import_with_knowledge_files_is_judged_at_the_service_and_then_goes_to_the_signer() {
    let (rig, accept, acts) = world_with_reads("k-run");
    let (_d, plan) = planned("k-run", &rig, &claim(), &source());
    let done = run_import(&acts, &rig.wiki, &plan).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.files, 3);
    let change = accept.seen.lock().unwrap().clone().unwrap();
    let mut written: Vec<String> = change
        .files
        .iter()
        .map(|f| f.path.display().to_string())
        .collect();
    written.sort();
    assert_eq!(
        written,
        vec![claim_path(), source_path(), "pages/One.md".to_owned()]
    );
    let message = Message::parse(&change.message).unwrap();
    assert!(
        message
            .summary()
            .as_str()
            .contains("2 claim or source file(s)"),
        "{}",
        message.summary().as_str()
    );
}

/// What an import is refused with, as a sentence, and that the signer was never asked.
fn refused_import(tag: &str, claim: &Claim, source: &Source) -> String {
    let (rig, accept, acts) = world_with_reads(tag);
    let (_d, plan) = planned(tag, &rig, claim, source);
    let err = run_import(&acts, &rig.wiki, &plan).unwrap_err();
    assert!(
        accept.seen.lock().unwrap().is_none(),
        "{tag}: the signer was asked"
    );
    assert!(
        rig.store.system_acts(&rig.wiki.id).unwrap().is_empty(),
        "{tag}: an act was recorded for an import that was refused"
    );
    match err {
        ImportError::Act(SystemActFailure::Invalid(why)) => why,
        other => panic!("{tag}: {other}"),
    }
}

#[test]
fn an_import_sets_nothing_that_only_a_curation_proposal_sets() {
    // A community status.
    let mut decided = claim();
    decided.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some("019a0000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(said()),
    };
    let why = refused_import("k-status", &decided, &source());
    assert!(
        why.contains(&claim_path()) && why.contains("community status"),
        "{why}"
    );
    // A source that was checked: a review day, or an availability.
    let mut checked = source();
    checked.availability = crate::knowledge::Availability::Unavailable;
    let why = refused_import("k-checked", &claim(), &checked);
    assert!(why.contains(&source_path()), "{why}");
}

#[test]
fn an_import_is_held_to_what_an_acceptance_holds_a_change_to() {
    // A claim that names a source the import does not bring (and the view does not have).
    let mut dangling = claim();
    dangling.sources.first_mut().unwrap().source =
        SourceId::parse("019a1111-0000-7000-8000-000000000502").unwrap();
    let why = refused_import("k-dangling", &dangling, &source());
    assert!(
        why.contains("names a source or a claim the view does not have"),
        "{why}"
    );
    // A claim anchored at a block no page has.
    let mut adrift = claim();
    adrift.expressions = vec![Expression::new("p1", "no-such-block").unwrap()];
    let why = refused_import("k-anchor", &adrift, &source());
    assert!(why.contains("names a page the view does not have"), "{why}");
    // A link judged on wording the claim does not have (K25).
    let mut unjudged = claim();
    unjudged.sources.first_mut().unwrap().judged_on =
        crate::knowledge::Digest::parse(&"0".repeat(64)).unwrap();
    let why = refused_import("k-judged", &unjudged, &source());
    assert!(why.contains("judged on a wording"), "{why}");
}

#[test]
fn a_system_change_still_may_not_write_the_knowledge_layer_and_an_import_cannot_be_judged_by_a_service_that_cannot_read()
 {
    let (rig, accept, acts) = world_with_reads("k-system");
    let request = |kind| SystemRequest {
        kind,
        summary: "x".to_owned(),
        files: vec![PreparedFile {
            path: PathBuf::from(claim_path()),
            content: Some(claim().to_yaml().into_bytes()),
        }],
        renames: Vec::new(),
        imported_from: (kind == SystemActKind::Import)
            .then(|| ImportSource::new("folder sha256:00").unwrap()),
        planned_against: None,
    };
    let err = acts
        .run(
            &rig.wiki,
            &SystemLabel::inklkunn(),
            request(SystemActKind::System),
        )
        .unwrap_err();
    assert!(
        matches!(&err, SystemActFailure::Invalid(why) if why.contains("does not write the knowledge layer")),
        "{err}"
    );
    // The service that cannot read the wiki does not wave knowledge files through.
    let blind = rig.system_acts(accept.clone());
    let err = blind
        .run(
            &rig.wiki,
            &SystemLabel::inklkunn(),
            request(SystemActKind::Import),
        )
        .unwrap_err();
    assert!(matches!(err, SystemActFailure::Unavailable(_)), "{err}");
    assert!(accept.seen.lock().unwrap().is_none());
}

#[test]
fn the_dry_run_says_what_the_run_would_refuse() {
    let (rig, _accept, _acts) = world_with_reads("k-dry");
    let mut decided = claim();
    decided.community_status.status = Standing::Accepted;
    decided.community_status.judged_on = Some(said());
    let (_d, mut plan) = planned("k-dry", &rig, &decided, &source());
    assert!(plan.refused.is_empty());
    judge_knowledge(&mut plan, &rig.reads, &*rig.fake, &rig.layout, &rig.wiki).unwrap();
    assert!(!plan.is_importable());
    assert_eq!(plan.refused.len(), 1, "{}", plan.report());
    assert!(
        plan.refused
            .first()
            .unwrap()
            .name
            .contains("knowledge/claims/")
    );
    // A sound set adds nothing.
    let (_d, mut sound) = planned("k-dry-sound", &rig, &claim(), &source());
    judge_knowledge(&mut sound, &rig.reads, &*rig.fake, &rig.layout, &rig.wiki).unwrap();
    assert!(sound.is_importable(), "{}", sound.report());
}

/// The reviewer's fixture (handoff 005 K52): the bundle of a wiki that has claims and a redirect page.
#[test]
fn an_import_is_judged_as_the_service_so_a_redirect_page_and_claims_import_together() {
    let (rig, accept, acts) = world_with_reads("k-redirect");
    let d = dir("k-redirect");
    bundle(&d.0, &claim(), &source());
    put(
        &d.0,
        "pages/Old.md",
        b"---\ntitle: Old\nkind: redirect\nredirect: One\n---\n\n",
    );
    let mut plan = plan_folder(&d.0, &Existing::at(&rig.fake.block_id()), false).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    // The dry run says nothing against it...
    judge_knowledge(&mut plan, &rig.reads, &*rig.fake, &rig.layout, &rig.wiki).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    // ... and the run, at the service's own path, writes all of it.
    let done = run_import(&acts, &rig.wiki, &plan).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.files, 4);
    let change = accept.seen.lock().unwrap().clone().unwrap();
    assert!(
        change
            .files
            .iter()
            .any(|f| f.path == std::path::Path::new("pages/Old.md"))
    );
    // The knowledge rules still hold for that bundle: a decided status is refused beside the redirect.
    let mut decided = claim();
    decided.community_status.status = Standing::Accepted;
    let d = dir("k-redirect-decided");
    bundle(&d.0, &decided, &source());
    put(
        &d.0,
        "pages/Old.md",
        b"---\ntitle: Old\nkind: redirect\nredirect: One\n---\n\n",
    );
    let mut plan = plan_folder(&d.0, &Existing::at(&rig.fake.block_id()), false).unwrap();
    judge_knowledge(&mut plan, &rig.reads, &*rig.fake, &rig.layout, &rig.wiki).unwrap();
    assert_eq!(plan.refused.len(), 1, "{}", plan.report());
    assert!(
        plan.refused
            .first()
            .unwrap()
            .name
            .contains("knowledge/claims/")
    );
}
