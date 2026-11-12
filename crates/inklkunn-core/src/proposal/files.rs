//! What the files of a proposal may be and do (corrections P4 and P5): **the service's own rules**, applied to
//! every edit that reaches it, whatever surface sent it and including a rollback plan. A surface keeps its
//! checks as the first line (they say the same thing in its own words, earlier); these are the ones that hold.
//!
//! **Paths (P4).** Every file path and both ends of every rename are `pages/<path>.md`, ASCII, with no empty,
//! `.` or `..` part ([`inklkunn_markup::path::is_inside_pages`]). A path that is not in the repository at the
//! proposal's base is a *new* page, and a new page's path is exactly what its title gives
//! (`title_to_path(path_to_title(path))`), so that no two titles share a path and no path is a spelling the
//! wiki never writes. A new path may not collide on a case-insensitive file system (`collision_key`) with a
//! page that stays at the base, or with another new path of the same proposal.
//!
//! **Owned keys (P5).** The information at the top of a page has keys only the wiki's administrators may set
//! ([`OWNED_KEYS`]: `kind`, `visibility`, `system`). Any change to one counts: a new value, a removed key, a page
//! that carries one removed, a page that carries one renamed. The exception is the ruling D8: `kind: redirect` is
//! ordinary wiki work, so a page may become a redirect, or stop being one, for anyone who may propose.

use std::collections::{BTreeMap, BTreeSet};

use inklkunn_markup::frontmatter;
use inklkunn_markup::path::{collision_key, is_inside_pages, path_to_title, title_to_path};
use inklkunn_store::{RevisionContent, WikiRecord};

use super::{Action, Invalid, Principal, ProposalError, ProposalService, Resource, Result};

/// The top-level keys of a page's information block that only an administrator may change.
pub const OWNED_KEYS: [&str; 3] = ["kind", "visibility", "system"];

/// The one `kind` anyone who may propose can set or clear (ruling D8), as the JSON text [`front_keys`] gives.
const FREE_KIND: &str = "\"redirect\"";

/// The top-level keys of the information at the top of `text` and what each is set to (as JSON text); empty
/// when the page has none, `None` when the information is not in the subset the wiki reads.
pub fn front_keys(text: &str) -> Option<BTreeMap<String, String>> {
    let (yaml, _, _) = frontmatter::split(text);
    let Some(yaml) = yaml else {
        return Some(BTreeMap::new());
    };
    match frontmatter::parse_yaml(yaml).ok()? {
        frontmatter::Value::Map(pairs) => Some(
            pairs
                .into_iter()
                .map(|(k, v)| (k, v.to_json().to_string()))
                .collect(),
        ),
        _ => Some(BTreeMap::new()),
    }
}

/// The owned keys that differ between `before` and `after` (keys as [`front_keys`] gives them), except a `kind`
/// that was and is either absent or `redirect` (ruling D8). A key left as it was is not a change.
pub fn changed_owned_keys(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<&'static str> {
    let free = |v: Option<&String>| v.is_none_or(|v| v == FREE_KIND);
    OWNED_KEYS
        .into_iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .filter(|k| !(*k == "kind" && free(before.get(*k)) && free(after.get(*k))))
        .collect()
}

/// Is `path` a page path at all: inside `pages/` and a `.md` file?
pub fn is_page_path(path: &str) -> bool {
    is_inside_pages(path) && path.ends_with(inklkunn_markup::path::EXTENSION)
}

/// Is `path` exactly what the title it spells gives?
fn is_canonical(path: &str) -> bool {
    path_to_title(path)
        .and_then(|title| title_to_path(&title).ok())
        .is_some_and(|canonical| canonical == path)
}

/// What a proposal's knowledge files (`given`: path and new text, `None` for a removal) come to against the base
/// (`was`: the text a path had there): refused when they set what only a curation proposal sets and the proposal is
/// not one ([`Invalid::KnowledgeCurated`]), and refused when a change would have no line on the proposal's page
/// (`describe` is the page's own, [`crate::knowledge::summary::summarize`]; [`Invalid::KnowledgeNotShown`]): trust
/// never moves unseen, in a curation proposal least of all.
pub(crate) fn knowledge_verdict(
    given: &[(String, Option<String>)],
    was: &dyn Fn(&str) -> Option<String>,
    curation: bool,
    describe: crate::knowledge::summary::Describer<'_>,
) -> std::result::Result<(), Invalid> {
    if !curation && !crate::knowledge::check::curated_by(given, was).is_empty() {
        return Err(Invalid::KnowledgeCurated);
    }
    if !crate::knowledge::summary::undescribed_by(given, was, describe).is_empty() {
        return Err(Invalid::KnowledgeNotShown);
    }
    Ok(())
}

impl ProposalService {
    /// Judge the files of `content` against the repository at its base: new paths (P4) and owned keys (P5). The
    /// syntactic rule (every path is a page path) is [`super::validate`]'s, which needs no repository.
    pub(super) fn judge_files(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        content: &RevisionContent,
    ) -> Result<()> {
        if content.files.is_empty() && content.renames.is_empty() {
            return Ok(());
        }
        let present = self.bases.page_paths(wiki, &content.base)?;
        self.judge_paths(content, &present)?;
        self.judge_owned_keys(principal, wiki, content, &present)
    }

