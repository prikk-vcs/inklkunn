//! A stand-in repository for the export's tests: files in memory, a history of blocks with real patch messages and prikk's own
//! operation names, and **the text each block gave each page**, so that the history records an export writes (RFC 007 D-7) can be
//! checked with no prikk, in both test runs. It answers what an export asks: the tip's tree and its extraction, the log, a block's
//! operations, and a file's bytes at a block.
#![allow(dead_code, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::Path;

use inklkunn_core::message::Message;
use inklkunn_prikk::storage::{
    Change, Diff, Entry, FileMeta, History, HistoryBlock, Materialized, Operation, Patch,
    PatchMessage, PathRef, Tree, Verification, View, Views,
};
use inklkunn_prikk::{ExtractBounds, Point, Storage, StorageError};

/// The tip of the stand-in's default view when it has no block of its own to say.
pub const NO_TIP: &str = "b10c0000000000000000000000000000000000000000000000000000000000ff";

/// A repository that holds these files at its tip and this history.
#[derive(Clone)]
pub struct Stand {
    /// The tip's files.
    pub files: BTreeMap<String, String>,
    /// Oldest first.
    pub blocks: Vec<(HistoryBlock, Change)>,
    /// The text a block gave a page, by `(block id, path)`; a write with none here gets a text of its own that names the block.
    pub texts: BTreeMap<(String, String), String>,
    /// The id each page has when no text says it, by path.
    pub ids: BTreeMap<String, String>,
}

impl Stand {
    /// A stand-in with these files and this history.
    pub fn new(files: BTreeMap<String, String>, blocks: Vec<(HistoryBlock, Change)>) -> Self {
        Stand {
            files,
            blocks,
            texts: BTreeMap::new(),
            ids: BTreeMap::new(),
        }
    }

    /// The same, with `text` as what `block` gave `path`.
    #[must_use]
    pub fn giving(mut self, block: &str, path: &str, text: &str) -> Self {
        self.texts
            .insert((block.to_owned(), path.to_owned()), text.to_owned());
        self
    }

    /// The same, with `id` as the id of the page at `path` where no text of it says another.
    #[must_use]
    pub fn identified(mut self, path: &str, id: &str) -> Self {
        self.ids.insert(path.to_owned(), id.to_owned());
        self
    }

    /// The tip: the newest block.
    pub fn tip(&self) -> &str {
        self.blocks
            .last()
            .map_or(NO_TIP, |(b, _)| b.block_id.as_str())
    }

    fn default_text(&self, path: &str, seq: u64) -> String {
        let id = self
            .ids
            .get(path)
            .cloned()
            .or_else(|| {
                self.files.get(path).and_then(|t| {
                    t.lines()
                        .find_map(|l| l.strip_prefix("id: ").map(str::to_owned))
                })
            })
            .unwrap_or_else(|| path.replace(['/', ' ', '.'], "-"));
        let title = path
            .strip_prefix("pages/")
            .and_then(|p| p.strip_suffix(".md"))
            .unwrap_or(path);
        format!("---\ntitle: {title}\nid: {id}\n---\n\nWritten by block {seq}.\n")
    }

    /// The text of `path` after the block at place `upto` (`None` when there is no such file).
    fn text_after(&self, upto: usize, path: &str) -> Option<String> {
        let mut current: Option<String> = None;
        for (place, (block, change)) in self.blocks.iter().enumerate().take(upto + 1) {
            for patch in &change.patches {
                let mut ops: Vec<&Operation> = patch.operations.iter().collect();
                // A patch's path-addressed operations first, then its edits.
                ops.sort_by_key(|op| op.kind == "edit-text");
                for op in ops {
                    let at = |i: usize| op.paths.get(i).and_then(|p| p.path.as_deref());
                    let written = || {
                        self.texts
                            .get(&(block.block_id.clone(), path.to_owned()))
                            .cloned()
                            .unwrap_or_else(|| self.default_text(path, block.update_seq))
                    };
                    match op.kind.as_str() {
                        "create-file" | "edit-text" if at(0) == Some(path) => {
                            current = Some(written());
                        }
                        "delete-node" if at(0) == Some(path) => current = None,
                        "rename-path" if at(1) == Some(path) => {
                            let moved = at(0)
                                .zip(place.checked_sub(1))
                                .and_then(|(from, before)| self.text_after(before, from));
                            current = Some(moved.unwrap_or_else(written));
                        }
                        "rename-path" if at(0) == Some(path) => current = None,
                        _ => {}
                    }
                }
            }
        }
        current
    }

