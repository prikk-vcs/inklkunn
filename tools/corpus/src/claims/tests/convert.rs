use std::path::Path;
use std::process::Command;

use crate::claims::convert::{
    Entry, convert, export_xml, parse_losses, synthetic_timestamp, xml_escape,
};
use crate::claims::{converted_path, parse_revisions, raw_path, write_file};

use super::support::Tmp;

#[test]
fn the_export_escapes_text_and_names_no_contributor_or_comment() {
    let revs = [
        Entry {
            id: 11,
            parent: 10,
            position: 0,
            text: "Tom & <Jerry> met > twice.",
        },
        Entry {
            id: 12,
            parent: 11,
            position: 1,
            text: "橋は一九五〇年に開通した。",
        },
    ];
    let (xml, dropped) = export_xml("en", "Tom & Jerry", 3, &revs);
    assert_eq!(dropped, 0);
    assert!(xml.contains("Tom &amp; &lt;Jerry&gt; met &gt; twice."));
    assert!(xml.contains("<title>Tom &amp; Jerry</title>"));
    assert!(xml.contains("橋は一九五〇年に開通した。"));
    assert!(xml.contains("xmlns=\"http://www.mediawiki.org/xml/export-0.11/\""));
    assert!(xml.contains("<text xml:space=\"preserve\">"));
    assert!(xml.contains("<ns>0</ns>") && xml.contains("<id>3</id>"));
    assert!(xml.contains("<id>12</id>") && xml.contains("<parentid>11</parentid>"));
    for bad in ["contributor", "comment", "username", "<ip>", "<minor"] {
        assert!(!xml.contains(bad), "{bad} in the export");
    }
    // The export is well formed: every tag opened is closed (a crude check; the converter parses it for real).
    assert_eq!(
        xml.matches("<revision>").count(),
        xml.matches("</revision>").count()
    );
}

#[test]
fn characters_xml_cannot_carry_are_dropped_and_counted() {
    assert_eq!(xml_escape("a\u{1}b\u{8}c"), ("abc".to_owned(), 2));
    assert_eq!(
        xml_escape("tab\there\nline"),
        ("tab\there\nline".to_owned(), 0)
    );
    let (xml, dropped) = export_xml(
        "en",
        "T",
        1,
        &[Entry {
            id: 1,
            parent: 0,
            position: 0,
            text: "x\u{2}y",
        }],
    );
    assert_eq!(dropped, 1);
    assert!(xml.contains(">xy<"));
}

#[test]
fn timestamps_are_synthetic_and_strictly_increasing() {
    assert_eq!(synthetic_timestamp(0), "2001-01-01T00:00:00Z");
    assert_eq!(synthetic_timestamp(1), "2001-01-01T00:00:01Z");
    assert_eq!(synthetic_timestamp(59), "2001-01-01T00:00:59Z");
    assert_eq!(synthetic_timestamp(61), "2001-01-01T00:01:01Z");
    assert_eq!(synthetic_timestamp(3_600), "2001-01-01T01:00:00Z");
    assert_eq!(synthetic_timestamp(86_400), "2001-01-02T00:00:00Z");
    assert_eq!(synthetic_timestamp(31 * 86_400), "2001-02-01T00:00:00Z");
    assert_eq!(synthetic_timestamp(365 * 86_400), "2002-01-01T00:00:00Z");
    let all: Vec<String> = (0..5_000).map(synthetic_timestamp).collect();
    assert!(all.windows(2).all(|w| w.first() < w.get(1)));
}

#[test]
fn the_converters_losses_are_read_from_its_report() {
    let report = "# Import report\n\n## Fidelity\n\n- **Level reached:** 3 (0 text)\n- **Degraded:**\n  - templates and parser functions (2×): kept as source\n  - wikitext: built-in converter\n- **Dropped:**\n  - media files: not in dumps\n\n## Details\n\n  - not a loss\n";
    let (degraded, dropped) = parse_losses(report);
    assert_eq!(
        degraded,
        [
            "templates and parser functions (2×): kept as source",
            "wikitext: built-in converter"
        ]
    );
    assert_eq!(dropped, ["media files: not in dumps"]);
    assert_eq!(parse_losses("nothing here"), (vec![], vec![]));
}

const FAKE: &str = r###"import json, os, sys
import xml.etree.ElementTree as ET

args = sys.argv[1:]
assert args[:2] == ["mediawiki", "import"], args
src, out = args[2], args[3]
raw = open(src, encoding="utf-8").read()
assert "contributor" not in raw and "comment" not in raw
ns = "{http://www.mediawiki.org/xml/export-0.11/}"
page = ET.fromstring(raw).find(ns + "page")
os.makedirs(out + "/history", exist_ok=True)
lines = []
for rev in page.findall(ns + "revision"):
    rid = rev.find(ns + "id").text
    text = rev.find(ns + "text").text or ""
    if "UNCONVERTIBLE" in text:
        continue
    lines.append(json.dumps({"rev": "r" + rid, "content": "---\ntitle: T\n---\nCONVERTED " + text}, ensure_ascii=False))
