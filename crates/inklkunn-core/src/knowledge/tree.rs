//! The knowledge files of an extracted tree: what the acceptance check and the derived index both read.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::{KNOWLEDGE_DIR, MAX_FILE_BYTES, is_keep};

/// The most bytes of knowledge files held in memory at once (the acceptance check holds the tip's knowledge files
/// together with the change).
pub const MAX_TREE_BYTES: u64 = 128 << 20;

/// A file under `knowledge/` of an extracted tree. Its bytes are cut one byte past [`MAX_FILE_BYTES`]: enough to
/// know the file is too long, never more than a bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFile {
    /// The repository path, `knowledge/claims/<id>.yaml`.
    pub path: String,
    /// The first bytes of the file.
    pub bytes: Vec<u8>,
}

/// Why the knowledge directory of an extracted tree could not be read.
#[derive(Debug)]
pub enum TreeError {
    /// More files and directories under `knowledge/` than the bound.
    TooMany(u64),
    /// More bytes of knowledge files than [`MAX_TREE_BYTES`] are held at once.
    TooLarge,
    /// A directory or a file could not be read.
    Io(std::io::Error),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeError::TooMany(n) => write!(f, "more than {n} entries under `knowledge/`"),
            TreeError::TooLarge => write!(
                f,
                "more than {MAX_TREE_BYTES} bytes of files under `knowledge/` to hold at once"
            ),
            TreeError::Io(e) => write!(f, "the extracted knowledge files could not be read: {e}"),
        }
    }
}

impl std::error::Error for TreeError {}

/// Every regular file under `<root>/knowledge`, in path order, with where it is on disk. `max_entries` bounds the
/// files and directories visited. A link, a device, or a name that is not UTF-8 is not a file of ours (prikk
/// stores none of them), and neither is a keep-file. A tree without a `knowledge/` directory has no knowledge files.
pub fn list_tree(root: &Path, max_entries: u64) -> Result<Vec<(String, PathBuf)>, TreeError> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    let mut visited: u64 = 0;
    let mut stack = vec![(KNOWLEDGE_DIR.to_owned(), root.join(KNOWLEDGE_DIR))];
    while let Some((logical, dir)) = stack.pop() {
        let listing = match std::fs::read_dir(&dir) {
            Ok(l) => l,
            Err(e) if logical == KNOWLEDGE_DIR && e.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(e) => return Err(TreeError::Io(e)),
        };
        for item in listing {
            let item = item.map_err(TreeError::Io)?;
            visited += 1;
            if visited > max_entries {
                return Err(TreeError::TooMany(max_entries));
            }
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(kind) = item.file_type() else {
                continue;
            };
            let path = format!("{logical}/{name}");
            if kind.is_dir() {
                stack.push((path, item.path()));
            } else if kind.is_file() && !is_keep(&path) {
                found.push((path, item.path()));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

/// The first bytes of a file, cut one byte past [`MAX_FILE_BYTES`].
pub fn read_head(file: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)?
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Every file of [`list_tree`] with its first bytes, all held at once and within [`MAX_TREE_BYTES`].
pub fn read_tree(root: &Path, max_entries: u64) -> Result<Vec<TreeFile>, TreeError> {
    let mut held: u64 = 0;
    list_tree(root, max_entries)?
        .into_iter()
        .map(|(path, file)| {
            let bytes = read_head(&file).map_err(TreeError::Io)?;
            held += bytes.len() as u64;
            if held > MAX_TREE_BYTES {
                return Err(TreeError::TooLarge);
            }
            Ok(TreeFile { path, bytes })
        })
        .collect()
}
