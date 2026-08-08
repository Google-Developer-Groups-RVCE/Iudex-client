use std::path::PathBuf;
use tracing::info;

use crate::api::client::ApiClient;
use crate::api::models::{Contest, Problem, TestInput};
use crate::config::Config;
use crate::error::{ClientError, Result};

pub struct ContestManager {
    api: ApiClient,
    cache_dir: PathBuf,
}

impl ContestManager {
    pub fn new(config: &Config) -> Result<Self> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| ClientError::ConfigError("Could not determine user home directory".to_string()))?;
        let cache_dir = PathBuf::from(home).join(".cache").join("cp-client");
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            api: ApiClient::new(config),
            cache_dir,
        })
    }

    pub async fn list_contests(&self) -> Result<Vec<Contest>> {
        self.api.get_contests().await
    }

    pub async fn get_contest(&self, contest_id: &str) -> Result<Contest> {
        self.api.get_contest(contest_id).await
    }

    pub async fn get_problem(&self, problem_id: &str) -> Result<Problem> {
        self.api.get_problem(problem_id).await
    }

    pub async fn fetch_test_inputs(&self, problem_id: &str) -> Result<Vec<TestInput>> {
        let cache_file = self.cache_dir.join(format!("tests_{}.json", problem_id));
        
        // Fetch fresh inputs from API
        match self.api.get_test_inputs(problem_id).await {
            Ok(inputs) => {
                if let Ok(json) = serde_json::to_string(&inputs) {
                    let _ = std::fs::write(&cache_file, json);
                }
                Ok(inputs)
            }
            Err(err) => {
                // Try fallback to cached test inputs if available
                if cache_file.exists() {
                    info!("Network request failed; falling back to cached test inputs for {}", problem_id);
                    let content = std::fs::read_to_string(&cache_file)?;
                    let inputs: Vec<TestInput> = serde_json::from_str(&content)?;
                    Ok(inputs)
                } else {
                    Err(err)
                }
            }
        }
    }
}