    fn place_of(&self, block: &str) -> Option<usize> {
        self.blocks.iter().position(|(b, _)| b.block_id == block)
    }
}

fn unsupported<T>() -> Result<T, StorageError> {
    Err(StorageError::Refused {
        verb: "stand-in",
        message: "the stand-in answers only what an export asks".to_owned(),
        kind: None,
        reason: None,
    })
}

impl Storage for Stand {
    fn views(&self, _: &Path) -> Result<Views, StorageError> {
        Ok(Views {
            schema_version: "branch-list-v1".to_owned(),
            views: vec![View {
                name: "heads/main".to_owned(),
                state_id: self
                    .blocks
                    .last()
                    .map_or_else(|| "a".repeat(64), |(b, _)| b.ref_state_id.clone()),
                closed: false,
                current: true,
            }],
        })
    }
    fn all_views(&self, _: &Path) -> Result<Views, StorageError> {
        unsupported()
    }
    fn tree(&self, _: &Path, point: &Point, _: Option<&str>) -> Result<Tree, StorageError> {
        Ok(Tree {
            schema_version: "tree-listing-v1".to_owned(),
            point: point.to_string(),
            target_block_id: self.tip().to_owned(),
            prefix: None,
            entries: self
                .files
                .iter()
                .map(|(path, text)| Entry {
                    path: path.clone(),
                    kind: "file".to_owned(),
                    encoding: None,
                    mode: None,
                    size: Some(text.len() as u64),
                })
                .collect(),
        })
    }
    fn stat(&self, _: &Path, point: &Point, path: &str) -> Result<FileMeta, StorageError> {
        let Point::Block(block) = point else {
            return unsupported();
        };
        let text = self
            .place_of(block)
            .and_then(|place| self.text_after(place, path));
        match text {
            Some(text) => Ok(FileMeta {
                schema_version: "path-content-v1".to_owned(),
                point: point.to_string(),
                target_block_id: block.clone(),
                path: path.to_owned(),
                kind: "file".to_owned(),
                encoding: Some("text".to_owned()),
                mode: None,
                size: text.len() as u64,
            }),
            None => Err(StorageError::Refused {
                verb: "stat",
                message: format!("path {path} not found"),
                kind: None,
                reason: None,
            }),
        }
    }
    fn read(&self, _: &Path, point: &Point, path: &str, max: u64) -> Result<Vec<u8>, StorageError> {
        let Point::Block(block) = point else {
            return unsupported();
        };
        let text = self
            .place_of(block)
            .and_then(|place| self.text_after(place, path));
        match text {
            Some(text) if (text.len() as u64) <= max => Ok(text.into_bytes()),
            Some(_) => Err(StorageError::Refused {
                verb: "cat",
                message: "the file is larger than the bound".to_owned(),
                kind: None,
                reason: None,
            }),
            None => Err(StorageError::Refused {
                verb: "cat",
                message: format!("path {path} not found"),
                kind: None,
                reason: None,
            }),
        }
    }
    fn history(&self, _: &Path, view: &Point, limit: Option<u64>) -> Result<History, StorageError> {
        let newest_first = self.blocks.iter().rev().map(|(b, _)| b.clone());
        let blocks = match limit.and_then(|l| usize::try_from(l).ok()) {
            Some(limit) => newest_first.take(limit).collect(),
            None => newest_first.collect(),
        };
        Ok(History {
            schema_version: "log-report-v1".to_owned(),
            view: view.to_string(),
            blocks,
        })
    }
    fn change(&self, _: &Path, block: &str) -> Result<Change, StorageError> {
        self.blocks
            .iter()
            .find(|(b, _)| b.block_id == block)
            .map(|(_, c)| c.clone())
            .ok_or(StorageError::Refused {
                verb: "stand-in",
                message: "no such block".to_owned(),
                kind: None,
                reason: None,
            })
    }
    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> Result<Diff, StorageError> {
        unsupported()
    }
    fn plan(&self, _: &Path, _: &Point) -> Result<Materialized, StorageError> {
        unsupported()
    }
    fn materialize(
        &self,
        _: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        _: ExtractBounds,
    ) -> Result<Materialized, StorageError> {
        let target = scratch.join(name);
        std::fs::create_dir_all(&target).expect("target");
        for (path, text) in &self.files {
            let at = target.join(path);
            std::fs::create_dir_all(at.parent().expect("a parent")).expect("dirs");
            std::fs::write(at, text).expect("a file");
        }
        Ok(Materialized {
            schema_version: "extract-report-v1".to_owned(),
            point: point.to_string(),
            point_kind: "block".to_owned(),
            target_block_id: self.tip().to_owned(),
            claim_parent_block_id: None,
            provisional: false,
            written: true,
            file_count: self.files.len() as u64,
            total_bytes: self.files.values().map(|t| t.len() as u64).sum(),
            largest_file_bytes: 0,
        })
    }
    fn verify(&self, _: &Path) -> Result<Verification, StorageError> {
        unsupported()
    }
}

