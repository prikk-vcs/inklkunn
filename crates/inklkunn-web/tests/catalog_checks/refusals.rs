//! **A refusal is never in a template** (threat model C-3g: protections and restrictions are decided in the core, never in a template,
//! and each refusal names its cause; readiness gap 4): no template says a sentence of a refusal, and no template decides anything.
//! The words come from the catalog and what is said comes from the code that decided; a template shows a presence (`if let Some`,
//! `is_empty`) and nothing else. (The scan of every literal of a template is `template_scan.rs`; this holds the refusals' sentences
//! and the vocabulary of a decision by name, so that the reason a template may not say them is said where it is held.)

use inklkunn_web::catalog::languages;

use super::{file_name, read, templates};

/// The keys of the messages that say a refusal, a restriction, a protection or a problem a person is told: by the area they are in, or
/// by a word of the key that says what they are.
fn is_a_refusal(key: &str) -> bool {
    [
        "outcome.",
        "outcome_note.",
        "refused.",
        "protect.",
        "protection.",
        "governance.",
        "problem.",
        "quota.",
        "restriction.",
    ]
    .iter()
    .any(|prefix| key.starts_with(prefix))
        || ["not_allowed", "refusal", "unavailable"]
            .iter()
            .any(|word| key.contains(word))
}

/// The stretches of text of a message between its placeholders that are long enough to be a sentence's own words.
fn stretches(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let (before, after) = match rest.split_once('{') {
            Some((before, after)) => (before, after.split_once('}').map(|(_, a)| a)),
            None => (rest, None),
        };
        // Each sentence of the stretch is a sentence of a refusal on its own: a template that says one of them is found.
        for sentence in before.split(". ").flat_map(|s| s.split('。')) {
            let words = sentence.trim().trim_end_matches('.');
            if words.chars().count() >= 16 {
                out.push(words.to_owned());
            }
        }
        match after {
            Some(after) => rest = after,
            None => break,
        }
    }
    out
}

/// The sentences of the refusals, in every language the site speaks.
fn refusal_sentences() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for language in languages() {
        for entry in language.entries.iter().filter(|e| is_a_refusal(e.key)) {
            for text in [entry.one, entry.other] {
                for stretch in stretches(text) {
                    out.push((entry.key.to_owned(), stretch));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The refusal sentences a template says in its own text.
fn sentences_said_in(template: &str, sentences: &[(String, String)]) -> Vec<(String, String)> {
    sentences
        .iter()
        .filter(|(_, stretch)| template.contains(stretch.as_str()))
        .cloned()
        .collect()
}

/// What a template does with a value that is a decision: a comparison, or a name of the vocabulary of authority. `role=` of an
/// attribute and the loop's own `role.` of a row are not it; the checks are on the text of the tags `{% … %}` and `{{ … }}` alone.
const DECISION_WORDS: &[&str] = &[
    "==",
    "!=",
    ">=",
    "<=",
    "Role",
    "Action",
    "Scope",
    "Rung",
    "authorize",
    "minimum",
    "may_",
    "is_allowed",
    "is_restricted",
    "is_protected",
];

/// The text of every tag of a template.
fn tags(template: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for (open, close) in [("{%", "%}"), ("{{", "}}")] {
        let mut rest = template;
        while let Some((_, after)) = rest.split_once(open) {
            let Some((tag, tail)) = after.split_once(close) else {
                break;
            };
            out.push(tag);
            rest = tail;
        }
    }
    out
}

/// A tag without the names of catalog messages (`Key::SearchScope` names a sentence, not a scope).
fn without_keys(tag: &str) -> String {
    let mut out = String::new();
    let mut rest = tag;
    while let Some((before, after)) = rest.split_once("Key::") {
        out.push_str(before);
        rest = after.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
    }
    out.push_str(rest);
    out
}

fn decisions_in(template: &str) -> Vec<String> {
    tags(template)
        .into_iter()
        .filter(|tag| {
            let tag = without_keys(tag);
            DECISION_WORDS.iter().any(|word| tag.contains(word))
        })
        .map(|tag| tag.trim().to_owned())
        .collect()
}

#[test]
fn no_template_says_a_sentence_of_a_refusal_in_any_language() {
    let sentences = refusal_sentences();
    assert!(
        sentences.len() >= 60,
        "the scan holds only {} stretches of refusals: are the catalogs being read?",
        sentences.len()
    );
    let mut said = Vec::new();
    let files = templates();
    assert!(files.len() >= 20, "the templates were not found");
    for file in files {
        for (key, stretch) in sentences_said_in(&read(&file), &sentences) {
            said.push(format!("{}: {key}: {stretch}", file_name(&file)));
        }
    }
    assert!(
        said.is_empty(),
        "a template says a refusal itself: {said:#?}"
    );
}

#[test]
fn no_template_decides_anything_a_template_shows_a_presence_and_nothing_more() {
    let mut decided = Vec::new();
    for file in templates() {
        for tag in decisions_in(&read(&file)) {
            decided.push(format!("{}: {tag}", file_name(&file)));
        }
    }
    assert!(
        decided.is_empty(),
        "a template compares or names the vocabulary of authority (decide in the core, and show what it said): {decided:#?}"
    );
}

#[test]
fn the_scans_of_the_refusals_are_shown_able_to_fail() {
    let sentences = refusal_sentences();
    // A template that says the refusal of a protection in its own words.
    let planted = "<p>A change here is accepted only after someone other than the proposer has approved it.</p>";
    let found = sentences_said_in(planted, &sentences);
    assert!(
        !found.is_empty(),
        "a refusal said in a template is not found"
    );
    // A template that decides.
    for planted in [
        "{% if account.role == \"admin\" %}<p>yes</p>{% endif %}",
        "{% if rung >= Role::Maintainer %}x{% endif %}",
        "{{ Action::Moderate }}",
    ] {
        assert!(
            !decisions_in(planted).is_empty(),
            "a decision in a template is not found: {planted}"
        );
    }
    // And a template that only shows is let alone.
    assert!(
        decisions_in("{% if let Some(text) = note %}<p class=\"note\" role=\"status\">{{ text }}</p>{% endif %}")
            .is_empty()
    );
}
