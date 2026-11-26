//! A draft of the rules: what the policy form (RFC 006 D-4; handoff G-T7) sends, as text the validator reads.
//!
//! The form shows each key as a sentence and a control, never as YAML. What it sends is a [`PolicyDraft`] (or a
//! [`ConstitutionDraft`]): one value for each key, as the control gave it. A draft is turned into **the text of a
//! governance file**, which is then judged by the very reader acceptance uses ([`validate_policy`](super::validate_policy),
//! [`validate_constitution`](super::validate_constitution)); what that reader accepts is turned into the canonical text
//! ([`Policy::to_yaml`](super::Policy::to_yaml)) the proposal carries. So **the form decides nothing about the rules**: it
//! writes a candidate, and the one validator says whether it is one.
//!
//! **A word or a text a person typed is never written as YAML syntax.** Every word and every text is written as a
//! double-quoted scalar (`quoted`), so no value can end its line, open a key or begin a comment; a value that is not one of
//! the words its key allows is refused by the validator as [`Refusal::UnknownValue`](super::Refusal), and a text with a
//! control character in it as [`Refusal::ForbiddenCharacter`](super::Refusal), at the key it was typed in. Numbers are whole
//! numbers the caller has already read from what was typed.
//!
//! The schema's own field names (what a [`Fault`](super::Fault) is about) are listed in [`fields`], so that a surface can map
//! each to the control it concerns and a test can hold that it maps every one.

use inklkunn_store::Role;

use super::chars::quoted;
use super::interwiki::{PREFIX_FIELD, TEMPLATE_FIELD};
use super::protect::{Require, Target};
use super::{Constitution, Interwiki, Policy};

/// The fields of the schema, as a [`Fault`](super::Fault) names them: one list, in the order of the files.
pub mod fields {
    /// `entry`.
    pub const ENTRY: &str = "entry";
    /// `rungs.comment`.
    pub const COMMENT: &str = "rungs.comment";
    /// `rungs.propose`.
    pub const PROPOSE: &str = "rungs.propose";
    /// `rungs.review`.
    pub const REVIEW: &str = "rungs.review";
    /// `rungs.create_view`.
    pub const CREATE_VIEW: &str = "rungs.create_view";
    /// `rungs.accept`.
    pub const ACCEPT: &str = "rungs.accept";
    /// `rungs.moderate`.
    pub const MODERATE: &str = "rungs.moderate";
    /// `rungs.decide_appeal`.
    pub const DECIDE_APPEAL: &str = "rungs.decide_appeal";
    /// `rungs.grant_up_to`.
    pub const GRANT_UP_TO: &str = "rungs.grant_up_to";
    /// `acceptance.approvals`.
    pub const APPROVALS: &str = "acceptance.approvals";
    /// `acceptance.self`.
    pub const SELF_ACCEPTANCE: &str = "acceptance.self";
    /// `knowledge.review_after`.
    pub const REVIEW_AFTER: &str = "knowledge.review_after";
    /// `protect` (the list as a whole).
    pub const PROTECT: &str = "protect";
    /// `protect.path`.
    pub const PROTECT_PATH: &str = "protect.path";
    /// `protect.namespace`.
    pub const PROTECT_NAMESPACE: &str = "protect.namespace";
    /// `protect.require`.
    pub const PROTECT_REQUIRE: &str = "protect.require";
    /// `protect.require.rung`.
    pub const PROTECT_REQUIRE_RUNG: &str = "protect.require.rung";
    /// `protect.reason`.
    pub const PROTECT_REASON: &str = "protect.reason";
    /// `policy_change.rung`.
    pub const POLICY_CHANGE_RUNG: &str = "policy_change.rung";
    /// `policy_change.approvals`.
    pub const POLICY_CHANGE_APPROVALS: &str = "policy_change.approvals";
    /// `constitution_change.approvals`.
    pub const CONSTITUTION_CHANGE_APPROVALS: &str = "constitution_change.approvals";

