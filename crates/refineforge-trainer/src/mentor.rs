//! Mentor capability for the ML / training engineer agent.
//!
//! Lets the `refine agent train` surface run in **teaching mode**: given an
//! operator request to learn about an LLM-engineering topic, produce a
//! `(system_prompt, topic_prompt)` pair that instructs any LLM strategy to
//! teach the topic using daily-life language, analogies, code snippets, and
//! the canonical end-of-topic outro (summary + mental model + beginner
//! mistakes + small exercise).
//!
//! The curriculum is a fixed taxonomy of LLM-engineering topics organized
//! into eleven sections (foundations → datasets/training → fine-tuning →
//! inference → local ecosystem → RAG → agents → model types → deployment →
//! evaluation → real-world skills). Each topic carries common-name aliases
//! so operator queries like `"context window"` and `"context windows"` resolve
//! to the same lesson.
//!
//! Scope (honest):
//!   - Curriculum, rules, prompt builders, alias resolution — REAL.
//!   - The teaching itself runs through whichever LLM strategy the trainer
//!     is paired with. This module emits prompts; it does not call any LLM.

use serde::{Deserialize, Serialize};

// ─── Curriculum ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Curriculum {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    pub topics: Vec<Topic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub name: String,
    /// Lower-cased alternative names; queries are matched against these
    /// after normalization. The canonical `name` is always included
    /// implicitly.
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl Curriculum {
    /// Find a topic by case-insensitive match against canonical name or
    /// any alias. Returns the first match in document order.
    pub fn find(&self, query: &str) -> Result<TopicLocation<'_>, MentorError> {
        let normalized = normalize(query);
        let mut hits: Vec<TopicLocation<'_>> = Vec::new();
        for section in &self.sections {
            for topic in &section.topics {
                if normalize(&topic.name) == normalized
                    || topic.aliases.iter().any(|a| normalize(a) == normalized)
                {
                    hits.push(TopicLocation { section, topic });
                }
            }
        }
        match hits.len() {
            0 => Err(MentorError::TopicNotFound(query.to_string())),
            1 => Ok(hits.into_iter().next().expect("len 1")),
            _ => Err(MentorError::AmbiguousTopic {
                query: query.to_string(),
                matches: hits
                    .iter()
                    .map(|h| format!("{} > {}", h.section.name, h.topic.name))
                    .collect(),
            }),
        }
    }

    /// Total topic count across all sections. Useful for capability reports.
    pub fn topic_count(&self) -> usize {
        self.sections.iter().map(|s| s.topics.len()).sum()
    }
}

/// One topic resolved against the curriculum, retaining its section
/// for context in the rendered prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicLocation<'a> {
    pub section: &'a Section,
    pub topic: &'a Topic,
}

