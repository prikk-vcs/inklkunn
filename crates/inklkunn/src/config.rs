//! What the operator sets (read from the environment; `serve` adds a file in RFC 003's handoff). Every
//! refusal names the setting and says what is wanted, so a misconfigured start never has to be guessed at.
//!
//! | Variable | Meaning | Default |
//! |---|---|---|
//! | `INKLKUNN_DATA_DIR` | holds `store.sqlite` and `wikis/` (absolute) | **required** |
//! | `INKLKUNN_SECRET_FILE` | the file with the operator's secret that opens every wiki's keys (absolute, 16 to 1024 bytes) | **required** |
//! | `INKLKUNN_KEY_DIR` | the sealed key records (mode 0700) | `<data dir>/keys` |
//! | `INKLKUNN_SEED_DIR` | a directory on a tmpfs where a signing call's keys live for that call | `/dev/shm/inklkunn` |
//! | `INKLKUNN_PRIKK` | the prikk binary | `prikk` (found on `PATH`) |
//!
//! prikk is confined with bubblewrap unless the command line starts with `--dev-unconfined`, which is for
//! development and tests, prints a banner, and is a flag rather than a variable so that it cannot be left
//! set in a service's environment by accident.

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

fn absolute(name: &str, value: &OsStr) -> Result<PathBuf, ConfigError> {
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
                .and_then(|v| absolute(name, &v))
        };
        let data_dir = required("INKLKUNN_DATA_DIR")?;
        let secret_file = required("INKLKUNN_SECRET_FILE")?;
        let key_dir = match get("INKLKUNN_KEY_DIR") {
            Some(v) => absolute("INKLKUNN_KEY_DIR", &v)?,
            None => data_dir.join("keys"),
        };
        let seed_dir = match get("INKLKUNN_SEED_DIR") {
            Some(v) => absolute("INKLKUNN_SEED_DIR", &v)?,
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

/// The banner `--dev-unconfined` prints on every start.
pub const UNCONFINED_BANNER: &str = "inklkunn: --dev-unconfined — prikk runs WITHOUT its sandbox. For development and tests only; \
never run a service this way.";

#[cfg(test)]
mod tests;