    /// The field of the promotion text of a rung (`promotion.<rung>`).
    pub const fn promotion(role: inklkunn_store::Role) -> &'static str {
        use inklkunn_store::Role;
        match role {
            Role::Observer => "promotion.observer",
            Role::Questioner => "promotion.questioner",
            Role::Contributor => "promotion.contributor",
            Role::Reviewer => "promotion.reviewer",
            Role::Maintainer => "promotion.maintainer",
            Role::Admin => "promotion.admin",
        }
    }

    /// Every field a draft of the policy can be refused at, except the six promotion texts ([`promotion`]).
    pub const POLICY: [&str; 17] = [
        ENTRY,
        COMMENT,
        PROPOSE,
        REVIEW,
        CREATE_VIEW,
        ACCEPT,
        MODERATE,
        DECIDE_APPEAL,
        GRANT_UP_TO,
        APPROVALS,
        SELF_ACCEPTANCE,
        REVIEW_AFTER,
        PROTECT,
        PROTECT_PATH,
        PROTECT_NAMESPACE,
        PROTECT_REQUIRE,
        PROTECT_REQUIRE_RUNG,
    ];

    /// `prefix`: a prefix of the interwiki map (every key of that file is one).
    pub const INTERWIKI_PREFIX: &str = super::PREFIX_FIELD;
    /// `template`: the address template a prefix of the interwiki map leads to.
    pub const INTERWIKI_TEMPLATE: &str = super::TEMPLATE_FIELD;

    /// Every field a draft of the interwiki map can be refused at: the prefix and the template of a row.
    pub const INTERWIKI: [&str; 2] = [INTERWIKI_PREFIX, INTERWIKI_TEMPLATE];

    /// Every field a draft of the constitution can be refused at.
    pub const CONSTITUTION: [&str; 3] = [
        POLICY_CHANGE_RUNG,
        POLICY_CHANGE_APPROVALS,
        CONSTITUTION_CHANGE_APPROVALS,
    ];
}

/// One standing protection as a form sends it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtectDraft {
    /// A whole namespace (every page below the title), or the one page.
    pub namespace: bool,
    /// The title, as typed.
    pub title: String,
    /// What it requires: `review`, or a rung's word.
    pub require: String,
    /// Why, as typed.
    pub reason: String,
}

/// The policy as a form sends it: one value for each key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDraft {
    /// `entry`: a word.
    pub entry: String,
    /// `rungs.comment`: a rung's word.
    pub comment: String,
    /// `rungs.propose`.
    pub propose: String,
    /// `rungs.review`.
    pub review: String,
    /// `rungs.create_view`.
    pub create_view: String,
    /// `rungs.accept`.
    pub accept: String,
    /// `rungs.moderate`.
    pub moderate: String,
    /// `rungs.decide_appeal`.
    pub decide_appeal: String,
    /// `rungs.grant_up_to`.
    pub grant_up_to: String,
    /// `acceptance.approvals`.
    pub approvals: i64,
    /// `acceptance.self`.
    pub self_acceptance: String,
    /// `knowledge.review_after`, in days; `None` leaves it to the operator's setting.
    pub review_after: Option<i64>,
    /// `promotion.<rung>`: the text for each rung that has one (an empty text is no text and is left out).
    pub promotion: Vec<(Role, String)>,
    /// `protect`.
    pub protect: Vec<ProtectDraft>,
}

impl PolicyDraft {
    /// The draft that says what `policy` says: what the form is filled with.
    pub fn of(policy: &Policy) -> PolicyDraft {
        let r = policy.rungs();
        PolicyDraft {
            entry: policy.entry().word().to_owned(),
            comment: r.comment().as_str().to_owned(),
            propose: r.propose().as_str().to_owned(),
            review: r.review().as_str().to_owned(),
            create_view: r.create_view().as_str().to_owned(),
            accept: r.accept().as_str().to_owned(),
            moderate: r.moderate().as_str().to_owned(),
            decide_appeal: r.decide_appeal().word().to_owned(),
            grant_up_to: r.grant_up_to().word().to_owned(),
            approvals: i64::from(policy.acceptance().approvals()),
            self_acceptance: policy.acceptance().self_acceptance().word().to_owned(),
            review_after: policy.review_after().map(i64::from),
            promotion: policy
                .promotions()
                .map(|(role, text)| (role, text.to_owned()))
                .collect(),
            protect: policy
                .protections()
                .iter()
                .map(|p| ProtectDraft {
                    namespace: matches!(p.target(), Target::Namespace(_)),
                    title: p.target().title().to_owned(),
                    require: match p.require() {
                        Require::Review => "review".to_owned(),
                        Require::Rung(role) => role.as_str().to_owned(),
                    },
                    reason: p.reason().to_owned(),
                })
                .collect(),
        }
    }

