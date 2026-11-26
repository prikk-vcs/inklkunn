//! RFC 007 D-8 and acceptance criterion 9 (handoff I-T4): the interwiki map is a governance file, and **each rule of it refuses by
//! name** at acceptance, a file in the repository that breaks one is a read fault (the wiki refuses writes), and an import
//! leaves a failing prefix out and lists it. A refusal never repeats what the file said.

use std::collections::BTreeSet;

use super::*;

const SECRET: &str = "ZZ-secret-ZZ";

fn none() -> BTreeSet<String> {
    BTreeSet::new()
}

fn namespaces(titles: &[&str]) -> BTreeSet<String> {
    namespaces_of_titles(titles.iter().copied())
}

fn map(text: &str) -> Interwiki {
    validate_interwiki(text, &none()).expect("a sound map")
}

fn refused(text: &str) -> Fault {
    validate_interwiki(text, &none()).expect_err("a refused map")
}

/// A one-prefix file with `template` as its value, quoted.
fn with(template: &str) -> String {
    format!("wp: \"{template}\"\n")
}

#[test]
fn a_sound_map_is_read_with_its_templates() {
    let m = map("wikipedia: https://en.wikipedia.org/wiki/{title}\n\
         docs: \"https://example.org:8443/docs?page={title}&x=1#top\"\n\
         a-1: https://sub.example.org/{title}\n");
    assert_eq!(m.len(), 3);
    assert_eq!(
        m.get("wikipedia"),
        Some("https://en.wikipedia.org/wiki/{title}")
    );
    assert_eq!(
        m.get("docs"),
        Some("https://example.org:8443/docs?page={title}&x=1#top")
    );
    assert_eq!(m.get("missing"), None);
}

#[test]
fn an_empty_file_is_an_empty_map_and_an_empty_map_is_an_empty_file() {
    for text in ["", "\n", "# nothing here\n", "{}\n"] {
        assert!(map(text).is_empty(), "{text:?}");
    }
    assert_eq!(Interwiki::empty().to_yaml(), "");
}

#[test]
fn the_canonical_text_reads_back_as_the_map_it_was_made_from() {
    let m = map("b: https://b.example/{title}\na: \"https://a.example/q?t={title}\"\n");
    let text = m.to_yaml();
    assert_eq!(
        text,
        "a: \"https://a.example/q?t={title}\"\nb: \"https://b.example/{title}\"\n"
    );
    assert_eq!(map(&text), m);
    assert_eq!(read_interwiki(text.as_bytes()).unwrap(), m);
}

#[test]
fn a_template_that_is_not_https_is_refused() {
    for template in [
        "http://example.org/{title}",
        "HTTPS://example.org/{title}",
        "//example.org/{title}",
        "ftp://example.org/{title}",
        "javascript:alert({title})",
        "example.org/{title}",
        "/wiki/{title}",
        "https:/example.org/{title}",
    ] {
        let f = refused(&with(template));
        assert_eq!(f.refusal, Refusal::TemplateScheme, "{template}");
        assert_eq!(f.name(), "template-not-https");
        assert_eq!(f.field, "template");
        assert_eq!(f.line, Some(1));
        assert_eq!(f.file, Some(GovernanceFile::Interwiki));
    }
}

#[test]
fn a_template_without_a_host_or_with_something_that_is_not_one_is_refused() {
    for template in [
        "https:///wiki/{title}",
        "https://?q={title}",
        "https://exa_mple.org/{title}",
        "https://-example.org/{title}",
        "https://example-.org/{title}",
        "https://example..org/{title}",
        "https://example.org:/{title}",
        "https://example.org:80a/{title}",
        "https://example.org:123456/{title}",
        "https://[::1]/{title}",
        "https://example.org:80:80/{title}",
    ] {
        let f = refused(&with(template));
        assert_eq!(f.refusal, Refusal::TemplateHost, "{template}");
        assert_eq!(f.name(), "template-host");
    }
}

#[test]
fn a_template_with_user_information_is_refused() {
    for template in [
        "https://user@example.org/{title}",
        "https://user:pass@example.org/{title}",
        "https://@example.org/{title}",
        "https://example.org@evil.example/{title}",
    ] {
        let f = refused(&with(template));
        assert_eq!(f.refusal, Refusal::TemplateUserInfo, "{template}");
        assert_eq!(f.name(), "template-userinfo");
    }
    // An at sign in the path is not user information.
    map(&with("https://example.org/@{title}"));
}