fn normalize(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace(['-', '_', '/'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── Teaching rules ─────────────────────────────────────────────────────

/// Rules that govern the mentor's teaching style. Apply to every topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeachingRules {
    /// High-level pedagogy rules (one per line in the system prompt).
    pub style_rules: Vec<String>,
    /// Required outro sections at the end of each topic, in order.
    pub per_topic_outro: Vec<String>,
}

// ─── Prompts ────────────────────────────────────────────────────────────

/// A rendered mentor prompt pair ready to be sent to an LLM strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentorPrompt {
    pub system_prompt: String,
    pub user_prompt: String,
    /// The section the topic was resolved into (for audit trace).
    pub section_name: String,
    /// The canonical topic name.
    pub topic_name: String,
}

/// Build the mentor's system prompt from the teaching rules.
///
/// The same system prompt is reused across every topic in a session so
/// the LLM strategy can prompt-cache it.
pub fn build_system_prompt(rules: &TeachingRules) -> String {
    let mut s = String::with_capacity(2048);
    s.push_str(
        "You are an expert AI engineer and teacher. Your job is to teach modern \
         LLM engineering and fine-tuning concepts from beginner to advanced level \
         using very simple daily-life language.\n\n",
    );
    s.push_str("Teach step-by-step like a real mentor. Assume the learner is smart but new to the topic.\n\n");
    s.push_str("Style rules:\n");
    for rule in &rules.style_rules {
        s.push_str("- ");
        s.push_str(rule);
        s.push('\n');
    }
    s.push_str("\nAt the end of every topic, always include these sections in order:\n");
    for outro in &rules.per_topic_outro {
        s.push_str("- ");
        s.push_str(outro);
        s.push('\n');
    }
    s.push_str("\nGoal: deep understanding, not memorization.");
    s
}

/// Build the per-topic user prompt for `topic_query` against `curriculum`.
pub fn build_topic_prompt(
    curriculum: &Curriculum,
    topic_query: &str,
) -> Result<MentorPrompt, MentorError> {
    let location = curriculum.find(topic_query)?;
    let user_prompt = format!(
        "Teach me the topic: \"{topic}\"\n\nIt belongs to the section: \"{section}\".\n\n\
         Follow the style rules from the system prompt. Start with the simplest possible \
         explanation, then build up. Do not skip the end-of-topic sections.",
        topic = location.topic.name,
        section = location.section.name,
    );
    Ok(MentorPrompt {
        system_prompt: build_system_prompt(&default_teaching_rules()),
        user_prompt,
        section_name: location.section.name.clone(),
        topic_name: location.topic.name.clone(),
    })
}

// ─── Listing ────────────────────────────────────────────────────────────

/// Human-readable enumeration of the curriculum (sections + topics).
/// Suitable for `refine-train mentor --list` output.
pub fn format_curriculum_listing(c: &Curriculum) -> String {
    let mut s = String::with_capacity(2048);
    s.push_str("Mentor curriculum:\n");
    for section in &c.sections {
        s.push('\n');
        s.push_str(&section.name);
        s.push_str(":\n");
        for topic in &section.topics {
            s.push_str("  - ");
            s.push_str(&topic.name);
            if !topic.aliases.is_empty() {
                s.push_str("  (aliases: ");
                s.push_str(&topic.aliases.join(", "));
                s.push(')');
            }
            s.push('\n');
        }
    }
    s.push_str(&format!(
        "\n{} sections, {} topics total.\n",
        c.sections.len(),
        c.topic_count()
    ));
    s
}

// ─── Defaults ───────────────────────────────────────────────────────────

/// Default teaching rules. Operators can override by deserializing a
/// custom [`TeachingRules`] from YAML/JSON and passing it to
/// [`build_system_prompt`] directly.
pub fn default_teaching_rules() -> TeachingRules {
    TeachingRules {
        style_rules: vec![
            "Use simple English only.".into(),
            "Avoid academic jargon unless strictly necessary.".into(),
            "Explain every difficult word in plain language.".into(),
            "Use real-world analogies and daily-life examples.".into(),
            "Use small code snippets when they make the idea clearer.".into(),
            "Show practical use cases.".into(),
            "Compare related concepts side-by-side when helpful.".into(),
            "Teach from fundamentals first, then advanced concepts.".into(),
        ],
        per_topic_outro: vec![
            "A short summary (3-5 lines).".into(),
            "A simple mental model the learner can carry in their head.".into(),
            "A list of common beginner mistakes to avoid.".into(),
            "A small exercise or mini-project the learner can do in under an hour.".into(),
        ],
    }
}

/// Default eleven-section curriculum.
pub fn default_curriculum() -> Curriculum {
    fn t(name: &str) -> Topic {
        Topic {
            name: name.into(),
            aliases: Vec::new(),
        }
    }
    fn ta(name: &str, aliases: &[&str]) -> Topic {
        Topic {
            name: name.into(),
            aliases: aliases.iter().map(|a| (*a).into()).collect(),
        }
    }
    Curriculum {
        sections: vec![
            Section {
                name: "Foundations".into(),
                topics: vec![
                    t("LLM basics"),
                    ta("How AI models work", &["how llms work"]),
                    t("Tokens"),
                    t("Tokenization"),
                    ta("Context windows", &["context window"]),
                    t("Embeddings"),
                    t("Transformers"),
                    ta("Attention mechanism", &["attention", "self-attention"]),
                    t("Parameters"),
                    ta("Training vs inference", &["training versus inference"]),
                    ta(
                        "Open-source vs closed-source models",
                        &["open vs closed source"],
                    ),
                ],
            },
            Section {
                name: "Datasets & Training".into(),
                topics: vec![
                    ta("SFT datasets", &["supervised fine-tuning datasets"]),
                    t("Instruction tuning"),
                    t("Preference datasets"),
                    t("Synthetic datasets"),
                    t("Data curation"),
                    t("Dataset cleaning"),
                    t("Dataset formatting"),
                    t("Fine-tuning basics"),
                    ta("Continued pretraining", &["continued pre-training"]),
                    t("Hallucination reduction"),
                ],
            },
            Section {
                name: "Fine-Tuning".into(),
                topics: vec![
                    ta("LoRA", &["low-rank adaptation"]),
                    ta("QLoRA", &["quantized lora"]),
                    ta("DPO", &["direct preference optimization"]),
                    ta("RLHF", &["reinforcement learning from human feedback"]),
                    t("Quantization"),
                    t("Model checkpoints"),
                    t("Adapter tuning"),
                    ta("GGUF models", &["gguf"]),
                ],
            },
            Section {
                name: "Inference & Optimization".into(),
                topics: vec![
                    ta("KV cache", &["kv-cache"]),
                    t("Flash Attention"),
                    t("Speculative decoding"),
                    t("Inference optimization"),
                    t("Model serving"),
                    t("Batch inference"),
                    t("GPU basics"),
                    t("VRAM basics"),
                    ta("Latency vs quality tradeoffs", &["latency versus quality"]),
                ],
            },
            Section {
                name: "Local AI Ecosystem".into(),
                topics: vec![
                    ta("llama.cpp", &["llama cpp"]),
                    t("Ollama"),
                    t("vLLM"),
                    ta("MLX", &["apple mlx"]),
                    ta("Hugging Face", &["huggingface", "hf"]),
                    t("Unsloth"),
                    t("Axolotl"),
                    ta("PEFT", &["parameter-efficient fine-tuning"]),
                    ta("TRL library", &["trl"]),
                ],
            },
            Section {
                name: "RAG & Memory".into(),
                topics: vec![
                    ta("RAG", &["retrieval-augmented generation"]),
                    ta("Vector databases", &["vector dbs", "vector db"]),
                    t("Chunking"),
                    t("Retrieval pipelines"),
                    t("AI memory systems"),
                    t("Semantic search"),
                ],
            },
            Section {
                name: "Agents & Workflows".into(),
                topics: vec![
                    t("Prompt engineering"),
                    t("System prompts"),
                    t("Tool calling"),
                    t("Function calling"),
                    t("AI agents"),
                    t("Agentic workflows"),
                    t("Multi-agent systems"),
                    t("Browser agents"),
                ],
            },
            Section {
                name: "Model Types".into(),
                topics: vec![
                    ta(
                        "VLMs",
                        &["vision-language models", "vision language models"],
                    ),
                    ta("SLMs", &["small language models"]),
                    t("Dense models"),
                    ta("MoE models", &["mixture of experts"]),
                    t("Coding models"),
                    t("Reasoning models"),
                ],
            },
            Section {
                name: "Deployment".into(),
                topics: vec![
                    t("Local inference"),
                    t("On-device AI"),
                    t("API serving"),
                    t("Cloud GPUs"),
                    t("Edge AI basics"),
                ],
            },
            Section {
                name: "Evaluation".into(),
                topics: vec![
                    t("AI benchmarks"),
                    ta("Human evals", &["human evaluation"]),
                    t("Cost-per-token analysis"),
                    t("Speed benchmarking"),
                    t("Quality benchmarking"),
                ],
            },
            Section {
                name: "Real-World Skills".into(),
                topics: vec![
                    t("Building chatbots"),
                    t("Building AI copilots"),
                    t("AI automation"),
                    t("AI SaaS workflows"),
                    t("AI coding workflows"),
                    t("AI orchestration systems"),
                    t("AI product thinking"),
                ],
            },
        ],
    }
}

// ─── Errors ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MentorError {
    TopicNotFound(String),
    AmbiguousTopic { query: String, matches: Vec<String> },
}

