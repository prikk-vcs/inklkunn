//! The bundle's interwiki map in the plan (RFC 007 D-8; handoff I-T4): the manifest's `interwiki` is judged one prefix at a time by the
//! judge acceptance uses, what passes is the map the import writes, what fails is left out and named, and the link prefixes the report
//! says the wiki lacks are the ones the judged map lacks.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::governance::{Refusal as Rule, read_interwiki};

/// The fixture's map as the import writes it: every template quoted, by prefix.
const FIXTURE_MAP: &str = "docs: \"https://docs.example.org/read?page={title}\"\nwikipedia: \"https://en.wikipedia.org/wiki/{title}\"\n";

/// A copy of the hand-made bundle whose manifest says `lines` under `interwiki:` (and nothing else of it changes: the manifest is
/// not in the checksums).
fn with_map(tag: &str, lines: &[String]) -> Dir {
    let d = copy_of(tag, "interwiki/bundle");
    let text = std::fs::read_to_string(d.0.join("wiki-bundle.yaml")).unwrap();
    let (before, rest) = text.split_once("interwiki:\n").unwrap();
    let after = rest.split_once("contents:").unwrap().1;
    let mut out = String::from(before);
    out.push_str("interwiki:\n");
    for l in lines {
        out.push_str("  ");
        out.push_str(l);
        out.push('\n');
    }
    out.push_str("contents:");
    out.push_str(after);
    std::fs::write(d.0.join("wiki-bundle.yaml"), out).unwrap();
    d
}

fn left_out(p: &BundlePlan) -> Vec<(String, &'static str)> {
    p.report
        .interwiki_left_out
        .iter()
        .map(|l| (l.prefix.clone(), l.rule))
        .collect()
}

#[test]
fn a_good_map_is_the_one_the_import_writes_and_the_report_says_so_and_what_it_lacks() {
    let p = plan(&fixture("interwiki/bundle"));
    assert_importable(&p);
    // `wikipedia` and `docs` pass; `help` is a namespace of the pages the import writes (`Help/Sowing`) and is left out and named.
    assert_eq!(p.interwiki.to_yaml(), FIXTURE_MAP);
    assert_eq!(read_interwiki(FIXTURE_MAP.as_bytes()).unwrap(), p.interwiki);
    assert_eq!(p.report.interwiki_in_bundle, 3);
    assert_eq!(
        p.report.interwiki_map,
        vec![
            (
                "docs".to_owned(),
                "https://docs.example.org/read?page={title}".to_owned()
            ),
            (
                "wikipedia".to_owned(),
                "https://en.wikipedia.org/wiki/{title}".to_owned()
            ),
        ]
    );
    assert_eq!(
        left_out(&p),
        vec![("help".to_owned(), Rule::PrefixIsNamespace.name())]
    );
    assert_eq!(p.report.interwiki_left_out_total, 1);
    // What the wiki's map lacks: `forum`, once. The prefixes the map covers are no longer missing, and `Help/...` is no prefix.
    assert_eq!(
        p.report.unmapped_prefixes.iter().collect::<Vec<_>>(),
        vec![(&"forum".to_owned(), &1)]
    );
    // The text says what will be written and what was left out; it no longer says the map is not read.
    let text = p.render();
    for needle in [
        "the interwiki map the wiki will have (governance/interwiki.yaml, written with the first change; the bundle's map had 3 prefix(es)):",
        "  wikipedia: https://en.wikipedia.org/wiki/{title}",
        "interwiki prefixes left out (1; the import goes on without them):",
        "  help: prefix-is-namespace",
        "link prefixes the wiki's interwiki map does not have",
        "  forum: 1 link(s)",
    ] {
        assert!(text.contains(needle), "{needle}:\n{text}");
    }
    assert!(!text.contains("does not read it yet"), "{text}");
    // The record of the act keeps the same.
    let json = p.report.to_json();
    assert_eq!(
        json.pointer("/links/interwiki_map/0/prefix"),
        Some(&serde_json::json!("docs"))
    );
    assert_eq!(
        json.pointer("/links/interwiki_left_out/0/rule"),
        Some(&serde_json::json!("prefix-is-namespace"))
    );
}