#[test]
fn the_placeholder_is_one_title_in_the_path_or_the_query() {
    map(&with("https://example.org/{title}"));
    map(&with("https://example.org/w/{title}/view"));
    map(&with("https://example.org/?title={title}"));
    map(&with("https://example.org/w?a=1&title={title}#fixed"));
    for template in [
        // None, two, and only one of them in place.
        "https://example.org/wiki/",
        "https://example.org/{title}/{title}",
        // Not in the path or the query: the host, the scheme's neighbour, the fragment, the port.
        "https://{title}.example.org/wiki",
        "https://example.org{title}",
        "https://example.org/wiki#{title}",
        "https://example.org:{title}/wiki",
        // Another placeholder, a stray brace, a placeholder that is not whole.
        "https://example.org/{lang}/{title}",
        "https://example.org/{title}}",
        "https://example.org/{{title}",
        "https://example.org/{Title}",
        "https://example.org/%7Btitle%7D",
    ] {
        let f = refused(&with(template));
        assert_eq!(f.refusal, Refusal::TemplatePlaceholder, "{template}");
        assert_eq!(f.name(), "template-placeholder");
    }
}

#[test]
fn a_template_holds_only_the_characters_a_url_is_written_with() {
    for template in [
        "https://example.org/a b/{title}",
        "https://example.org/é/{title}",
        "https://example.org/<{title}>",
        "https://example.org/\\{title}",
        "https://example.org/{title}^",
        "https://example.org/`{title}",
        "https://example.org/{title}|",
        "https://example.org/\u{3000}{title}",
    ] {
        let f = refused(&with(template));
        assert_eq!(f.refusal, Refusal::TemplateCharacter, "{template:?}");
        assert_eq!(f.name(), "template-character");
    }
    // A quote in a plain value is a character of the value, and no character a template may hold.
    assert_eq!(
        refused("wp: https://example.org/\"{title}\n").refusal,
        Refusal::TemplateCharacter
    );
    assert_eq!(
        refused(&with("https://example.org/{ title }")).refusal,
        Refusal::TemplateCharacter
    );
}

#[test]
fn a_template_longer_than_the_bound_is_refused() {
    let long = format!(
        "https://example.org/{}/{{title}}",
        "a".repeat(MAX_TEMPLATE_CHARS)
    );
    let f = refused(&with(&long));
    assert_eq!(f.refusal, Refusal::Bounds);
    assert_eq!(f.field, "template");
    let edge = format!(
        "https://example.org/{}{{title}}",
        "a".repeat(MAX_TEMPLATE_CHARS - "https://example.org/{title}".len())
    );
    assert_eq!(edge.chars().count(), MAX_TEMPLATE_CHARS);
    map(&with(&edge));
}

#[test]
fn a_template_that_is_not_text_is_a_wrong_shape() {
    for text in [
        "wp: 42\n",
        "wp: true\n",
        "wp:\n  - https://example.org/{title}\n",
        "wp:\n  url: https://example.org/{title}\n",
        "wp: [https://example.org/{title}]\n",
        "wp:\n",
    ] {
        let f = refused(text);
        assert_eq!(f.refusal, Refusal::WrongShape, "{text:?}");
        assert_eq!(f.field, "template");
        assert_eq!(f.line, Some(1), "{text:?}");
    }
}

#[test]
fn a_prefix_is_ascii_letters_digits_and_a_hyphen() {
    for good in [
        "wp",
        "WP",
        "a-b",
        "9",
        "-",
        "x".repeat(MAX_PREFIX_CHARS).as_str(),
    ] {
        map(&format!("\"{good}\": https://example.org/{{title}}\n"));
    }
    // A key that cannot be a prefix is a key the schema does not have.
    for bad in ["a_b", "a.b", "a b", "é", "a:b", "a/b", "ａ"] {
        let f = refused(&format!("\"{bad}\": https://example.org/{{title}}\n"));
        assert_eq!(f.refusal, Refusal::UnknownKey, "{bad:?}");
        assert_eq!(f.name(), "unknown-key");
        assert_eq!(f.field, "prefix");
    }
    let empty = refused("\"\": https://example.org/{title}\n");
    assert_eq!(empty.refusal, Refusal::UnknownKey);
}

#[test]
fn a_prefix_longer_than_thirty_two_characters_is_refused() {
    let f = refused(&format!(
        "{}: https://example.org/{{title}}\n",
        "x".repeat(33)
    ));
    assert_eq!(f.refusal, Refusal::Bounds);
    assert_eq!(f.field, "prefix");
    assert_eq!(f.line, Some(1));
}

