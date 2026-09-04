use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::api::models::VerdictStatus;
use crate::error::Result;

const HISTORY_FILE: &str = "history.jsonl";

/// One line of the local submission log. Everything here is already known at submit time, so no extra server round-trip is needed to record it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionRecord {
    pub submission_id: String,
    pub problem_id: String,
    pub language: String,
    pub verdict: VerdictStatus,
    pub passed_test_cases: usize,
    pub total_test_cases: usize,
    pub timestamp: DateTime<Utc>,
}

/// Appends a record as one JSON line. Append-only keeps writes O(1) and avoids a read-modify-write race if two submissions land close together.
pub fn append_record(cache_dir: &Path, record: &SubmissionRecord) -> Result<()> {
    let path = cache_dir.join(HISTORY_FILE);
    let line = serde_json::to_string(record)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

/// Reads every well-formed record. A single corrupt line is skipped and logged rather than sinking the whole history.
pub fn load_history(cache_dir: &Path) -> Result<Vec<SubmissionRecord>> {
    let path = cache_dir.join(HISTORY_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let mut records = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<SubmissionRecord>(line) {
            Ok(record) => records.push(record),
            Err(err) => warn!("Skipping malformed submission history entry: {}", err),
        }
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, problem: &str) -> SubmissionRecord {
        SubmissionRecord {
            submission_id: id.to_string(),
            problem_id: problem.to_string(),
            language: "cpp".to_string(),
            verdict: VerdictStatus::Accepted,
            passed_test_cases: 3,
            total_test_cases: 3,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn appends_and_reads_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        append_record(dir.path(), &sample("sub_1", "A")).unwrap();
        append_record(dir.path(), &sample("sub_2", "B")).unwrap();

        let history = load_history(dir.path()).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].submission_id, "sub_1");
        assert_eq!(history[1].problem_id, "B");
    }

    #[test]
    fn missing_file_is_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_history(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn malformed_line_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        append_record(dir.path(), &sample("sub_1", "A")).unwrap();
        std::fs::write(
            dir.path().join(HISTORY_FILE),
            "{ not valid json\n{\"broken\":true}\n",
        )
        .unwrap();
        append_record(dir.path(), &sample("sub_2", "B")).unwrap();

        let history = load_history(dir.path()).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].submission_id, "sub_2");
    }
}