open(out + "/history/mw_1.jsonl", "w", encoding="utf-8").write("\n".join(lines) + "\n")
open(out + "/seen.xml", "w", encoding="utf-8").write(raw)
open(out + "/import-report.md", "w", encoding="utf-8").write(
    "## Fidelity\n\n- **Level reached:** 3\n- **Degraded:**\n  - wikitext: fake converter\n- **Dropped:**\n  - media files: none\n\n## Details\n")
"###;

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn a_run_against_a_fake_converter_caches_converted_text_and_records_what_it_could_not() {
    if !python_available() {
        eprintln!("skipping: python3 not on PATH");
        return;
    }
    let wc = Tmp::new("wc");
    write_file(
        &wc.path().join("tools").join("wikicommons.py"),
        FAKE.as_bytes(),
    )
    .unwrap();
    // A real (empty) git history, so the report can name the checkout's commit.
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(wc.path())
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "x"]);
    let head = git(&["rev-parse", "HEAD"]).trim().to_owned();

    let cache = Tmp::new("convert");
    let articles = parse_revisions(
        "en\tTom & Jerry\t11\t10\nen\tTom & Jerry\t12\t11\nen\tTom & Jerry\t13\t12\nen\tTom & Jerry\t14\t13\nja\t古い橋\t21\t20\n",
    )
    .unwrap();
    let a = articles.first().unwrap();
    let put = |lang: &str, key: &str, id: u64, text: &str| {
        write_file(&raw_path(cache.path(), lang, key, id), text.as_bytes()).unwrap();
    };
    put("en", &a.key, 11, "Tom & <Jerry> met.");
    put("en", &a.key, 12, "UNCONVERTIBLE sentence.");
    put("en", &a.key, 13, "Tom met Jerry twice.");
    // 14 was deleted upstream: no raw text.
    put(
        "ja",
        &articles.get(1).unwrap().key,
        21,
        "橋は一九五〇年に開通した。",
    );

    let report = convert(&articles, cache.path(), wc.path()).unwrap();
    assert_eq!(report.wikicommons_commit.as_deref(), Some(head.as_str()));
    assert!(report.timestamps.contains("synthetic"));
    let en = report
        .languages
        .get("en")
        .unwrap()
        .get("Tom & Jerry")
        .unwrap();
    assert_eq!(
        (en.pinned, en.missing_raw, en.converted, en.skipped),
        (4, 1, 2, 1)
    );
    assert_eq!(en.degraded, ["wikitext: fake converter"]);
    assert_eq!(en.dropped, ["media files: none"]);
    let ja = report.languages.get("ja").unwrap().get("古い橋").unwrap();
    assert_eq!((ja.converted, ja.skipped), (1, 0));

    let read = |lang: &str, key: &str, id: u64| {
        std::fs::read_to_string(converted_path(cache.path(), lang, key, id))
    };
    assert_eq!(
        read("en", &a.key, 11).unwrap(),
        "---\ntitle: T\n---\nCONVERTED Tom & <Jerry> met."
    );
    assert!(
        read("en", &a.key, 12).is_err(),
        "an unconvertible revision is skipped, not replaced"
    );
    assert!(read("en", &a.key, 13).is_ok());
    assert!(
        read("ja", &articles.get(1).unwrap().key, 21)
            .unwrap()
            .contains("橋は一九五〇年に開通した。")
    );

    // What the converter was given: synthetic times by position, nothing about editors.
    let seen = std::fs::read_to_string(
        cache
            .path()
            .join("convert")
            .join("en")
            .join(&a.key)
            .join("bundle")
            .join("seen.xml"),
    )
    .unwrap();
    assert!(seen.contains("<timestamp>2001-01-01T00:00:00Z</timestamp>"));
    assert!(seen.contains("<timestamp>2001-01-01T00:00:02Z</timestamp>"));
    assert!(
        !seen.contains("<timestamp>2001-01-01T00:00:03Z</timestamp>"),
        "revision 14 was not given"
    );
    assert!(!seen.contains("contributor"));
}

#[test]
fn a_missing_converter_is_a_clear_error() {
    let cache = Tmp::new("convert-missing");
    let articles = parse_revisions("en\tT\t1\t0\n").unwrap();
    let err = convert(
        &articles,
        cache.path(),
        Path::new("/nonexistent-wiki-commons"),
    )
    .unwrap_err();
    assert!(err.contains("converter is not at"), "{err}");
}