#[test]
fn a_prefix_that_is_a_namespace_of_the_wiki_is_refused_whatever_the_case() {
    let ns = namespaces(&[
        "Help/Contents",
        "Talk:Foo",
        "Plain",
        "wiki/a/b",
        "Draft:Y/Z",
    ]);
    for prefix in ["Help", "help", "HELP", "Talk", "talk", "wiki", "Draft"] {
        let text = format!("{prefix}: https://example.org/{{title}}\n");
        let f = validate_interwiki(&text, &ns).expect_err(prefix);
        assert_eq!(f.refusal, Refusal::PrefixIsNamespace, "{prefix}");
        assert_eq!(f.name(), "prefix-is-namespace");
        assert_eq!(f.field, "prefix");
    }
    // A page title that is no namespace is no clash, and a prefix beside the namespaces is fine.
    validate_interwiki(
        "Plain: https://example.org/{title}\nother: https://o.example/{title}\n",
        &ns,
    )
    .unwrap();
}

#[test]
fn the_namespaces_of_a_wiki_are_the_directories_and_the_colon_prefixes_of_its_titles() {
    let ns = namespaces(&[
        "Home",
        "Guides/Setup",
        "Help:Contents",
        "Help:Contents/Deep",
        "A:B:C",
        "not a prefix/Page",
        "bad_prefix:Page",
        "/x",
    ]);
    let want: BTreeSet<String> = ["guides", "help", "a"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(ns, want);
}

#[test]
fn reading_a_file_in_the_repository_does_not_judge_the_namespaces() {
    // The clash is judged when a change is accepted: a page made after it must not make the wiki refuse writes.
    let text = "Help: https://example.org/{title}\n";
    let ns = namespaces(&["Help/Contents"]);
    assert_eq!(
        validate_interwiki(text, &ns).unwrap_err().refusal,
        Refusal::PrefixIsNamespace
    );
    assert_eq!(read_interwiki(text.as_bytes()).unwrap().len(), 1);
}

#[test]
fn a_key_written_twice_is_refused() {
    let f = refused("wp: https://a.example/{title}\nwp: https://b.example/{title}\n");
    assert_eq!(f.refusal, Refusal::RepeatedKey);
    assert_eq!(f.line, Some(2));
}

#[test]
fn a_map_longer_than_its_bounds_is_refused() {
    let many: String = (0..=MAX_INTERWIKI_PREFIXES)
        .map(|i| format!("p{i}: https://example.org/{{title}}\n"))
        .collect();
    let f = refused(&many);
    assert_eq!(f.refusal, Refusal::Bounds);
    assert_eq!(f.field, "prefix");
    let ok: String = (0..MAX_INTERWIKI_PREFIXES)
        .map(|i| format!("p{i}: https://example.org/{{title}}\n"))
        .collect();
    assert_eq!(map(&ok).len(), MAX_INTERWIKI_PREFIXES);
    let big = format!("# {}\n", "x".repeat(MAX_INTERWIKI_BYTES));
    assert_eq!(refused(&big).refusal, Refusal::TooLong);
}

#[test]
fn text_outside_the_subset_a_forbidden_character_and_bytes_that_are_not_text_are_refused() {
    for (text, refusal) in [
        ("wp: &a https://example.org/{title}\n", Refusal::WrongShape),
        ("- wp\n", Refusal::WrongShape),
        ("just words\n", Refusal::WrongShape),
        ("wp: \"https://example.org/{title}\n", Refusal::WrongShape),
        (
            "wp: https://example.org/{title}\u{202e}\n",
            Refusal::ForbiddenCharacter,
        ),
        (
            "wp: https://example.org/{title}\u{0}\n",
            Refusal::ForbiddenCharacter,
        ),
        (
            "\u{feff}wp: https://example.org/{title}\n",
            Refusal::ForbiddenCharacter,
        ),
    ] {
        assert_eq!(refused(text).refusal, refusal, "{text:?}");
    }
    let f = validate_interwiki_bytes(&[0xff, 0xfe, b'a'], &none()).unwrap_err();
    assert_eq!(f.refusal, Refusal::NotText);
    assert_eq!(f.file, Some(GovernanceFile::Interwiki));
}

#[test]
fn a_refusal_names_the_line_of_its_prefix_and_never_repeats_the_file() {
    let text = format!(
        "# {SECRET}\nok: https://example.org/{{title}}\n\nbad: \"http://{SECRET}.example/{{title}}\"\n"
    );
    let f = refused(&text);
    assert_eq!(f.refusal, Refusal::TemplateScheme);
    assert_eq!(f.line, Some(4));
    let shown = f.to_string();
    assert!(!shown.contains(SECRET), "{shown}");
    assert_eq!(
        shown,
        "governance/interwiki.yaml, line 4: an interwiki template does not start with https:// (`template`)"
    );
    let key = refused(&format!("{SECRET}_x: https://example.org/{{title}}\n"));
    assert!(!key.to_string().contains(SECRET), "{key}");
}

#[test]
fn a_file_that_cannot_be_read_in_the_repository_is_a_read_fault_that_names_the_file() {
    let fault = read_interwiki(b"wp: http://example.org/{title}\n").unwrap_err();
    assert_eq!(fault.file(), GovernanceFile::Interwiki);
    assert_eq!(fault.refusal(), Refusal::TemplateScheme);
    assert_eq!(fault.line(), Some(1));
    assert!(
        fault
            .to_string()
            .starts_with("governance/interwiki.yaml, line 1: ")
    );
    assert_eq!(
        read_interwiki(&[0xc3, 0x28]).unwrap_err().refusal(),
        Refusal::NotText
    );
}

#[test]
fn the_file_is_a_governance_file_and_a_change_to_it_is_a_policy_change() {
    assert_eq!(
        GovernanceFile::Interwiki.path(),
        "governance/interwiki.yaml"
    );
    assert_eq!(
        GovernanceFile::of_path("governance/interwiki.yaml"),
        Some(GovernanceFile::Interwiki)
    );
    assert_eq!(
        GovernanceFile::at_or_below("governance/interwiki.yaml/x"),
        Some(GovernanceFile::Interwiki)
    );
    assert!(refuse_unknown_files(["governance/interwiki.yaml"]).is_ok());
    assert!(refuse_unknown_files(["governance/interwiki.yml"]).is_err());
    assert_eq!(
        GovernanceChange::of_path("governance/interwiki.yaml"),
        Some(GovernanceChange::Policy)
    );
    assert_eq!(
        GovernanceChange::at_or_below("governance/interwiki.yaml/x"),
        Some(GovernanceChange::Policy)
    );
    assert!(only_governance_files(["governance/interwiki.yaml"]));
    assert!(only_governance_files([
        "governance/interwiki.yaml",
        "governance/policy.yaml"
    ]));
    // A change to the map is its own proposal: a page does not travel with it.
    assert!(mixes_rules_with_other_paths([
        "governance/interwiki.yaml",
        "pages/Home.md"
    ]));
    assert!(
        refuse_outside_default_view(["governance/interwiki.yaml"], ViewPlace::Alternative).is_err()
    );
    assert!(refuse_outside_default_view(["governance/interwiki.yaml"], ViewPlace::Default).is_ok());
    // What a policy change needs is what a change to the map needs.
    let c = Constitution::default();
    assert_eq!(
        GovernanceChange::of_path("governance/interwiki.yaml").map(|k| k.required(&c)),
        GovernanceChange::of_path("governance/policy.yaml").map(|k| k.required(&c)),
    );
}

#[test]
fn an_import_leaves_out_each_prefix_that_fails_and_lists_it_with_the_rule() {
    let ns = namespaces(&["Help:Contents"]);
    let given = [
        ("wikipedia", "https://en.wikipedia.org/wiki/{title}"),
        ("plain", "http://example.org/{title}"),
        ("help", "https://help.example/{title}"),
        ("bad_prefix", "https://example.org/{title}"),
        ("nohost", "https://:80/{title}"),
        ("user", "https://u@example.org/{title}"),
        ("two", "https://example.org/{title}/{title}"),
        ("space", "https://example.org/a b/{title}"),
        ("docs", "https://example.org/docs?page={title}"),
    ]
    .map(|(p, t)| (p.to_owned(), t.to_owned()));
    let (accepted, left) = judge_interwiki(given, &ns);
    let kept: Vec<&str> = accepted.iter().map(|(p, _)| p).collect();
    assert_eq!(kept, ["docs", "wikipedia"]);
    let said: Vec<(&str, Refusal)> = left
        .iter()
        .map(|l| (l.prefix.as_str(), l.refusal))
        .collect();
    assert_eq!(
        said,
        [
            ("plain", Refusal::TemplateScheme),
            ("help", Refusal::PrefixIsNamespace),
            ("bad_prefix", Refusal::UnknownKey),
            ("nohost", Refusal::TemplateHost),
            ("user", Refusal::TemplateUserInfo),
            ("two", Refusal::TemplatePlaceholder),
            ("space", Refusal::TemplateCharacter),
        ]
    );
    // What was accepted reads back whole from the file the import writes.
    assert_eq!(
        read_interwiki(accepted.to_yaml().as_bytes()).unwrap(),
        accepted
    );
}

#[test]
fn an_import_takes_no_more_prefixes_than_a_file_may_hold() {
    let given: Vec<(String, String)> = (0..MAX_INTERWIKI_PREFIXES + 3)
        .map(|i| (format!("p{i:04}"), "https://example.org/{title}".to_owned()))
        .collect();
    let (accepted, left) = judge_interwiki(given, &none());
    assert_eq!(accepted.len(), MAX_INTERWIKI_PREFIXES);
    assert_eq!(left.len(), 3);
    assert!(left.iter().all(|l| l.refusal == Refusal::Bounds));
    assert_eq!(left.first().map(|l| l.prefix.as_str()), Some("p0256"));
    // The canonical text of a full map is within the file's bound.
    assert!(accepted.to_yaml().len() <= MAX_INTERWIKI_BYTES);
    assert_eq!(
        read_interwiki(accepted.to_yaml().as_bytes()).unwrap(),
        accepted
    );
}

#[test]
fn the_same_judgement_serves_the_file_and_the_import() {
    // Every template a file refuses is a prefix an import leaves out, with the same rule.
    for template in [
        "http://example.org/{title}",
        "https://example.org/",
        "https://u@example.org/{title}",
        "https://bad_host/{title}",
        "https://example.org/é{title}",
        "https://{title}.example.org/",
    ] {
        let file = refused(&with(template)).refusal;
        let (accepted, left) = judge_interwiki([("wp".to_owned(), template.to_owned())], &none());
        assert!(accepted.is_empty(), "{template}");
        assert_eq!(left.len(), 1);
        assert_eq!(left.first().map(|l| l.refusal), Some(file), "{template}");
    }
}

// ---------------------------------------------------------------------------------------------------- W-5

#[test]
fn a_title_is_in_a_taken_namespace_by_the_definition_the_prefixes_clash_is_judged_by() {
    // One definition of a namespace, used in both directions: a prefix is refused when a title has it as a namespace, and a title is
    // refused when the map has its namespace as a prefix. They cannot disagree, whatever the title.
    let titles = [
        "Help:Contents",
        "help/Sowing",
        "HELP:Contents/Sub",
        "Gh/Deep/Notes",
        "GH:Notes",
        "Notes",
        "Notes: with a space",
        "A b:c",
        "ghost:Notes",
        "gh",
        "gh/",
        "/gh/x",
        "x/gh:y",
        "gh:y/x",
        "",
        ":x",
        "a:b:c",
    ];
    for prefix in ["help", "gh", "ghost", "a", "x", "notes"] {
        let text = format!("{prefix}: https://example.org/{{title}}\n");
        let m = map(&text);
        for title in titles {
            let is_a_namespace = namespaces(&[title]).contains(prefix);
            assert_eq!(
                m.takes_namespace_of(title).is_some(),
                is_a_namespace,
                "{prefix} / {title:?}"
            );
            // And the clash the map's own rule makes is the same fact.
            assert_eq!(
                validate_interwiki(&text, &namespaces(&[title])).is_err(),
                is_a_namespace,
                "{prefix} / {title:?}"
            );
        }
    }
}

#[test]
fn the_prefix_is_compared_without_the_case_of_ascii_letters_and_is_named_as_the_map_spells_it() {
    let m = map(
        "GH: https://github.com:8443/org/repo/wiki/{title}\nwp: https://en.wikipedia.org/wiki/{title}\n",
    );
    for title in ["gh:Notes", "GH:Notes", "Gh/Notes", "gH/Deep/Notes"] {
        let taken = m.takes_namespace_of(title).expect(title);
        assert_eq!(taken.prefix(), "GH", "{title}");
        assert_eq!(taken.host(), "github.com:8443", "{title}");
    }
    // The first namespace of the title the map has: the directory, then the part before the colon.
    let taken = m
        .takes_namespace_of("wp/gh:x")
        .expect("the directory first");
    assert_eq!(taken.prefix(), "wp");
    assert!(m.takes_namespace_of("Ghost:Notes").is_none());
    assert!(m.takes_namespace_of("Notes").is_none());
    assert!(Interwiki::empty().takes_namespace_of("gh:Notes").is_none());
    // What a surface says holds nothing of the title.
    let said = m
        .takes_namespace_of("gh:ZZ-secret-ZZ")
        .expect("taken")
        .to_string();
    assert!(!said.contains("secret"), "{said}");
}
