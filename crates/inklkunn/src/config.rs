//! What the operator sets, from the environment and, for `serve`, a file. Every refusal names the setting
//! and says what is wanted, so a misconfigured start never has to be guessed at.
//!
//! | Variable | Meaning | Default |
//! |---|---|---|
//! | `INKLKUNN_DATA_DIR` | holds `store.sqlite` and `wikis/` (absolute) | **required** |
//! | `INKLKUNN_SECRET_FILE` | the file with the operator's secret that opens every wiki's keys (absolute, 16 to 1024 bytes) | **required** |
//! | `INKLKUNN_KEY_DIR` | the sealed key records (mode 0700) | `<data dir>/keys` |
//! | `INKLKUNN_SEED_DIR` | a directory on a tmpfs where a signing call's keys live for that call | `/dev/shm/inklkunn` |
//! | `INKLKUNN_PRIKK` | the prikk binary | `prikk` (found on `PATH`) |
//! | `INKLKUNN_BIND` | (`serve`) where to listen | `127.0.0.1:8080` |
//! | `INKLKUNN_APP_ORIGIN` | (`serve`) the origin pages and the API are served from, e.g. `https://wiki.example` | **required** |
//! | `INKLKUNN_CONTENT_ORIGIN` | (`serve`) the origin wiki-authored bytes are served from, a different host when reachable | **required** |
//! | `INKLKUNN_ADMISSION_BUDGET_MIB` | (`serve`) the memory the prikk calls share | `1024` |
//! | `INKLKUNN_ADMISSION_WAIT_MS` | (`serve`) how long a read waits for budget before it is told the service is busy | `2000` |
//! | `INKLKUNN_TIP_TTL_SECS` | (`serve`) how long a view's newest state is held before prikk is asked again; a change made a moment ago may take this long to show on a page | `5` |
//! | `INKLKUNN_INDEX_MAX_MIB` | (`serve`) the largest wiki, in MiB of everything it holds (attachments too), the title index is built from; a larger one is "could not be read now" until this is raised. **The build extracts the whole wiki into `<data dir>/wikis/…/scratch` for a moment, so the data directory needs that much free disk.** | `4096` |
//! | `INKLKUNN_INDEX_MAX_FILES` | (`serve`) the most files the title index is built from | `200000` |
//! | `INKLKUNN_PAGE_MAX_KIB` | (`serve`) the largest page source read, in KiB | `1024` |
//! | `INKLKUNN_TRUSTED_PROXIES` | (`serve`) the reverse proxies whose `X-Forwarded-For` is believed: comma-separated addresses or networks (`10.0.0.0/8`); none by default | none |
//!
//! `serve --config FILE` reads the same names from a file, one `NAME=value` per line (`#` comments); the
//! environment wins over the file, and a name that is not a setting is refused with its line number.
//!
//! prikk is confined with bubblewrap unless the command line starts with `--dev-unconfined`, which is for
//! development and tests, prints a banner, and is a flag rather than a variable so that it cannot be left
//! set in a service's environment by accident.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use inklkunn_prikk::sandbox::{BwrapConfig, Sandbox, probe_bwrap};

/// A setting that cannot be used, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// The operator's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `INKLKUNN_DATA_DIR`.
    pub data_dir: PathBuf,
    /// `INKLKUNN_KEY_DIR`.
    pub key_dir: PathBuf,
    /// `INKLKUNN_SECRET_FILE`.
    pub secret_file: PathBuf,
    /// `INKLKUNN_SEED_DIR`.
    pub seed_dir: PathBuf,
    /// `INKLKUNN_PRIKK`.
    pub prikk: OsString,
    /// Started with `--dev-unconfined`.
    pub dev_unconfined: bool,
}

/// A setting that must be an absolute, UTF-8 path.
pub(crate) fn absolute_setting(name: &str, value: &OsStr) -> Result<PathBuf, ConfigError> {
    let path = PathBuf::from(value);
    if value.is_empty() || !path.is_absolute() {
        return Err(ConfigError(format!(
            "{name} must be an absolute path (it is `{}`)",
            path.display()
        )));
    }
    if path.to_str().is_none() {
        return Err(ConfigError(format!("{name} must be a UTF-8 path")));
    }
    Ok(path)
}

