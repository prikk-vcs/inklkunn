use super::*;

#[test]
fn text_is_escaped_and_what_xml_cannot_carry_is_dropped() {
    assert_eq!(
        escape("<script>alert(\"x\") & 'y'</script>"),
        "&lt;script&gt;alert(&quot;x&quot;) &amp; &apos;y&apos;&lt;/script&gt;"
    );
    assert_eq!(escape("a\u{0}b\u{8}c\u{b}d\u{1f}e\u{fffe}f"), "abcdef");
    assert_eq!(escape("tab\tnewline\nreturn\r"), "tab\tnewline\nreturn\r");
    assert_eq!(escape("日本語 é"), "日本語 é");
}

#[test]
fn a_feed_names_its_own_address_and_carries_every_entry_escaped() {
    let feed = Feed {
        lang: "en",
        id: "urn:inklkunn:feed:1".into(),
        title: "Recent <changes>".into(),
        updated: 1_700_000_000,
        self_href: "http://x.example/w/-/changes.atom?a=1&b=2".into(),
        alternate_href: "http://x.example/w/-/changes".into(),
        author: "The Wiki".into(),
        version: "0.0.0",
        entries: vec![Entry {
            id: "urn:inklkunn:1:b".into(),
            title: "<img src=x onerror=alert(1)>".into(),
            updated: 1_700_000_001,
            href: "http://x.example/w/Home".into(),
            related: Some("http://x.example/w/-/diff/a/b/Home".into()),
            author: "Aiko & Ben".into(),
            summary: Some("Pages: Home".into()),
            categories: vec!["minor", "automated"],
        }],
    };
    let xml = feed.render();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed "));
    assert!(xml.contains("<title>Recent &lt;changes&gt;</title>"));
    assert!(xml.contains("href=\"http://x.example/w/-/changes.atom?a=1&amp;b=2\""));
    assert!(xml.contains("<title>&lt;img src=x onerror=alert(1)&gt;</title>"));
    assert!(!xml.contains("<img"));
    assert!(xml.contains("<name>Aiko &amp; Ben</name>"));
    // The flags are categories a reader can filter on.
    assert!(
        xml.contains("<category term=\"minor\"/>")
            && xml.contains("<category term=\"automated\"/>")
    );
    assert!(xml.contains("<updated>2023-11-14T22:13:20Z</updated>"));
    assert!(xml.ends_with("</feed>\n"));
}