impl std::fmt::Display for MentorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TopicNotFound(q) => write!(f, "topic not found in curriculum: {q:?}"),
            Self::AmbiguousTopic { query, matches } => write!(
                f,
                "query {query:?} matched multiple curriculum entries: {matches:?}"
            ),
        }
    }
}

impl std::error::Error for MentorError {}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_curriculum_has_all_eleven_sections() {
        let c = default_curriculum();
        assert_eq!(c.sections.len(), 11);
        assert!(c.topic_count() >= 70);
    }

    #[test]
    fn find_resolves_canonical_name() {
        let c = default_curriculum();
        let loc = c.find("LoRA").unwrap();
        assert_eq!(loc.section.name, "Fine-Tuning");
        assert_eq!(loc.topic.name, "LoRA");
    }

    #[test]
    fn find_resolves_alias_case_insensitive() {
        let c = default_curriculum();
        let loc = c.find("low-rank adaptation").unwrap();
        assert_eq!(loc.topic.name, "LoRA");
        let loc = c.find("CONTEXT WINDOW").unwrap();
        assert_eq!(loc.topic.name, "Context windows");
    }

    #[test]
    fn find_returns_not_found_for_unknown() {
        let c = default_curriculum();
        let err = c.find("quantum bogosort").unwrap_err();
        assert!(matches!(err, MentorError::TopicNotFound(_)));
    }

    #[test]
    fn system_prompt_includes_rules_and_outro() {
        let rules = default_teaching_rules();
        let s = build_system_prompt(&rules);
        assert!(s.contains("simple English"));
        assert!(s.contains("real-world analogies"));
        assert!(s.contains("summary"));
        assert!(s.contains("mental model"));
        assert!(s.contains("beginner mistakes"));
        assert!(s.contains("exercise"));
    }

    #[test]
    fn topic_prompt_resolves_and_quotes_section() {
        let c = default_curriculum();
        let p = build_topic_prompt(&c, "QLoRA").unwrap();
        assert_eq!(p.section_name, "Fine-Tuning");
        assert_eq!(p.topic_name, "QLoRA");
        assert!(p.user_prompt.contains("QLoRA"));
        assert!(p.user_prompt.contains("Fine-Tuning"));
        assert!(!p.system_prompt.is_empty());
    }

    #[test]
    fn normalize_collapses_separators_and_case() {
        assert_eq!(normalize("KV-Cache"), "kv cache");
        assert_eq!(normalize("  Hugging   Face  "), "hugging face");
        assert_eq!(normalize("function_calling"), "function calling");
    }

    #[test]
    fn rules_round_trip_through_serde() {
        let r = default_teaching_rules();
        let json = serde_json::to_string(&r).unwrap();
        let back: TeachingRules = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn curriculum_round_trips_through_serde() {
        let c = default_curriculum();
        let json = serde_json::to_string(&c).unwrap();
        let back: Curriculum = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn listing_includes_every_section_and_topic_count() {
        let c = default_curriculum();
        let s = format_curriculum_listing(&c);
        for section in &c.sections {
            assert!(
                s.contains(&section.name),
                "section {} missing",
                section.name
            );
        }
        assert!(s.contains("11 sections"));
        assert!(s.contains("topics total"));
    }
}