impl Settings {
    /// Read the settings through `get` (the process environment in the binary, a map in a test).
    pub fn load(
        get: impl Fn(&str) -> Option<OsString>,
        dev_unconfined: bool,
    ) -> Result<Self, ConfigError> {
        let required = |name: &str| {
            get(name)
                .ok_or_else(|| ConfigError(format!("{name} is not set; it is required")))
                .and_then(|v| absolute_setting(name, &v))
        };
        let data_dir = required("INKLKUNN_DATA_DIR")?;
        let secret_file = required("INKLKUNN_SECRET_FILE")?;
        let key_dir = match get("INKLKUNN_KEY_DIR") {
            Some(v) => absolute_setting("INKLKUNN_KEY_DIR", &v)?,
            None => data_dir.join("keys"),
        };
        let seed_dir = match get("INKLKUNN_SEED_DIR") {
            Some(v) => absolute_setting("INKLKUNN_SEED_DIR", &v)?,
            None => PathBuf::from("/dev/shm/inklkunn"),
        };
        let prikk = match get("INKLKUNN_PRIKK") {
            Some(v) if v.is_empty() => {
                return Err(ConfigError("INKLKUNN_PRIKK is set but empty".to_owned()));
            }
            Some(v) => v,
            None => OsString::from("prikk"),
        };
        Ok(Settings {
            data_dir,
            key_dir,
            secret_file,
            seed_dir,
            prikk,
            dev_unconfined,
        })
    }

    /// `<data dir>/store.sqlite`.
    pub fn store_path(&self) -> PathBuf {
        self.data_dir.join("store.sqlite")
    }

    /// `<data dir>/wikis`: the root layout's root.
    pub fn wikis_root(&self) -> PathBuf {
        self.data_dir.join("wikis")
    }

    /// How prikk is confined. Bubblewrap unless `--dev-unconfined`, and bubblewrap must work here: a host
    /// that cannot make the confinement refuses to run prikk at all rather than run it unconfined.
    pub fn sandbox(&self) -> Result<Sandbox, ConfigError> {
        if self.dev_unconfined {
            return Ok(Sandbox::Unconfined);
        }
        let bwrap = BwrapConfig::default();
        probe_bwrap(&bwrap.bwrap).map_err(|why| {
            ConfigError(format!(
                "bubblewrap cannot confine prikk here ({why}); install bubblewrap and allow user namespaces, \
                 or, for development only, start with --dev-unconfined"
            ))
        })?;
        Ok(Sandbox::Bwrap(bwrap))
    }
}

/// The settings `serve` adds: where it listens and what its two origins are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSettings {
    /// `INKLKUNN_BIND`, default `127.0.0.1:8080`.
    pub bind: String,
    /// `INKLKUNN_APP_ORIGIN`, required.
    pub app_origin: String,
    /// `INKLKUNN_CONTENT_ORIGIN`, required.
    pub content_origin: String,
    /// `INKLKUNN_ADMISSION_BUDGET_MIB`: the memory budget the prikk calls share, default 1024.
    pub admission_budget_mib: u64,
    /// `INKLKUNN_ADMISSION_WAIT_MS`: how long a read waits for budget before it is told the service is busy,
    /// default 2000.
    pub admission_wait_ms: u64,
    /// What the read service holds and reads.
    pub read: ReadSettings,
    /// `INKLKUNN_TRUSTED_PROXIES`: networks of reverse proxies, as written (validated at start).
    pub trusted_proxies: Vec<String>,
}

/// The read service's settings (RFC 003 D-2; handoff 003 T4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadSettings {
    /// `INKLKUNN_TIP_TTL_SECS`, default 5.
    pub tip_ttl_secs: u64,
    /// `INKLKUNN_INDEX_MAX_MIB`, default 4096.
    pub index_max_mib: u64,
    /// `INKLKUNN_INDEX_MAX_FILES`, default 200,000.
    pub index_max_files: u64,
    /// `INKLKUNN_PAGE_MAX_KIB`, default 1024.
    pub page_max_kib: u64,
}

impl Default for ReadSettings {
    fn default() -> Self {
        ReadSettings {
            tip_ttl_secs: 5,
            index_max_mib: 4096,
            index_max_files: 200_000,
            page_max_kib: 1024,
        }
    }
}

