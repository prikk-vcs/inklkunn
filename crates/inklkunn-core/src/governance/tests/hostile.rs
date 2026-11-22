//! Hostile and accidental input: huge numbers, deep nesting, control and direction characters, repeated keys, anchors
//! and tags, a byte-order mark, line endings, and every one-character change of a valid file. Whatever the input, the
//! answer is a policy or a typed fault, never a panic, and a fault never repeats the file.

use super::*;

const VALID: &str = "entry: observer\nrungs:\n  comment: observer\n  propose: contributor\n  review: reviewer\n  create_view: reviewer\n  accept: maintainer\n  moderate: maintainer\n  decide_appeal: admin\n  grant_up_to: reviewer\nacceptance:\n  approvals: 2\n  self: never\nknowledge:\n  review_after: 365\npromotion:\n  contributor: \"A few accepted edits.\"\nprotect:\n  - namespace: Policies\n    require: review\n    reason: \"Rules are read.\"\n  - path: Home\n    require:\n      rung: admin\n    reason: \"One hand.\"\n";

fn judge(text: &str) -> Result<Policy, Fault> {
    validate_policy(text, &PlatformLimits::default())
}

#[test]
fn the_valid_file_is_valid() {
    judge(VALID).unwrap();
}

#[test]
fn a_hostile_table_is_refused_by_name_and_never_panics() {
    let deep = format!("a: {}x{}\n", "[".repeat(5_000), "]".repeat(5_000));
    let deep_blocks: String = (0..200)
        .map(|i| format!("{}k{i}:\n", "  ".repeat(i)))
        .collect();
    let many_keys: String = (0..20_000).map(|i| format!("k{i}: 1\n")).collect();
    let cases: Vec<(String, Refusal)> = vec![
        // Numbers.
        (
            "acceptance:\n  approvals: 99999999999999999999999999\n".into(),
            Refusal::WrongShape,
        ),
        (
            "acceptance:\n  approvals: 9223372036854775808\n".into(),
            Refusal::WrongShape,
        ),
        (
            "acceptance:\n  approvals: -9223372036854775808\n".into(),
            Refusal::OutOfRange,
        ),
        (
            "acceptance:\n  approvals: 1e3\n".into(),
            Refusal::WrongShape,
        ),
        (
            "acceptance:\n  approvals: 0x2\n".into(),
            Refusal::WrongShape,
        ),
        ("acceptance:\n  approvals: 02\n".into(), Refusal::WrongShape),
        (
            "acceptance:\n  approvals: .nan\n".into(),
            Refusal::WrongShape,
        ),
        (
            "knowledge:\n  review_after: 99999999999\n".into(),
            Refusal::OutOfRange,
        ),
        (
            "knowledge:\n  review_after: 2026-11-19\n".into(),
            Refusal::WrongShape,
        ),
        // Nesting.
        (deep, Refusal::WrongShape),
        (deep_blocks, Refusal::WrongShape),
        // Anchors, aliases, tags, documents, merge keys.
        ("entry: &a none\nrungs: *a\n".into(), Refusal::WrongShape),
        ("entry: !!str none\n".into(), Refusal::WrongShape),
        ("rungs:\n  <<: *x\n".into(), Refusal::WrongShape),
        // Repeated keys.
        ("entry: none\nentry: none\n".into(), Refusal::RepeatedKey),
        (
            "rungs: {accept: admin, accept: admin}\n".into(),
            Refusal::RepeatedKey,
        ),
        // Characters.
        ("entry: none\u{0}\n".into(), Refusal::ForbiddenCharacter),
        ("entry: none\u{7}\n".into(), Refusal::ForbiddenCharacter),
        ("entry: none \u{202e}\n".into(), Refusal::ForbiddenCharacter),
        (
            "# \u{2066}hidden\u{2069}\nentry: none\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        (
            "entry: none # \u{200f}\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        (
            "entry: none\u{2028}rungs: {}\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        ("entry: none\u{2029}\n".into(), Refusal::ForbiddenCharacter),
        ("\u{feff}entry: none\n".into(), Refusal::ForbiddenCharacter),
        ("entry: none\u{feff}\n".into(), Refusal::ForbiddenCharacter),
        ("entry: none\u{61c}\n".into(), Refusal::ForbiddenCharacter),
        (
            "promotion:\n  admin: \"a\\u202eb\"\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        (
            "promotion:\n  admin: \"a\\u0000b\"\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        (
            "promotion:\n  admin: \"a\\tb\"\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        (
            "promotion:\n  admin: \"a\\u2028b\"\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        // A lone carriage return in a value.
        (
            "entry: none\rrungs: {}\n".into(),
            Refusal::ForbiddenCharacter,
        ),
        // Size.
        (
            format!("entry: none\n# {}\n", "x".repeat(MAX_POLICY_BYTES)),
            Refusal::TooLong,
        ),
        (many_keys, Refusal::TooLong),
    ];
    for (text, refusal) in cases {
        let f = judge(&text).expect_err(&text.chars().take(40).collect::<String>());
        assert_eq!(
            f.refusal,
            refusal,
            "{:?}",
            text.chars().take(60).collect::<String>()
        );
        assert_eq!(f.file, Some(GovernanceFile::Policy));
        assert!(
            f.to_string().len() < 400,
            "a fault is a sentence, not the file"
        );
    }
}

#[test]
fn line_endings_and_a_trailing_comment_are_read() {
    let crlf = VALID.replace('\n', "\r\n");
    assert_eq!(judge(&crlf).unwrap(), judge(VALID).unwrap());
    // A file without a final newline, with comments and blank lines.
    let comments = format!("# the rules\n\n{}\n# end", VALID.trim_end());
    assert_eq!(judge(&comments).unwrap(), judge(VALID).unwrap());
    // A tab in a comment is not a forbidden character.
    assert!(judge("entry: none # a\ttab\n").is_ok());
}

#[test]
fn a_size_bound_is_exact() {
    let pad = |n: usize| format!("entry: none\n#{}\n", "x".repeat(n));
    let base = pad(0).len();
    assert!(judge(&pad(MAX_POLICY_BYTES - base)).is_ok());
    assert_eq!(
        judge(&pad(MAX_POLICY_BYTES - base + 1))
            .unwrap_err()
            .refusal,
        Refusal::TooLong
    );
    let c = |n: usize| format!("#{}\n", "x".repeat(n));
    assert!(validate_constitution(&c(MAX_CONSTITUTION_BYTES - 2)).is_ok());
    assert_eq!(
        validate_constitution(&c(MAX_CONSTITUTION_BYTES - 1))
            .unwrap_err()
            .refusal,
        Refusal::TooLong
    );
}

/// A small deterministic generator, so the property tests need no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        *items
            .get(self.below(items.len()))
            .expect("an item of a non-empty list")
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n).unwrap()).unwrap()
    }
}

