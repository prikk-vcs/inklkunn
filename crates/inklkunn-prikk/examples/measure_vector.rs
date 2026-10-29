//! Measurement only (handoff 003 T9; ledger PK-20, issue IS-12): the smallest memory ceiling at which each read
//! verb still succeeds on a repository, **through the driver's own vector** (`prlimit --data` in front of
//! bubblewrap in front of prikk, or unconfined with `--unconfined`), as a multiple of the store's size *S*.
//! The cost table (`cost.rs`) is derived from figures like these; the ceiling it gives a call is 1.5 × its
//! cost, so a verb whose measured need is above its cost × 1.5 would be stopped by its own ceiling.
//!
//! ```text
//! cargo run --release -p inklkunn-prikk --example measure_vector -- REPO [--unconfined] [--path P]
//! ```
//!
//! `REPO` is a prikk repository with at least one sealed block. `--path` names the file `cat` reads (default
//! the largest file under `pages/`). The verbs: `tree`, `cat` of that file, `extract` of the tip, `verify`,
//! and, when the repository has two or more blocks, `show` of the newest block and `diff` of the file between
//! the two newest blocks. Each is bisected to 256 KiB with the driver's `with_fixed_ceiling` knob; a call that
//! the ceiling stopped is `CeilingExceeded`, anything else is a failure of the measurement and is printed.
//!
//! The repository is copied into a scratch directory first (the driver mounts it read-only, but prikk may
//! write caches; the copy keeps the original untouched). Latency is the wall time of the call at the found
//! ceiling.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use inklkunn_prikk::{
    Admission, ExtractBounds, Point, PrikkCli, Sandbox, Storage, StorageError, Unavailable, cost,
};

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type()?.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn largest_page(repo: &Path) -> Option<String> {
    let mut best: Option<(u64, String)> = None;
    let mut stack = vec![repo.join("pages")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            let meta = entry.metadata().ok()?;
            if meta.is_dir() {
                stack.push(path);
            } else if best.as_ref().is_none_or(|(size, _)| meta.len() > *size) {
                let rel = path.strip_prefix(repo).ok()?.to_str()?.to_owned();
                best = Some((meta.len(), rel));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Run `call` with the ceiling `bytes`: `Ok(true)` it succeeded, `Ok(false)` the ceiling stopped it, `Err` it
/// failed for another reason.
fn attempt(
    sandbox: &Sandbox,
    source: &Path,
    bytes: u64,
    call: &dyn Fn(&PrikkCli, &Path) -> Result<(), StorageError>,
) -> Result<bool, String> {
    let driver = PrikkCli::open(
        "prikk",
        sandbox.clone(),
        Admission::new(64 << 30, 64 << 20, Duration::from_secs(1)),
    )
    .map_err(|e| e.to_string())?
    .with_fixed_ceiling(bytes)
    .with_fixed_deadline(Duration::from_secs(600));
    // A fresh copy of the repository for every probe (the method of spike A): prikk reads a file whose mtime is
    // within the same second as "now" again, and a copy made just now is exactly that, as is a repository whose
    // file was written a moment ago.
    let probe = std::env::temp_dir().join(format!("measure-vector-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&probe);
    copy_dir(source, &probe).map_err(|e| e.to_string())?;
    let outcome = call(&driver, &probe);
    let _ = std::fs::remove_dir_all(&probe);
    match outcome {
        Ok(()) => Ok(true),
        Err(StorageError::Unavailable {
            reason: Unavailable::CeilingExceeded { .. },
            ..
        }) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// The smallest ceiling (to 256 KiB) at which `call` succeeds, and the wall time of the call there.
fn smallest(
    sandbox: &Sandbox,
    source: &Path,
    store_bytes: u64,
    call: &dyn Fn(&PrikkCli, &Path) -> Result<(), StorageError>,
) -> Result<(u64, Duration), String> {
    let mut hi = (store_bytes * 8).max(64 << 20);
    if !attempt(sandbox, source, hi, call)? {
        return Err(format!("still stopped at {hi} bytes"));
    }
    let mut lo = 1 << 20;
    while hi - lo > 256 << 10 {
        let mid = lo + (hi - lo) / 2;
        if attempt(sandbox, source, mid, call)? {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let started = Instant::now();
    attempt(sandbox, source, hi, call)?;
    Ok((hi, started.elapsed()))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(repo) = args.first().map(PathBuf::from) else {
        eprintln!("usage: measure_vector REPO [--unconfined] [--path P]");
        return ExitCode::from(2);
    };
    let unconfined = args.iter().any(|a| a == "--unconfined");
    let sandbox = if unconfined {
        Sandbox::Unconfined
    } else {
        Sandbox::default()
    };
    let path = args
        .iter()
        .position(|a| a == "--path")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| largest_page(&repo));
    let Some(path) = path else {
        eprintln!("no file under pages/ to read");
        return ExitCode::from(2);
    };
    let scratch = std::env::temp_dir().join(format!("measure-vector-{}", std::process::id()));
    let work = scratch.join("repo");
    if let Err(e) = copy_dir(&repo, &work) {
        eprintln!("copying the repository: {e}");
        return ExitCode::from(2);
    }
    let store_bytes = cost::store_size(&work).unwrap_or(0);
    let main_view = Point::View("heads/main".to_owned());
    let blocks: Vec<String> = PrikkCli::open(
        "prikk",
        sandbox.clone(),
        Admission::new(64 << 30, 64 << 20, Duration::from_secs(1)),
    )
    .ok()
    .and_then(|d| d.history(&work, &main_view, Some(2)).ok())
    .map(|h| h.blocks.into_iter().map(|b| b.block_id).collect())
    .unwrap_or_default();
    println!(
        "# repository {} — S = {:.1} MiB, file {path}, {} ({} block(s) listed)",
        repo.display(),
        store_bytes as f64 / f64::from(1 << 20),
        if unconfined {
            "unconfined (prlimit only)"
        } else {
            "the driver's vector (prlimit + bubblewrap)"
        },
        blocks.len()
    );
    println!(
        "{:<8} {:>12} {:>9} {:>9}",
        "verb", "need (MiB)", "× S", "secs"
    );
    let mut rows: Vec<(&str, Result<(u64, Duration), String>)> = Vec::new();
    let mv = &main_view;
    let p = path.as_str();
    rows.push((
        "tree",
        smallest(&sandbox, &work, store_bytes, &|d, w| {
            d.tree(w, mv, None).map(drop)
        }),
    ));
    rows.push((
        "cat",
        smallest(&sandbox, &work, store_bytes, &|d, w| {
            d.read(w, mv, p, u64::MAX >> 1).map(drop)
        }),
    ));
    let extract_dir = scratch.join("extract");
    let _ = std::fs::create_dir_all(&extract_dir);
    let counter = std::cell::Cell::new(0u32);
    rows.push((
        "extract",
        smallest(&sandbox, &work, store_bytes, &|d, w| {
            counter.set(counter.get() + 1);
            let name = format!("x{}", counter.get());
            let r = d.materialize(
                w,
                mv,
                &extract_dir,
                &name,
                ExtractBounds {
                    max_bytes: None,
                    max_files: None,
                },
            );
            let _ = std::fs::remove_dir_all(extract_dir.join(&name));
            r.map(drop)
        }),
    ));
    rows.push((
        "verify",
        smallest(&sandbox, &work, store_bytes, &|d, w| d.verify(w).map(drop)),
    ));
    if let [newest, older, ..] = blocks.as_slice() {
        let (newest, older) = (newest.clone(), older.clone());
        rows.push((
            "show",
            smallest(&sandbox, &work, store_bytes, &|d, w| {
                d.change(w, &newest).map(drop)
            }),
        ));
        rows.push((
            "diff",
            smallest(&sandbox, &work, store_bytes, &|d, w| {
                d.diff(
                    w,
                    &Point::Block(older.clone()),
                    &Point::Block(newest.clone()),
                    &[p],
                )
                .map(drop)
            }),
        ));
    }
    let mut failed = false;
    for (verb, row) in rows {
        match row {
            Ok((bytes, took)) => println!(
                "{verb:<8} {:>12.1} {:>9.2} {:>9.2}",
                bytes as f64 / f64::from(1 << 20),
                bytes as f64 / store_bytes.max(1) as f64,
                took.as_secs_f64()
            ),
            Err(e) => {
                failed = true;
                println!("{verb:<8} measurement failed: {e}");
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