impl WebSettings {
    /// Read the web settings through `get`.
    pub fn load(get: &dyn Fn(&str) -> Option<OsString>) -> Result<Self, ConfigError> {
        let text = |name: &str| -> Result<Option<String>, ConfigError> {
            get(name)
                .map(|v| {
                    v.into_string()
                        .map_err(|_| ConfigError(format!("{name} must be UTF-8 text")))
                })
                .transpose()
        };
        let required = |name: &str| {
            text(name)?.ok_or_else(|| ConfigError(format!("{name} is not set; it is required")))
        };
        let number = |name: &str, default: u64, min: u64, max: u64| -> Result<u64, ConfigError> {
            match text(name)? {
                None => Ok(default),
                Some(t) => t
                    .parse::<u64>()
                    .ok()
                    .filter(|n| (min..=max).contains(n))
                    .ok_or_else(|| {
                        ConfigError(format!(
                            "{name} must be a whole number from {min} to {max} (it is `{t}`)"
                        ))
                    }),
            }
        };
        Ok(WebSettings {
            bind: text("INKLKUNN_BIND")?.unwrap_or_else(|| "127.0.0.1:8080".to_owned()),
            app_origin: required("INKLKUNN_APP_ORIGIN")?,
            content_origin: required("INKLKUNN_CONTENT_ORIGIN")?,
            admission_budget_mib: number("INKLKUNN_ADMISSION_BUDGET_MIB", 1024, 256, 1_048_576)?,
            admission_wait_ms: number("INKLKUNN_ADMISSION_WAIT_MS", 2000, 10, 600_000)?,
            read: ReadSettings {
                tip_ttl_secs: number("INKLKUNN_TIP_TTL_SECS", 5, 0, 3600)?,
                index_max_mib: number("INKLKUNN_INDEX_MAX_MIB", 4096, 1, 1_048_576)?,
                index_max_files: number("INKLKUNN_INDEX_MAX_FILES", 200_000, 1, 100_000_000)?,
                page_max_kib: number("INKLKUNN_PAGE_MAX_KIB", 1024, 1, 1_048_576)?,
            },
            trusted_proxies: match text("INKLKUNN_TRUSTED_PROXIES")? {
                None => Vec::new(),
                Some(list) => list
                    .split(',')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
                    .collect(),
            },
        })
    }
}

/// Every setting there is, for the configuration file: a name not in this list is a typo and is refused.
pub const KNOWN: &[&str] = &[
    "INKLKUNN_DATA_DIR",
    "INKLKUNN_SECRET_FILE",
    "INKLKUNN_KEY_DIR",
    "INKLKUNN_SEED_DIR",
    "INKLKUNN_PRIKK",
    "INKLKUNN_BIND",
    "INKLKUNN_APP_ORIGIN",
    "INKLKUNN_CONTENT_ORIGIN",
    "INKLKUNN_ADMISSION_BUDGET_MIB",
    "INKLKUNN_ADMISSION_WAIT_MS",
    "INKLKUNN_TIP_TTL_SECS",
    "INKLKUNN_INDEX_MAX_MIB",
    "INKLKUNN_INDEX_MAX_FILES",
    "INKLKUNN_PAGE_MAX_KIB",
    "INKLKUNN_TRUSTED_PROXIES",
];

/// Read a configuration file: one `NAME=value` per line, `#` comments and blank lines allowed, no quoting, no
/// expansion. A name that is not a setting, a name given twice, or a line that is not `NAME=value` is refused
/// with its line number: a misspelt setting must not silently mean its default.
pub fn read_file(text: &str) -> Result<HashMap<String, OsString>, ConfigError> {
    let mut out = HashMap::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = n + 1;
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| ConfigError(format!("line {at}: expected NAME=value")))?;
        let name = name.trim();
        if !KNOWN.contains(&name) {
            return Err(ConfigError(format!("line {at}: `{name}` is not a setting")));
        }
        if out
            .insert(name.to_owned(), OsString::from(value.trim()))
            .is_some()
        {
            return Err(ConfigError(format!("line {at}: `{name}` is set twice")));
        }
    }
    Ok(out)
}

/// The settings the environment gives, then the file's for what the environment leaves out.
pub fn layered<'a>(
    env: &'a dyn Fn(&str) -> Option<OsString>,
    file: &'a HashMap<String, OsString>,
) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| env(name).or_else(|| file.get(name).cloned())
}

/// The banner `--dev-unconfined` prints on every start.
pub const UNCONFINED_BANNER: &str = "inklkunn: --dev-unconfined — prikk runs WITHOUT its sandbox. For development and tests only; \
never run a service this way.";

#[cfg(test)]
mod tests;