#[test]
fn a_template_that_fails_is_left_out_and_named_the_import_goes_on_and_its_links_are_missing_again()
{
    let d = with_map(
        "iw-bad-template",
        &[
            "ok: https://ok.example.org/{title}".to_owned(),
            "wikipedia: http://en.wikipedia.org/wiki/{title}".to_owned(),
            "plain: https://plain.example.org/no-placeholder".to_owned(),
            "twice: https://twice.example.org/{title}/{title}".to_owned(),
            "user: https://me@user.example.org/{title}".to_owned(),
            "host: https://-bad-.example.org/{title}".to_owned(),
            "frag: https://frag.example.org/page#{title}".to_owned(),
            "brace: \"https://brace.example.org/{title}/{other}\"".to_owned(),
            "ascii: \"https://ascii.example.org/{title}/caf\u{e9}\"".to_owned(),
        ],
    );
    let p = plan(&d.0);
    // The import goes on, and the map is the one template that passes.
    assert_importable(&p);
    assert_eq!(
        p.interwiki.iter().map(|(k, _)| k).collect::<Vec<_>>(),
        vec!["ok"]
    );
    let mut got = left_out(&p);
    got.sort();
    let mut want = vec![
        ("wikipedia".to_owned(), Rule::TemplateScheme.name()),
        ("plain".to_owned(), Rule::TemplatePlaceholder.name()),
        ("twice".to_owned(), Rule::TemplatePlaceholder.name()),
        ("user".to_owned(), Rule::TemplateUserInfo.name()),
        ("host".to_owned(), Rule::TemplateHost.name()),
        ("frag".to_owned(), Rule::TemplatePlaceholder.name()),
        ("brace".to_owned(), Rule::TemplatePlaceholder.name()),
        ("ascii".to_owned(), Rule::TemplateCharacter.name()),
    ];
    want.sort();
    assert_eq!(got, want, "{}", p.render());
    // `wikipedia` is no longer the wiki's, so its link is a missing one again: the report recomputes against the judged map.
    assert_eq!(p.report.unmapped_prefixes.get("wikipedia"), Some(&1));
    assert_eq!(p.report.unmapped_prefixes.get("docs"), Some(&1));
    let text = p.render();
    assert!(text.contains("  wikipedia: template-not-https"), "{text}");
    assert!(text.contains("the import goes on without them"), "{text}");
}

#[test]
fn a_prefix_that_is_a_namespace_is_left_out_whatever_its_case_and_one_that_is_no_prefix_is_named_escaped()
 {
    let d = with_map(
        "iw-namespace",
        &[
            "HELP: https://help.example.org/{title}".to_owned(),
            "seeds: https://seeds.example.org/{title}".to_owned(),
            "\"two words\": https://words.example.org/{title}".to_owned(),
            "\"evil\u{202e}\": https://evil.example.org/{title}".to_owned(),
            format!("{}: https://long.example.org/{{title}}", "p".repeat(33)),
        ],
    );
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(
        p.interwiki.iter().map(|(k, _)| k).collect::<Vec<_>>(),
        vec!["seeds"]
    );
    let got = left_out(&p);
    assert!(
        got.contains(&("HELP".to_owned(), Rule::PrefixIsNamespace.name())),
        "{got:?}"
    );
    assert!(
        got.contains(&("two words".to_owned(), Rule::UnknownKey.name())),
        "{got:?}"
    );
    // The bundle's own text never reaches the report raw: a direction control is written as an escape, a long prefix is judged by length.
    assert!(
        got.contains(&("evil\\u{202e}".to_owned(), Rule::UnknownKey.name())),
        "{got:?}"
    );
    let text = p.render();
    assert!(!text.contains('\u{202e}'), "{text}");
    assert!(text.contains("evil\\u{202e}"), "{text}");
    assert!(
        got.iter()
            .any(|(prefix, rule)| prefix.starts_with("pppp") && *rule == Rule::Bounds.name()),
        "{got:?}"
    );
}

