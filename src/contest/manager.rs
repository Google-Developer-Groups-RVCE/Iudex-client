use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{info, warn};

use crate::api::client::ApiClient;
use crate::api::models::{Contest, Problem, TestInput};
use crate::config::Config;
use crate::contest::history::{self, SubmissionRecord};
use crate::error::{ClientError, Result};

pub struct ContestManager {
    api: ApiClient,
    cache_dir: PathBuf,
    cache_ttl: Duration,
}

impl ContestManager {
    pub fn new(config: &Config) -> Result<Self> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| {
                ClientError::ConfigError("Could not determine user home directory".to_string())
            })?;
        let cache_dir = PathBuf::from(home).join(".cache").join("cp-client");
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            api: ApiClient::new(config),
            cache_dir,
            cache_ttl: Duration::from_secs(config.cache_ttl_secs),
        })
    }

    pub async fn list_contests(&self) -> Result<Vec<Contest>> {
        self.api.get_contests().await
    }

    /// Contest metadata (title, window, problem list) is cached so `contest <id>`
    /// keeps working offline, mirroring the problem/test-input paths.
    pub async fn fetch_contest(&self, contest_id: &str) -> Result<Contest> {
        let cache_file = self.cache_path("contest", contest_id);
        match self.api.get_contest(contest_id).await {
            Ok(contest) => {
                write_cache(&cache_file, &contest);
                Ok(contest)
            }
            Err(err) => {
                self.fall_back_to_cache(&cache_file, &format!("contest {}", contest_id), err)
            }
        }
    }

    /// Appends a verdict to the local submission log. Left uncached-agnostic on
    /// purpose: history is a client-side record, independent of the server.
    pub fn record_submission(&self, record: &SubmissionRecord) -> Result<()> {
        history::append_record(&self.cache_dir, record)
    }

    pub fn submission_history(&self) -> Result<Vec<SubmissionRecord>> {
        history::load_history(&self.cache_dir)
    }

    /// Problem metadata carries the authoritative time and memory limits, so it
    /// is cached alongside the test inputs and stays usable offline.
    pub async fn fetch_problem(&self, problem_id: &str) -> Result<Problem> {
        let cache_file = self.cache_path("problem", problem_id);
        match self.api.get_problem(problem_id).await {
            Ok(problem) => {
                write_cache(&cache_file, &problem);
                Ok(problem)
            }
            Err(err) => self.fall_back_to_cache(&cache_file, &format!("problem {}", problem_id), err),
        }
    }

    pub async fn fetch_test_inputs(&self, problem_id: &str) -> Result<Vec<TestInput>> {
        let cache_file = self.cache_path("tests", problem_id);
        match self.api.get_test_inputs(problem_id).await {
            Ok(inputs) => {
                write_cache(&cache_file, &inputs);
                Ok(inputs)
            }
            Err(err) => self.fall_back_to_cache(
                &cache_file,
                &format!("test inputs for {}", problem_id),
                err,
            ),
        }
    }

    fn cache_path(&self, kind: &str, problem_id: &str) -> PathBuf {
        self.cache_dir
            .join(format!("{}_{}.json", kind, cache_key(problem_id)))
    }

    /// Falls back to cached data, but only while it is still fresh. An
    /// unbounded fallback means one network blip pins a stale copy for good.
    fn fall_back_to_cache<T: DeserializeOwned>(
        &self,
        cache_file: &Path,
        label: &str,
        err: ClientError,
    ) -> Result<T> {
        match cache_age(cache_file) {
            Some(age) if age <= self.cache_ttl => {
                info!(
                    "Network request failed; falling back to cached {} (age {}s)",
                    label,
                    age.as_secs()
                );
                let content = std::fs::read_to_string(cache_file)?;
                Ok(serde_json::from_str(&content)?)
            }
            Some(age) => {
                warn!(
                    "Cached {} is stale ({}s old, TTL {}s); refusing to use it",
                    label,
                    age.as_secs(),
                    self.cache_ttl.as_secs()
                );
                Err(err)
            }
            None => Err(err),
        }
    }
}

fn write_cache<T: Serialize>(cache_file: &Path, value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        let _ = std::fs::write(cache_file, json);
    }
}

/// Problem ids arrive from the command line and are interpolated into a
/// filename, so anything that could escape the cache directory (`..`, `/`) is
/// replaced rather than trusted.
fn cache_key(problem_id: &str) -> String {
    problem_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn cache_age(path: &Path) -> Option<Duration> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

#[cfg(test)]
mod tests {
    use super::cache_key;

    #[test]
    fn ordinary_problem_ids_are_unchanged() {
        assert_eq!(cache_key("A"), "A");
        assert_eq!(cache_key("div2-problem_3"), "div2-problem_3");
    }

    #[test]
    fn traversal_sequences_cannot_escape_the_cache_directory() {
        assert_eq!(cache_key("../../etc/passwd"), "______etc_passwd");
        assert_eq!(cache_key("/absolute"), "_absolute");
        assert!(!cache_key("..\\windows").contains('\\'));
    }
}