    /// The text of a `governance/policy.yaml` that says what the draft says, for the validator to read: every key, each
    /// word and text double-quoted. **It is a candidate, not a policy**: only [`validate_policy`](super::validate_policy)
    /// says whether it is one.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("entry: {}\n", quoted(&self.entry)));
        out.push_str("rungs:\n");
        for (key, value) in [
            ("comment", &self.comment),
            ("propose", &self.propose),
            ("review", &self.review),
            ("create_view", &self.create_view),
            ("accept", &self.accept),
            ("moderate", &self.moderate),
            ("decide_appeal", &self.decide_appeal),
            ("grant_up_to", &self.grant_up_to),
        ] {
            out.push_str(&format!("  {key}: {}\n", quoted(value)));
        }
        out.push_str("acceptance:\n");
        out.push_str(&format!("  approvals: {}\n", self.approvals));
        out.push_str(&format!("  self: {}\n", quoted(&self.self_acceptance)));
        if let Some(days) = self.review_after {
            out.push_str(&format!("knowledge:\n  review_after: {days}\n"));
        }
        let promotion: Vec<&(Role, String)> = self
            .promotion
            .iter()
            .filter(|(_, text)| !text.trim().is_empty())
            .collect();
        if !promotion.is_empty() {
            out.push_str("promotion:\n");
            for (role, text) in promotion {
                out.push_str(&format!("  {}: {}\n", role.as_str(), quoted(text)));
            }
        }
        if !self.protect.is_empty() {
            out.push_str("protect:\n");
            for p in &self.protect {
                let key = if p.namespace { "namespace" } else { "path" };
                out.push_str(&format!("  - {key}: {}\n", quoted(&p.title)));
                if p.require == "review" {
                    out.push_str("    require: review\n");
                } else {
                    out.push_str(&format!(
                        "    require:\n      rung: {}\n",
                        quoted(&p.require)
                    ));
                }
                out.push_str(&format!("    reason: {}\n", quoted(&p.reason)));
            }
        }
        out
    }
}

/// The constitution as a form sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstitutionDraft {
    /// `policy_change.rung`: `maintainer` or `admin`.
    pub policy_rung: String,
    /// `policy_change.approvals`.
    pub policy_approvals: i64,
    /// `constitution_change.approvals`.
    pub constitution_approvals: i64,
}

impl ConstitutionDraft {
    /// The draft that says what `constitution` says.
    pub fn of(constitution: &Constitution) -> ConstitutionDraft {
        ConstitutionDraft {
            policy_rung: constitution.policy_change().rung.as_str().to_owned(),
            policy_approvals: i64::from(constitution.policy_change().approvals),
            constitution_approvals: i64::from(constitution.constitution_change().approvals),
        }
    }

    /// The text of a `governance/constitution.yaml` that says what the draft says (a candidate, as [`PolicyDraft::to_text`]).
    pub fn to_text(&self) -> String {
        format!(
            "policy_change:\n  rung: {}\n  approvals: {}\nconstitution_change:\n  approvals: {}\n",
            quoted(&self.policy_rung),
            self.policy_approvals,
            self.constitution_approvals,
        )
    }
}

/// The interwiki map as a form sends it: **one row for each prefix, the prefix and its address template as typed** (the form
/// trims them). The file has no key of its own, so there is nothing else to say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InterwikiDraft {
    /// The rows, in the order the form lists them: `(prefix, template)`.
    pub rows: Vec<(String, String)>,
}

impl InterwikiDraft {
    /// The draft that says what `map` says: what the form is filled with, by prefix.
    pub fn of(map: &Interwiki) -> InterwikiDraft {
        InterwikiDraft {
            rows: map
                .iter()
                .map(|(prefix, template)| (prefix.to_owned(), template.to_owned()))
                .collect(),
        }
    }

    /// The text of a `governance/interwiki.yaml` that says what the draft says, for the validator to read
    /// ([`validate_interwiki`](super::validate_interwiki)): **each prefix and each template double-quoted**, so that nothing a
    /// person typed is YAML syntax. A prefix that holds a character no prefix has is therefore a key the validator refuses
    /// ([`Refusal::UnknownKey`](super::Refusal)), never a second key. A candidate, not a map: the empty draft is the empty text,
    /// an empty map.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (prefix, template) in &self.rows {
            out.push_str(&format!("{}: {}\n", quoted(prefix), quoted(template)));
        }
        out
    }
}