#[test]
fn a_map_longer_than_the_bound_is_taken_to_the_bound_and_the_rest_is_left_out_and_counted() {
    let lines: Vec<String> = (0..300)
        .map(|n| format!("p{n:03}: https://p{n:03}.example.org/{{title}}"))
        .collect();
    let d = with_map("iw-bound", &lines);
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.interwiki_in_bundle, 300);
    assert_eq!(p.interwiki.len(), 256);
    assert_eq!(p.report.interwiki_map.len(), 256);
    // The first 256, in the bundle's order, are the map; the 44 after the bound are left out, each with the rule `bounds`.
    assert!(p.interwiki.get("p255").is_some() && p.interwiki.get("p256").is_none());
    assert_eq!(p.report.interwiki_left_out_total, 44);
    assert!(
        p.report
            .interwiki_left_out
            .iter()
            .all(|l| l.rule == Rule::Bounds.name())
    );
    // The file the map makes is one the rules read back whole.
    assert_eq!(
        read_interwiki(p.interwiki.to_yaml().as_bytes()).unwrap(),
        p.interwiki
    );
    // The text lists a bounded number and says how many more.
    let text = p.render();
    assert!(text.contains("interwiki prefixes left out (44;"), "{text}");
    assert!(text.contains("… and 19 more prefix(es)"), "{text}");
}

#[test]
fn a_bundle_with_no_map_or_one_no_prefix_of_which_passes_writes_none_and_says_which_it_was() {
    let p = plan(&small("iw-none").0);
    assert_importable(&p);
    assert!(p.interwiki.is_empty());
    assert_eq!(p.interwiki.to_yaml(), "");
    assert_eq!(p.report.interwiki_in_bundle, 0);
    assert!(!p.render().contains("interwiki map"), "{}", p.render());
    // A map every prefix of which fails is the same as none for the wiki, and the report says why there is none.
    let d = with_map(
        "iw-all-bad",
        &["a: http://a.example.org/{title}".to_owned()],
    );
    let p = plan(&d.0);
    assert_importable(&p);
    assert!(p.interwiki.is_empty());
    let text = p.render();
    assert!(
        text.contains("has 1 prefix(es), and none can be the wiki's: no map is written"),
        "{text}"
    );
    assert!(text.contains("  a: template-not-https"), "{text}");
    // A manifest whose map is not a map of text is still a refusal of the manifest, as before.
    let d = with_map("iw-not-text", &["a: [1, 2]".to_owned()]);
    let p = plan(&d.0);
    assert_refused(
        &p,
        "wiki-bundle.yaml",
        "`interwiki` maps a prefix to something that is not text",
    );
}