#[test]
fn a_valid_file_with_one_character_changed_is_valid_or_a_typed_fault() {
    let chars: Vec<char> = VALID.chars().collect();
    let replacements = [
        ' ', ':', '-', '#', '"', '\'', '{', '}', '[', ']', '&', '*', '!', '\n', '\t', '\u{0}',
        '\u{202e}', '9', 'z', '日', ',', '|', '>', '%', '@', '`', '\\',
    ];
    let mut valid = 0;
    let mut refused = 0;
    for i in 0..chars.len() {
        // Delete it, and replace it with each of the replacements.
        let mut variants: Vec<String> = Vec::new();
        let without: String = chars
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, c)| *c)
            .collect();
        variants.push(without);
        for r in replacements {
            let mut v = chars.clone();
            if let Some(slot) = v.get_mut(i) {
                *slot = r;
            }
            variants.push(v.into_iter().collect());
        }
        for text in variants {
            match judge(&text) {
                Ok(p) => {
                    valid += 1;
                    // Whatever is read is a policy that obeys the rules and writes back as itself.
                    let r = p.rungs();
                    assert!(r.propose() <= r.review() && r.review() <= r.accept());
                    assert!(r.moderate() >= Role::Reviewer);
                    assert!(p.acceptance().approvals() <= MAX_APPROVALS);
                    assert_eq!(judge(&p.to_yaml()).unwrap(), p);
                }
                Err(f) => {
                    refused += 1;
                    assert_eq!(f.file, Some(GovernanceFile::Policy));
                    assert!(!f.name().is_empty());
                    assert!(f.to_string().starts_with("governance/policy.yaml"));
                    if let Some(line) = f.line {
                        assert!(
                            line >= 1 && line <= text.lines().count().max(1) + 1,
                            "line {line}"
                        );
                    }
                }
            }
        }
    }
    assert!(
        valid > 0 && refused > valid,
        "{valid} valid, {refused} refused"
    );
}