/// One operation of a block, in prikk's own names (`create-file`, `edit-text`, `delete-node`); the words `add`, `modify` and
/// `delete` are taken for them, as the first tests of the export wrote them.
pub fn op(kind: &str, path: &str) -> Operation {
    let kind = match kind {
        "add" => "create-file",
        "modify" => "edit-text",
        "delete" => "delete-node",
        other => other,
    };
    Operation {
        kind: kind.to_owned(),
        paths: vec![PathRef {
            path: Some(path.to_owned()),
            unresolved_node_id: None,
        }],
    }
}

/// A `rename-path` operation: the old path, then the new one.
pub fn rename(from: &str, to: &str) -> Operation {
    Operation {
        kind: "rename-path".to_owned(),
        paths: [from, to]
            .into_iter()
            .map(|p| PathRef {
                path: Some(p.to_owned()),
                unresolved_node_id: None,
            })
            .collect(),
    }
}

/// A block of one patch with this message and these operations.
pub fn block_of(
    id: &str,
    seq: u64,
    patch: &str,
    message: &Message,
    operations: Vec<Operation>,
) -> (HistoryBlock, Change) {
    (
        HistoryBlock {
            block_id: id.to_owned(),
            ref_state_id: format!("{seq:064x}"),
            update_seq: seq,
            kind: if seq == 1 { "Root" } else { "Normal" }.to_owned(),
            rollback_block: false,
            patch_count: 1,
            patch_messages: vec![PatchMessage {
                patch_id: patch.to_owned(),
                message: message.to_text(),
            }],
            previous_ref_state_id: (seq > 1).then(|| format!("{:064x}", seq - 1)),
        },
        Change {
            schema_version: "show-report-v1".to_owned(),
            patches: vec![Patch {
                patch_id: patch.to_owned(),
                queued: false,
                operations,
            }],
        },
    )
}

/// A block whose operations are `(kind, path)`.
pub fn block(
    id: &str,
    seq: u64,
    patch: &str,
    message: &Message,
    ops: &[(&str, &str)],
) -> (HistoryBlock, Change) {
    block_of(
        id,
        seq,
        patch,
        message,
        ops.iter().map(|(kind, path)| op(kind, path)).collect(),
    )
}
