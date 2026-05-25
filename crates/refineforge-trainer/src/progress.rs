//! Parse training-tool stdout into per-step metric records.
//!
//! Three parsers ship:
//! - `huggingface`: HF Trainer's dict-printed-per-eval-step format
//!   like `{'loss': 0.412, 'learning_rate': 1.9e-05, 'epoch': 0.42}`
//! - `axolotl`: similar to HF but with slight format differences
//! - `generic`: regex-based `key=value key2=value2` extraction —
//!   works with any tool whose log lines follow the convention.
//!
//! An unrecognised line is NOT an error. The parser simply emits
//! nothing for that line. Per-step records are appended to
//! `progress.jsonl` in the run directory.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressRecord {
    pub timestamp: DateTime<Utc>,
    /// Source line (truncated to 500 chars in the JSONL).
    pub raw: String,
    pub metrics: BTreeMap<String, f64>,
    /// Step / epoch if parseable. Step is preferred; if only epoch
    /// is available, it's stored under metrics["epoch"].
    pub step: Option<u64>,
}

pub trait ProgressParser: Send + Sync {
    /// Parse one stdout line. Return None if the line carries no
    /// metric data (informational logs, banner lines, blank lines).
    fn parse_line(&self, line: &str) -> Option<ProgressRecord>;
}

/// Factory: pick a parser by name.
pub fn parser_for(name: &str) -> Box<dyn ProgressParser> {
    match name {
        "huggingface" | "hf" | "hf_trainer" => Box::new(HuggingFaceParser::new()),
        "axolotl" => Box::new(AxolotlParser::new()),
        _ => Box::new(GenericParser::new()),
    }
}

// ─── HuggingFace Trainer parser ─────────────────────────────────────────
//
// HF Trainer prints lines like (note: single quotes):
//   {'loss': 0.412, 'learning_rate': 1.9e-05, 'epoch': 0.42}
//   {'eval_loss': 0.391, 'eval_runtime': 12.4, 'epoch': 0.42, 'step': 100}
//
// We convert single quotes → double, parse as JSON.

pub struct HuggingFaceParser {
    line_re: Regex,
}

impl HuggingFaceParser {
    pub fn new() -> Self {
        Self {
            // Match a dict-shaped line, possibly preceded by timestamp / log prefix.
            line_re: Regex::new(r"\{[^{}]*\}").unwrap(),
        }
    }
}

impl Default for HuggingFaceParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressParser for HuggingFaceParser {
    fn parse_line(&self, line: &str) -> Option<ProgressRecord> {
        let m = self.line_re.find(line)?;
        let dict_str = m.as_str();
        // HF prints with single quotes; JSON needs doubles.
        let json_str = dict_str.replace('\'', "\"");
        let val: serde_json::Value = serde_json::from_str(&json_str).ok()?;
        let obj = val.as_object()?;
        let mut metrics = BTreeMap::new();
        let mut step: Option<u64> = None;
        for (k, v) in obj {
            if k == "step" {
                if let Some(n) = v.as_u64() {
                    step = Some(n);
                    continue;
                }
            }
            if let Some(n) = v.as_f64() {
                metrics.insert(k.clone(), n);
            } else if let Some(n) = v.as_u64() {
                metrics.insert(k.clone(), n as f64);
            }
        }
        if metrics.is_empty() && step.is_none() {
            return None;
        }
        Some(ProgressRecord {
            timestamp: Utc::now(),
            raw: truncate(line, 500),
            metrics,
            step,
        })
    }
}

// ─── Axolotl parser ─────────────────────────────────────────────────────
//
// Axolotl writes HF-compatible dict lines for the most part; we fall
// back to the HF parser. A future iteration can specialise.

pub struct AxolotlParser(HuggingFaceParser);

impl AxolotlParser {
    pub fn new() -> Self {
        Self(HuggingFaceParser::new())
    }
}

impl Default for AxolotlParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressParser for AxolotlParser {
    fn parse_line(&self, line: &str) -> Option<ProgressRecord> {
        self.0.parse_line(line)
    }
}

