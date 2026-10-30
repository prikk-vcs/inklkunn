#![forbid(unsafe_code)]
//! `inklkunn-corpus` — generators for the two M0 spikes (RFC 001 D-10; handoff 001 T6). Measurement only.
//!
//! ```text
//! inklkunn-corpus wiki      --out DIR --keys DIR --pages N --total-bytes B [--depth D] [--blocks K] [--seed S]
//! inklkunn-corpus histories --out DIR --keys DIR --trials N [--seed S] [--report FILE]
//! inklkunn-corpus merge-fixture --report FILE --out FILE
//! ```
//!
//! `wiki` builds a prikk repository holding a Portable Wiki Bundle of `N` pages whose text totals about `B`
//! bytes, with one hierarchy chain `D` levels deep, sealed in `K` blocks plus one editing block, so that
//! every read verb has something to do. `histories` builds a base wiki and then runs `N` trials of two
//! concurrent edits from one baseline: one sealed on `heads/main`, the other on a branch, then
//! `prikk merge` from the baseline; it records prikk's confluence outcome beside the product's block-level
//! three-way merge (`inklkunn_core::merge`, RFC 004 D-5) of the same two edits, with the measurement hook's
//! count of overlaps a line-level merge would have made clean, and a review-latency model of how often a base
//! moves. `merge-fixture` replays a recorded `histories` report without prikk and writes its pairs as the
//! three sides of a merge: the committed fixture of `inklkunn-core`'s merge test (handoff 004 T2).
//!
//! Keys: `--keys` names a directory this tool fills with a throwaway author and maintainer pair
//! (`prikk setup` with `HOME` pointed there); never a real key directory.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod histories;
mod prikk;
mod wiki;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("inklkunn-corpus: {e}");
            2
        }
    };
    std::process::exit(code);
}

/// `--key value` pairs after the subcommand.
pub struct Opts {
    /// The pairs as given.
    pub values: BTreeMap<String, String>,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        let mut it = args.iter();
        while let Some(k) = it.next() {
            let Some(key) = k.strip_prefix("--") else {
                return Err(format!("unexpected argument `{k}`"));
            };
            let Some(v) = it.next() else {
                return Err(format!("`--{key}` needs a value"));
            };
            values.insert(key.to_owned(), v.clone());
        }
        Ok(Opts { values })
    }

    /// A required absolute path.
    pub fn path(&self, key: &str) -> Result<PathBuf, String> {
        let v = self
            .values
            .get(key)
            .ok_or_else(|| format!("`--{key}` is required"))?;
        let p = PathBuf::from(v);
        if !p.is_absolute() {
            return Err(format!("`--{key}` must be absolute"));
        }
        Ok(p)
    }

    /// A number, with a default when the key is absent (`None` makes it required).
    pub fn num(&self, key: &str, default: Option<u64>) -> Result<u64, String> {
        match self.values.get(key) {
            Some(v) => v.parse::<u64>().map_err(|e| format!("`--{key}`: {e}")),
            None => default.ok_or_else(|| format!("`--{key}` is required")),
        }
    }

    /// An optional path.
    pub fn opt_path(&self, key: &str) -> Option<PathBuf> {
        self.values.get(key).map(PathBuf::from)
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let Some((sub, rest)) = args.split_first() else {
        return Err(
            "usage: inklkunn-corpus wiki|histories|merge-fixture --out DIR --keys DIR …".to_owned(),
        );
    };
    let opts = Opts::parse(rest)?;
    match sub.as_str() {
        "wiki" => wiki::build_wiki(&opts),
        "histories" => histories::run(&opts),
        "merge-fixture" => histories::merge_fixture(&opts),
        other => Err(format!("unknown subcommand `{other}`")),
    }
}

/// Run a program to completion, returning stdout; a failure carries stderr.
pub fn sh(
    dir: &Path,
    env: &[(&str, &OsStr)],
    program: &str,
    args: &[&str],
) -> Result<Vec<u8>, String> {
    let mut c = Command::new(program);
    c.args(args).current_dir(dir).stdin(Stdio::null());
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c
        .output()
        .map_err(|e| format!("{program} {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "{program} {} failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}