    /// Judge the knowledge files of `content` against the base, at **submit and at every revision** (handoff 005
    /// corrections K7): a proposal the acceptance would always refuse is not one that can be sent. The rules are
    /// the acceptance's own ([`crate::knowledge::check::curated_by`]: what only a curation proposal sets), run on
    /// the files as the base had them; the acceptance still judges them again on the tip, where it is final.
    pub(super) fn judge_knowledge(
        &self,
        wiki: &WikiRecord,
        content: &RevisionContent,
    ) -> Result<()> {
        let given: Vec<(String, Option<String>)> = content
            .files
            .iter()
            .filter(|f| crate::knowledge::family_of(&f.path).is_some())
            .map(|f| (f.path.clone(), f.text.clone()))
            .collect();
        if given.is_empty() {
            return Ok(());
        }
        let mut before: BTreeMap<&str, Option<String>> = BTreeMap::new();
        for (path, _) in &given {
            before.insert(path, self.bases.page_text(wiki, &content.base, path)?);
        }
        let was = |path: &str| before.get(path).cloned().flatten();
        Ok(knowledge_verdict(
            &given,
            &was,
            crate::knowledge::curation::is_curation(content),
            &crate::knowledge::summary::summarize,
        )?)
    }

    fn judge_paths(&self, content: &RevisionContent, present: &BTreeSet<String>) -> Result<()> {
        // Only pages are judged here; a knowledge file's place and name are the knowledge layer's (K-T2).
        let pages = || content.files.iter().filter(|f| is_page_path(&f.path));
        let removed: BTreeSet<&str> = pages()
            .filter(|f| f.text.is_none())
            .map(|f| f.path.as_str())
            .chain(content.renames.iter().map(|r| r.from.as_str()))
            .collect();
        // The paths that are new at the base: a page written where there was none, and where a page is moved to.
        let new: BTreeSet<&str> = pages()
            .filter(|f| f.text.is_some())
            .map(|f| f.path.as_str())
            .chain(content.renames.iter().map(|r| r.to.as_str()))
            .filter(|p| !present.contains(*p))
            .collect();
        // A move onto a page that stays is no new page; it is a collision too (the page would be replaced).
        let landing: BTreeSet<&str> = content.renames.iter().map(|r| r.to.as_str()).collect();
        let unchanged: BTreeSet<&str> = present
            .iter()
            .map(String::as_str)
            .filter(|p| !removed.contains(p))
            .collect();
        if landing.iter().any(|to| unchanged.contains(to)) {
            return Err(Invalid::PathCollides.into());
        }
        // The names a rename starts from must be pages the rule reads (a path outside `pages/` was refused
        // already); a start that is not at the base is judged like a new path.
        let starts_absent = content
            .renames
            .iter()
            .map(|r| r.from.as_str())
            .filter(|p| !present.contains(*p));
        if new
            .iter()
            .copied()
            .chain(starts_absent)
            .any(|p| !is_canonical(p))
        {
            return Err(Invalid::PathNotCanonical.into());
        }
        let stays: BTreeSet<String> = unchanged.iter().map(|p| collision_key(p)).collect();
        let mut keys = BTreeSet::new();
        for path in &new {
            let key = collision_key(path);
            if stays.contains(&key) || !keys.insert(key) {
                return Err(Invalid::PathCollides.into());
            }
        }
        Ok(())
    }

    fn judge_owned_keys(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        content: &RevisionContent,
        present: &BTreeSet<String>,
    ) -> Result<()> {
        let at_base = |path: &str| -> Result<Option<String>> {
            if present.contains(path) {
                Ok(self.bases.page_text(wiki, &content.base, path)?)
            } else {
                Ok(None)
            }
        };
        // What a page at the base had, read as keys. A page that is unreadable already has none to protect.
        let keys_before =
            |old: &Option<String>| old.as_deref().and_then(front_keys).unwrap_or_default();
        let mut changed: BTreeSet<&'static str> = BTreeSet::new();
        for file in content.files.iter().filter(|f| is_page_path(&f.path)) {
            let old = at_base(&file.path)?;
            let after = match &file.text {
                // Unchanged text changes nothing, whatever it holds.
                Some(new) if old.as_deref() == Some(new.as_str()) => continue,
                Some(new) => front_keys(new).ok_or(Invalid::FrontUnreadable)?,
                None => BTreeMap::new(),
            };
            changed.extend(changed_owned_keys(&keys_before(&old), &after));
        }
        for rename in &content.renames {
            let old = at_base(&rename.from)?;
            changed.extend(changed_owned_keys(&keys_before(&old), &BTreeMap::new()));
        }
        if changed.is_empty() {
            return Ok(());
        }
        let admin = self
            .authorizer
            .authorize(
                principal,
                Action::Admin,
                &Resource::Wiki {
                    wiki: wiki.id.clone(),
                },
            )
            .is_allowed();
        if admin {
            Ok(())
        } else {
            Err(ProposalError::OwnedKeys(changed.into_iter().collect()))
        }
    }
}