// ─── Generic parser ─────────────────────────────────────────────────────
//
// Matches lines like:
//   step=42 loss=0.412 lr=0.0002
//   [step 42] loss=0.412
// Anything that looks like `key=value` (value floatable) is captured.

pub struct GenericParser {
    kv_re: Regex,
    step_re: Regex,
}

impl GenericParser {
    pub fn new() -> Self {
        Self {
            kv_re: Regex::new(
                r"([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([-+]?\d+(?:\.\d*)?(?:[eE][-+]?\d+)?)",
            )
            .unwrap(),
            step_re: Regex::new(r"(?i)step[\s=:]+(\d+)").unwrap(),
        }
    }
}

impl Default for GenericParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressParser for GenericParser {
    fn parse_line(&self, line: &str) -> Option<ProgressRecord> {
        let mut metrics = BTreeMap::new();
        for cap in self.kv_re.captures_iter(line) {
            let key = cap[1].to_string();
            if key == "step" {
                continue;
            }
            if let Ok(v) = cap[2].parse::<f64>() {
                metrics.insert(key, v);
            }
        }
        let step = self
            .step_re
            .captures(line)
            .and_then(|c| c[1].parse::<u64>().ok());
        if metrics.is_empty() && step.is_none() {
            return None;
        }
        Some(ProgressRecord {
            timestamp: Utc::now(),
            raw: truncate(line, 500),
            metrics,
            step,
        })
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hf_parses_canonical_train_line() {
        let p = HuggingFaceParser::new();
        let line = "{'loss': 0.412, 'learning_rate': 1.9e-05, 'epoch': 0.42}";
        let r = p.parse_line(line).expect("must parse");
        assert_eq!(r.metrics.get("loss"), Some(&0.412));
        assert!((r.metrics.get("learning_rate").unwrap() - 1.9e-05).abs() < 1e-9);
        assert_eq!(r.step, None);
    }

    #[test]
    fn hf_parses_eval_line_with_step() {
        let p = HuggingFaceParser::new();
        let line = "{'eval_loss': 0.391, 'eval_runtime': 12.4, 'epoch': 0.42, 'step': 100}";
        let r = p.parse_line(line).expect("must parse");
        assert_eq!(r.step, Some(100));
        assert_eq!(r.metrics.get("eval_loss"), Some(&0.391));
    }

    #[test]
    fn hf_ignores_non_dict_lines() {
        let p = HuggingFaceParser::new();
        assert!(p.parse_line("Loading dataset...").is_none());
        assert!(p.parse_line("").is_none());
        assert!(p.parse_line("***** Running training *****").is_none());
    }

    #[test]
    fn generic_parses_kv_pairs() {
        let p = GenericParser::new();
        let line = "step=42 loss=0.412 lr=0.0002";
        let r = p.parse_line(line).expect("must parse");
        assert_eq!(r.step, Some(42));
        assert_eq!(r.metrics.get("loss"), Some(&0.412));
        assert_eq!(r.metrics.get("lr"), Some(&0.0002));
    }

    #[test]
    fn generic_handles_no_step() {
        let p = GenericParser::new();
        let r = p.parse_line("loss=1.5").expect("must parse");
        assert_eq!(r.step, None);
        assert_eq!(r.metrics.get("loss"), Some(&1.5));
    }

    #[test]
    fn generic_returns_none_for_pure_text() {
        let p = GenericParser::new();
        assert!(p.parse_line("Starting epoch 1").is_none());
    }

    #[test]
    fn factory_dispatches_correctly() {
        // Smoke-test that the factory returns the right parser by
        // exercising parser-specific behaviour.
        let hf = parser_for("huggingface");
        assert!(hf.parse_line("{'loss': 1.0}").is_some());

        let gen = parser_for("generic");
        assert!(gen.parse_line("step=1 loss=1.0").is_some());

        // Unknown name falls back to generic.
        let other = parser_for("unknown");
        assert!(other.parse_line("step=5 metric=2.5").is_some());
    }
}
