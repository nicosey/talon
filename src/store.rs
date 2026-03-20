use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;

/// Append one job-run record to the store file as a JSON line.
///
/// Format (one record per line):
/// {"ts":"2026-03-20T07:00:01Z","job":"Robotics Briefing","success":true,"output":"..."}
///
/// If `path` is empty the record is printed to stdout instead (useful in tests).
pub fn append(path: &str, job: &str, success: bool, output: &str) {
    let record = serde_json::json!({
        "ts":      Utc::now().to_rfc3339(),
        "job":     job,
        "success": success,
        "output":  output,
    });

    if path.is_empty() {
        println!("[TALON store] {}", record);
        return;
    }

    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{}", record) {
                eprintln!("[TALON] store: write error ({}): {}", path, e);
            }
        }
        Err(e) => eprintln!("[TALON] store: cannot open {} — {}", path, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> String {
        format!("/tmp/talon_store_test_{}.jsonl", name)
    }

    fn cleanup(path: &str) {
        let _ = fs::remove_file(path);
    }

    // ── file creation ─────────────────────────────────────────────────────────

    #[test]
    fn creates_file_if_missing() {
        let path = tmp("creates");
        cleanup(&path);
        append(&path, "job", true, "output");
        assert!(fs::metadata(&path).is_ok());
        cleanup(&path);
    }

    #[test]
    fn empty_path_does_not_create_file() {
        // should print to stdout instead — no panic
        append("", "job", true, "output");
    }

    // ── record format ─────────────────────────────────────────────────────────

    #[test]
    fn record_is_valid_json() {
        let path = tmp("valid_json");
        cleanup(&path);
        append(&path, "My Job", true, "hello");
        let content = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(v["job"], "My Job");
        assert_eq!(v["success"], true);
        assert_eq!(v["output"], "hello");
        assert!(v["ts"].as_str().is_some());
        cleanup(&path);
    }

    #[test]
    fn failure_record_has_success_false() {
        let path = tmp("failure");
        cleanup(&path);
        append(&path, "job", false, "timeout");
        let content = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(v["success"], false);
        assert_eq!(v["output"], "timeout");
        cleanup(&path);
    }

    // ── append behaviour ──────────────────────────────────────────────────────

    #[test]
    fn each_call_appends_a_new_line() {
        let path = tmp("multiline");
        cleanup(&path);
        append(&path, "A", true, "first");
        append(&path, "B", false, "second");
        append(&path, "C", true, "third");
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        cleanup(&path);
    }

    #[test]
    fn second_run_does_not_overwrite_first() {
        let path = tmp("no_overwrite");
        cleanup(&path);
        append(&path, "job", true, "run 1");
        append(&path, "job", true, "run 2");
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("run 1"));
        assert!(content.contains("run 2"));
        cleanup(&path);
    }

    #[test]
    fn each_line_is_independently_valid_json() {
        let path = tmp("each_line_json");
        cleanup(&path);
        for i in 0..5 {
            append(&path, &format!("job{}", i), i % 2 == 0, &format!("out{}", i));
        }
        let content = fs::read_to_string(&path).unwrap();
        for line in content.lines() {
            let v: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("line not valid JSON: {line} — {e}"));
            assert!(v["ts"].as_str().is_some());
        }
        cleanup(&path);
    }

    // ── field content ─────────────────────────────────────────────────────────

    #[test]
    fn output_with_special_characters_round_trips() {
        let path = tmp("special_chars");
        cleanup(&path);
        let raw = "line1\nline2\t\"quoted\" & <escaped>";
        append(&path, "job", true, raw);
        let content = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(v["output"].as_str().unwrap(), raw);
        cleanup(&path);
    }

    #[test]
    fn ts_field_is_rfc3339() {
        let path = tmp("ts_format");
        cleanup(&path);
        append(&path, "job", true, "x");
        let content = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        let ts = v["ts"].as_str().unwrap();
        // RFC3339 timestamps contain 'T' and 'Z' or offset
        assert!(ts.contains('T'), "ts not RFC3339: {ts}");
        cleanup(&path);
    }
}
