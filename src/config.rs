use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use crate::error::{ClientError, Result};

/// How long a cached copy of a problem's test inputs stays usable as an offline
/// fallback. Without a bound, one network blip pins a stale copy forever.
const DEFAULT_CACHE_TTL_SECS: u64 = 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server_url: String,
    pub auth_token: Option<String>,
    pub username: Option<String>,
    pub cpp_compiler: String,
    pub java_compiler: String,
    pub java_runner: String,
    pub python_interpreter: String,
    pub default_timeout_ms: u64,
    pub max_output_bytes: usize,
    /// Address-space cap applied to judged processes, in MB. `0` disables it.
    ///
    /// Enforced via `RLIMIT_AS` on Linux and via `-Xmx` for Java everywhere.
    /// macOS cannot enforce it (Darwin rejects lowering `RLIMIT_AS`), and says
    /// so at run time rather than pretending. Off by default: a cap tight
    /// enough to matter for C++ can stop interpreters from starting.
    #[serde(default)]
    pub default_memory_limit_mb: u64,
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,
    pub temp_dir: Option<PathBuf>,
}

fn default_cache_ttl_secs() -> u64 {
    DEFAULT_CACHE_TTL_SECS
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:8080".to_string(),
            auth_token: None,
            username: None,
            cpp_compiler: detect_binary(&["g++", "clang++"], "--version")
                .unwrap_or_else(|| "g++".to_string()),
            java_compiler: detect_binary(&["javac"], "-version")
                .unwrap_or_else(|| "javac".to_string()),
            java_runner: detect_binary(&["java"], "-version").unwrap_or_else(|| "java".to_string()),
            python_interpreter: detect_binary(&["python3", "python"], "--version")
                .unwrap_or_else(|| "python3".to_string()),
            default_timeout_ms: 5000,
            max_output_bytes: 10 * 1024 * 1024, // 10MB limit
            default_memory_limit_mb: 0,
            cache_ttl_secs: DEFAULT_CACHE_TTL_SECS,
            temp_dir: None,
        }
    }
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| {
                ClientError::ConfigError("Could not determine user home directory".to_string())
            })?;
        let path = PathBuf::from(home)
            .join(".config")
            .join("cp-client")
            .join("config.json");
        Ok(path)
    }

    /// Reads the stored configuration. Pure: it never writes.
    ///
    /// A malformed file is a hard error rather than being silently replaced by
    /// defaults, which would discard the saved auth token and then report a
    /// confusing authentication failure instead of the real cause.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).map_err(|err| {
            ClientError::ConfigError(format!(
                "{} is not valid configuration JSON ({}). Fix or delete the file and run again.",
                path.display(),
                err
            ))
        })
    }

    /// Loads the configuration, creating it from detected defaults on first run.
    ///
    /// Detection executes candidate binaries, so the result is persisted rather
    /// than recomputed on every invocation. The write lives here, in a function
    /// named for it, instead of hiding inside `load`.
    pub fn load_or_create() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            return Self::load();
        }

        info!(
            "No configuration found; detecting toolchains and creating {:?}",
            path
        );
        let config = Config::default();
        config.save()?;
        Ok(config)
    }

    /// Writes the configuration with owner-only permissions. The file holds an
    /// auth token, so it must not be group- or world-readable.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            restrict_permissions(parent, 0o700)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        write_private(&path, &json)?;
        info!("Configuration saved.");
        Ok(())
    }
}

/// Creates the file with mode 0600 up front so there is no window in which the
/// token sits in a world-readable file, then tightens any pre-existing file.
fn write_private(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    drop(file);

    // `mode` above only applies when the file is created, so an existing
    // 0644 file from an older build still needs correcting.
    restrict_permissions(path, 0o600)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path, _mode: u32) -> Result<()> {
    // Windows ACL inheritance already keeps per-user config private.
    Ok(())
}

/// Finds the first candidate on `PATH` that is present *and actually runs*.
///
/// Existence on `PATH` is not evidence of a working toolchain: macOS ships stub
/// `javac`/`java` shims that exist, carry the exec bit, and then fail at exec
/// time with "Unable to locate a Java Runtime" when no JDK is installed. So each
/// candidate is executed with `version_arg` and kept only if it exits zero.
///
/// Returns the resolved absolute path rather than the bare name. That matters
/// when a broken shim shadows a working install earlier in `PATH` — recording
/// "javac" would re-select the shim at compile time even though a real one was
/// found further down.
fn detect_binary(candidates: &[&str], version_arg: &str) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;

    for candidate in candidates {
        for dir in std::env::split_paths(&path_var) {
            for bin_path in candidate_paths(&dir, candidate) {
                if is_executable(&bin_path) && runs_successfully(&bin_path, version_arg) {
                    info!("Detected {} at {}", candidate, bin_path.display());
                    return Some(bin_path.to_string_lossy().to_string());
                }
            }
        }
    }

    warn!(
        "No working binary found for any of {:?}; falling back to a bare name, \
         which will fail at compile time if it is not installed",
        candidates
    );
    None
}

fn candidate_paths(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut paths = vec![dir.join(name)];
    if cfg!(windows) {
        paths.push(dir.join(format!("{}.exe", name)));
    }
    paths
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn runs_successfully(path: &Path, version_arg: &str) -> bool {
    std::process::Command::new(path)
        .arg(version_arg)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::detect_binary;

    #[test]
    fn rejects_binary_that_is_not_on_path() {
        assert!(detect_binary(&["cp-client-no-such-binary-xyz"], "--version").is_none());
    }

    #[test]
    fn accepts_a_binary_that_runs() {
        // `true` ignores its arguments and exits 0 on both BSD and GNU.
        let found = detect_binary(&["true"], "--version");
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("true"));
    }

    #[test]
    fn skips_candidates_that_exist_but_fail_to_run() {
        // `false` is on PATH and executable, but always exits non-zero, so it
        // must be rejected and the working candidate chosen instead.
        assert_eq!(
            detect_binary(&["false", "true"], "--version").map(|p| p
                .rsplit('/')
                .next()
                .unwrap()
                .to_string()),
            Some("true".to_string())
        );
    }
}