#[test]
fn a_bundles_governance_files_are_never_imported_the_map_comes_from_the_manifest_alone() {
    let d = copy_of("iw-governance", "interwiki/bundle");
    put(&d.0, "governance/policy.yaml", b"rungs:\n  accept: owner\n");
    put(&d.0, "governance/constitution.yaml", b"requirements: {}\n");
    // The bundle's own file of the map says something else than its manifest: it is not read.
    put(
        &d.0,
        "governance/interwiki.yaml",
        b"evil: \"https://evil.example.org/{title}\"\n",
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_importable(&p);
    assert!(
        p.plan
            .files
            .iter()
            .all(|f| !f.path.starts_with("governance/")),
        "{:?}",
        p.plan.files
    );
    assert_eq!(p.interwiki.to_yaml(), FIXTURE_MAP);
    let line = p
        .report
        .left_out
        .iter()
        .find(|l| l.what == "governance/")
        .expect("listed as not imported");
    assert_eq!(line.files, 3);
    assert!(
        line.why
            .contains("none of a bundle's governance files is imported")
    );
}

#[test]
fn the_map_is_judged_against_the_titles_of_every_page_the_replay_writes_a_moved_away_one_too() {
    // A page that is moved from `Garden/Beds` to `Beds` during the history is gone from `Garden/` at the end, and its directory is a
    // namespace the wiki had while the import ran: a prefix of that name is left out, as the safer side. (The first record's text was
    // hidden at the source, so no revision of the page is ever at `Garden/Beds`: only the rename's `from` names it.)
    let d = dir("iw-history");
    manifest(
        &d.0,
        &[
            "license: CC-BY-SA-4.0",
            "checksums: sha256sums.txt",
            "interwiki:",
            "  garden: https://garden.example.org/{title}",
            "  fine: https://fine.example.org/{title}",
        ],
    );
    let beds = |n: u32| format!("---\nid: beds\ntitle: Beds\n---\n\nBeds {n}.\n");
    put(&d.0, "pages/Beds.md", beds(2).as_bytes());
    let r1 = serde_json::json!({
        "rev": "r1", "parent": null, "type": "create", "at": "2025-01-01T00:00:00Z",
        "suppressed": ["content"], "reason": "personal-data",
    })
    .to_string();
    let mut moved: serde_json::Value = serde_json::from_str(&record(
        "r2",
        Some("r1"),
        "2025-01-02T00:00:00Z",
        Some(&beds(2)),
    ))
    .unwrap();
    set(&mut moved, "type", "rename");
    set(&mut moved, "from", "Garden/Beds");
    set(&mut moved, "to", "Beds");
    put(
        &d.0,
        "history/beds.jsonl",
        format!("{r1}\n{moved}\n").as_bytes(),
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_importable(&p);
    let names: Vec<&str> = p.interwiki.iter().map(|(k, _)| k).collect();
    assert!(names.contains(&"fine"), "{names:?}");
    assert!(!names.contains(&"garden"), "{names:?}\n{}", p.render());
    assert!(
        left_out(&p).contains(&("garden".to_owned(), Rule::PrefixIsNamespace.name())),
        "{:?}",
        left_out(&p)
    );
}

#[test]
fn a_page_titled_in_a_mapped_prefix_is_never_written_beside_the_prefix_the_prefix_is_left_out() {
    // The rule is symmetric (correction W-5): a wiki never has a page `GH:Notes` and a map with `gh`. An import judges it as acceptance
    // does, by the same definition of a namespace and without regard to case, and it keeps the page: the bundle's prefix is the one left
    // out, named, and the links that used it are reported missing as for any prefix the wiki lacks.
    let d = dir("iw-symmetric");
    manifest(
        &d.0,
        &[
            "license: CC-BY-SA-4.0",
            "checksums: sha256sums.txt",
            "interwiki:",
            "  gh: https://github.com/org/repo/wiki/{title}",
            "  fine: https://fine.example.org/{title}",
        ],
    );
    let path = inklkunn_markup::path::title_to_path("GH:Notes").unwrap();
    put(
        &d.0,
        &path,
        b"---\nid: notes\ntitle: GH:Notes\n---\n\nNotes.\n",
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_importable(&p);
    let names: Vec<&str> = p.interwiki.iter().map(|(k, _)| k).collect();
    assert_eq!(names, vec!["fine"], "{}", p.render());
    assert_eq!(
        left_out(&p),
        vec![("gh".to_owned(), Rule::PrefixIsNamespace.name())]
    );
    // The map that results and the page that is written never meet the rule in the other direction.
    let kept = p
        .interwiki
        .iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
    let (map, left) = crate::governance::judge_interwiki(kept, &Default::default());
    assert!(left.is_empty());
    assert!(map.takes_namespace_of("GH:Notes").is_none());
}