#[test]
fn random_line_soups_never_panic_and_a_fault_never_repeats_the_file() {
    let words = [
        "entry",
        "rungs",
        "acceptance",
        "protect",
        "path",
        "namespace",
        "require",
        "reason",
        "rung",
        "comment",
        "accept",
        "none",
        "admin",
        "review",
        "x",
        "1",
        "-1",
        "300",
        "true",
        "~",
        "[",
        "]",
        "{",
        "}",
        "a: b",
        "- ",
        "  ",
        "    ",
        ": ",
        "\"",
        "#",
        "SECRET-WORD",
    ];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..4_000 {
        let mut text = String::new();
        for _ in 0..rng.below(12) {
            for _ in 0..rng.below(6) {
                text.push_str(rng.pick(&words));
                if rng.below(3) > 0 {
                    text.push(if rng.below(2) == 0 { ' ' } else { ':' });
                }
            }
            text.push('\n');
        }
        for result in [
            judge(&text).map(|_| ()),
            validate_constitution(&text).map(|_| ()),
        ] {
            if let Err(f) = result {
                assert!(!f.to_string().contains("SECRET-WORD"), "{text:?}: {f}");
            }
        }
        let _ = read_policy(text.as_bytes());
        let _ = read_constitution(text.as_bytes());
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..2_000 {
        let n = rng.below(300);
        let bytes: Vec<u8> = (0..n)
            .map(|_| u8::try_from(rng.below(256)).unwrap())
            .collect();
        let _ = read_policy(&bytes);
        let _ = read_constitution(&bytes);
        let _ = validate_policy_bytes(&bytes, &PlatformLimits::default());
        let _ = refuse_outside_default_view(
            [String::from_utf8_lossy(&bytes).as_ref()],
            ViewPlace::Alternative,
        );
        let _ = Target::namespace(&String::from_utf8_lossy(&bytes));
    }
}

#[test]
fn generated_policies_are_read_back_as_themselves() {
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    let roles = Role::ALL;
    for _ in 0..500 {
        let pick = |rng: &mut Rng, lo: Role, hi: Role| {
            let span: Vec<Role> = roles
                .iter()
                .copied()
                .filter(|r| *r >= lo && *r <= hi)
                .collect();
            rng.pick(&span)
        };
        let propose = pick(&mut rng, Role::Observer, Role::Admin);
        let review = pick(&mut rng, propose, Role::Admin);
        let accept = pick(&mut rng, review, Role::Admin);
        let moderate = pick(&mut rng, Role::Reviewer, Role::Admin);
        let comment = pick(&mut rng, Role::Observer, Role::Admin);
        let create_view = pick(&mut rng, Role::Observer, Role::Admin);
        let entry = rng.pick(Entry::ALL);
        let appeal = rng.pick(AppealRung::ALL);
        let grant = rng.pick(GrantUpTo::ALL);
        let text = format!(
            "entry: {}\nrungs:\n  comment: {}\n  propose: {}\n  review: {}\n  create_view: {}\n  accept: {}\n  moderate: {}\n  decide_appeal: {}\n  grant_up_to: {}\nacceptance:\n  approvals: {}\n  self: {}\n",
            entry.word(),
            comment.as_str(),
            propose.as_str(),
            review.as_str(),
            create_view.as_str(),
            accept.as_str(),
            moderate.as_str(),
            appeal.word(),
            grant.word(),
            rng.below(6),
            rng.pick(SelfAcceptance::ALL).word(),
        );
        let p = judge(&text).expect(&text);
        assert_eq!(judge(&p.to_yaml()).unwrap(), p);
        assert_eq!(p.entry(), entry);
        assert_eq!(p.rungs().accept(), accept);
    }
}
