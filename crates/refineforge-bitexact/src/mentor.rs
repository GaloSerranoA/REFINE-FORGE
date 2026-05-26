//! Mentor capability for the GPU / CUDA kernel engineer agent.
//!
//! Lets `refine-bitexact` run in **teaching mode**: given an operator request
//! to learn a CUDA / GPU-kernel topic, produce a `(system_prompt,
//! topic_prompt)` pair that instructs any LLM strategy to teach the topic
//! using daily-life language, analogies, code snippets, and the canonical
//! end-of-topic outro (summary + mental model + beginner mistakes + small
//! exercise).
//!
//! The curriculum is a fixed taxonomy of CUDA / GPU-kernel topics organized
//! into seven sections (beginner → core architecture → practical guides →
//! advanced optimization → Python and high-level CUDA → modern releases →
//! Refine-Forge kernel specifics). Each section also carries a list of
//! external book titles for operators who want primary-source reading —
//! titles are bibliographic facts, not curriculum content. Topic
//! descriptions and aliases are authored here from common CUDA knowledge.
//!
//! Scope (honest):
//!   - Curriculum, rules, prompt builders, alias resolution — REAL.
//!   - The teaching itself runs through whichever LLM strategy the operator
//!     pairs with `refine-bitexact mentor`. This module emits prompts; it
//!     does not call any LLM. It also does not validate the LLM's CUDA
//!     code — that's the kernel engineer's job, and `refine-bitexact run`
//!     is the gate that catches non-determinism in any kernel produced.

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
    /// External book titles relevant to this section. Bibliographic
    /// reference only — titles are not curriculum content. Operators
    /// pick which to read; the mentor teaches from its own taxonomy.
    #[serde(default)]
    pub references: Vec<String>,
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
        "You are an expert GPU and CUDA kernel engineer and teacher. Your job is to \
         teach CUDA programming, GPU architecture, and kernel optimization from \
         beginner to advanced level using very simple daily-life language.\n\n",
    );
    s.push_str(
        "Teach step-by-step like a real mentor. Assume the learner is a smart \
         programmer who is new to GPU work. They know what a function and a loop \
         are; they do NOT yet know what a warp, an SM, or a coalesced load is.\n\n",
    );
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
    s.push_str(
        "\nGoal: deep understanding of how GPUs actually execute code, not memorization \
         of API names. When a topic touches floating-point order, atomics, or algorithm \
         selection, always mention the determinism implication — the operator's job is \
         to ship kernels that pass the bit-exact reproducibility gate.",
    );
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
         explanation, then build up. Show at least one small code snippet (CUDA C++ or \
         Python via CuPy/Numba, whichever fits the topic best). Do not skip the \
         end-of-topic sections.",
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

/// Human-readable enumeration of the curriculum (sections + topics +
/// reference book titles). Suitable for `refine-bitexact mentor --list`
/// output.
pub fn format_curriculum_listing(c: &Curriculum) -> String {
    let mut s = String::with_capacity(4096);
    s.push_str("CUDA / GPU Kernel mentor curriculum:\n");
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
        if !section.references.is_empty() {
            s.push_str("  references:\n");
            for r in &section.references {
                s.push_str("    * ");
                s.push_str(r);
                s.push('\n');
            }
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
            "Use real-world analogies — highways, factories, kitchens, classrooms — to \
             explain threads, blocks, warps, memory tiers, and synchronization."
                .into(),
            "Show small CUDA C++ or Python code snippets that actually compile and run.".into(),
            "For every kernel example, state the launch configuration: grid, block, and \
             shared-memory size when relevant."
                .into(),
            "Compare GPU vs CPU behaviour whenever it clarifies why a GPU technique exists.".into(),
            "Name what can go wrong: race conditions, illegal memory access, out-of-bounds \
             writes, warp divergence, register spilling, occupancy collapse."
                .into(),
            "Teach from fundamentals first, then optimization, then hardware specifics.".into(),
            "When the topic touches atomics, floating-point order, mixed precision, or \
             algorithm selection (cuBLAS, cuDNN), explicitly call out the determinism \
             implication for bit-exact reproducibility."
                .into(),
        ],
        per_topic_outro: vec![
            "A short summary (3-5 lines).".into(),
            "A simple mental model the learner can carry in their head.".into(),
            "A list of common beginner mistakes to avoid.".into(),
            "A small exercise — a kernel under ~40 lines the learner can compile with \
             nvcc, run on a single GPU (or free Colab T4), and inspect with Nsight."
                .into(),
        ],
    }
}

/// Default seven-section curriculum.
///
/// The section organization follows the public structure of the
/// awesome-cuda-books community list (a bibliographic taxonomy of where
/// CUDA books fall). Topic names and aliases are authored from common
/// CUDA knowledge — not lifted from any single book. Each section's
/// `references` field carries the book titles that are commonly placed
/// in that bucket; operators may consult them for primary-source depth.
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
                name: "Beginner / Getting Started".into(),
                topics: vec![
                    ta("GPU vs CPU architecture", &["gpu versus cpu"]),
                    ta(
                        "Host vs device memory",
                        &["host versus device memory", "device memory model"],
                    ),
                    ta(
                        "Kernel launch syntax",
                        &["kernel launch", "triple chevron", "<<<grid block>>>"],
                    ),
                    ta(
                        "Thread block and grid hierarchy",
                        &[
                            "thread hierarchy",
                            "grid block thread",
                            "threadIdx blockIdx",
                        ],
                    ),
                    ta(
                        "cudaMalloc and cudaFree",
                        &["device memory allocation", "cudamalloc"],
                    ),
                    ta(
                        "cudaMemcpy",
                        &["host-device transfer", "cudamemcpy", "memory transfer"],
                    ),
                    t("Hello-world CUDA kernel"),
                    ta(
                        "CUDA error checking",
                        &["cudaGetLastError", "error checking", "cuda errors"],
                    ),
                    ta("nvcc compiler basics", &["nvcc", "nvcc flags"]),
                ],
                references: vec![
                    "CUDA by Example: An Introduction to General-Purpose GPU Programming".into(),
                    "Learn CUDA Programming".into(),
                    "CUDA for Engineers: An Introduction to High-Performance Parallel Computing"
                        .into(),
                ],
            },
            Section {
                name: "Core Architecture & Parallel Programming".into(),
                topics: vec![
                    ta(
                        "SIMT execution model",
                        &["simt", "single instruction multiple thread"],
                    ),
                    ta(
                        "Warps and warp scheduling",
                        &["warp", "warps", "warp scheduler"],
                    ),
                    ta(
                        "Streaming multiprocessors",
                        &["sm", "sms", "streaming multiprocessor"],
                    ),
                    ta(
                        "Warp divergence",
                        &["thread divergence", "branch divergence"],
                    ),
                    ta("GPU memory hierarchy", &["memory hierarchy"]),
                    t("Registers and local memory"),
                    ta("Shared memory", &["smem", "__shared__"]),
                    ta("Global memory", &["device global memory"]),
                    ta("L1 and L2 caches", &["l1 cache", "l2 cache"]),
                    ta(
                        "Block-level synchronization",
                        &["__syncthreads", "syncthreads"],
                    ),
                    ta("Atomic operations", &["atomics", "atomicAdd", "atomicCAS"]),
                    ta(
                        "Memory coalescing",
                        &["coalesced access", "coalesced loads"],
                    ),
                ],
                references: vec![
                    "Programming Massively Parallel Processors: A Hands-on Approach \
                     (3rd Edition)"
                        .into(),
                ],
            },
            Section {
                name: "Practical & Hands-On Guides".into(),
                topics: vec![
                    ta(
                        "Matrix multiplication kernel",
                        &["matmul", "gemm", "naive matmul"],
                    ),
                    ta(
                        "Parallel reduction",
                        &["reduction", "tree reduction", "parallel sum"],
                    ),
                    ta(
                        "Scan and prefix sum",
                        &["scan", "prefix sum", "inclusive scan", "exclusive scan"],
                    ),
                    ta("Stencil computations", &["stencil", "stencil kernel"]),
                    ta(
                        "Tiling with shared memory",
                        &["tiled matmul", "shared-memory tiling", "tiling"],
                    ),
                    ta("CUDA streams", &["streams", "cudaStream"]),
                    ta("Asynchronous memcpy", &["async memcpy", "cudaMemcpyAsync"]),
                    ta("Events and timing", &["cudaEvent", "events", "timing"]),
                    ta("Pinned memory", &["page-locked memory", "cudaMallocHost"]),
                    ta(
                        "Multi-GPU programming basics",
                        &["multi gpu", "peer access", "p2p"],
                    ),
                ],
                references: vec![
                    "Programming in Parallel with CUDA: A Practical Guide".into(),
                    "Professional CUDA C Programming".into(),
                    "GPU Parallel Program Development Using CUDA".into(),
                    "CUDA for Deep Learning".into(),
                ],
            },
            Section {
                name: "Advanced / Optimization / Reference".into(),
                topics: vec![
                    ta("Occupancy analysis", &["occupancy"]),
                    t("Latency hiding"),
                    ta(
                        "Shared memory bank conflicts",
                        &["bank conflicts", "smem bank conflict"],
                    ),
                    ta(
                        "Memory throughput optimization",
                        &["memory bandwidth", "throughput tuning"],
                    ),
                    t("Instruction throughput"),
                    ta("Roofline model", &["roofline"]),
                    ta(
                        "Tensor cores",
                        &["wmma", "mma", "tensor core", "tensor cores"],
                    ),
                    ta(
                        "Cooperative groups",
                        &["coop groups", "cuda::cooperative_groups"],
                    ),
                    t("Dynamic parallelism"),
                    ta("PTX inspection", &["ptx assembly", "ptx"]),
                    ta(
                        "Nsight Compute profiling",
                        &["nsight compute", "ncu", "nsight"],
                    ),
                    ta("Nsight Systems profiling", &["nsight systems", "nsys"]),
                    ta(
                        "Mixed-precision arithmetic",
                        &["fp16", "tf32", "bf16", "mixed precision"],
                    ),
                ],
                references: vec![
                    "The CUDA Handbook: A Comprehensive Guide to GPU Programming".into(),
                    "CUDA Programming: A Developer's Guide to Parallel Computing with GPUs".into(),
                    "CUDA Application Design and Development".into(),
                    "CUDA C++ Optimization".into(),
                    "CUDA C++ Debugging".into(),
                ],
            },
            Section {
                name: "Python & High-Level CUDA".into(),
                topics: vec![
                    t("CuPy"),
                    t("PyCUDA"),
                    ta("Numba CUDA", &["@cuda.jit", "numba", "numba cuda"]),
                    ta("cuBLAS", &["cublas"]),
                    ta("cuDNN", &["cudnn"]),
                    ta("cuFFT", &["cufft"]),
                    ta("Triton language", &["triton", "openai triton"]),
                    ta("Thrust C++ library", &["thrust"]),
                    ta("CUB primitives", &["cub"]),
                    ta("GPU programming with C++", &["c++ cuda", "modern c++ gpu"]),
                ],
                references: vec![
                    "Hands-On GPU Programming with Python and CUDA".into(),
                    "GPU Programming with C++ and CUDA".into(),
                ],
            },
            Section {
                name: "Modern Releases (2022–2026)".into(),
                topics: vec![
                    ta("CUDA Graphs", &["graph capture", "cuda graph"]),
                    ta("Unified Memory", &["managed memory", "cudaMallocManaged"]),
                    ta("Memory pools", &["cudaMemPool"]),
                    ta(
                        "C++20 and C++26 features in CUDA",
                        &["modern c++ in cuda", "c++26 cuda"],
                    ),
                    ta(
                        "Tensor Memory Accelerator",
                        &["tma", "tensor memory accelerator"],
                    ),
                    ta("Hopper architecture features", &["hopper", "h100"]),
                    ta(
                        "Ada Lovelace architecture features",
                        &["ada lovelace", "ada"],
                    ),
                    ta(
                        "Stream capture",
                        &["cuda 12 streams capture", "streams capture"],
                    ),
                    ta(
                        "Asynchronous SM-level features",
                        &["async sm", "cluster launch"],
                    ),
                ],
                references: vec![
                    "Programming in Parallel with CUDA (modern edition)".into(),
                    "Programming Massively Parallel Processors (3rd Ed.)".into(),
                    "GPU Programming with C++ and CUDA".into(),
                    "CUDA for Deep Learning".into(),
                    "CUDA C++ Optimization".into(),
                    "CUDA C++ Debugging".into(),
                    "CUDA Programming from Basics to Advanced".into(),
                    "CUDA Mastery".into(),
                    "CUDA in Action".into(),
                    "Mastering CUDA C++ Programming".into(),
                    "High-Performance Computing with C++26 and CUDA 13".into(),
                ],
            },
            Section {
                name: "Refine-Forge Kernel Specifics".into(),
                topics: vec![
                    ta(
                        "Bit-exact reproducibility gate",
                        &["refine-bitexact", "reproducibility gate", "bit exact gate"],
                    ),
                    ta(
                        "SHA-256 output hashing for kernel outputs",
                        &["sha256 hashing", "output hashing"],
                    ),
                    ta(
                        "atomicAdd nondeterminism",
                        &["atomic ordering", "nondeterministic atomics"],
                    ),
                    ta(
                        "cuBLAS algorithm selection determinism",
                        &["cublas determinism", "deterministic gemm"],
                    ),
                    ta(
                        "Floating-point determinism on GPU",
                        &["fp determinism", "floating point order"],
                    ),
                    ta(
                        "Environment variables for determinism",
                        &[
                            "CUBLAS_WORKSPACE_CONFIG",
                            "cublas workspace config",
                            "deterministic env",
                        ],
                    ),
                    ta(
                        "Kernel experiment manifest format",
                        &["kernel manifest", "experiment yaml"],
                    ),
                    ta(
                        "Run-all CI gate workflow",
                        &["run-all", "ci gate", "run_all"],
                    ),
                ],
                references: vec!["docs/bit-exact-reproducibility.md (in this repository)".into()],
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
    fn default_curriculum_has_all_seven_sections() {
        let c = default_curriculum();
        assert_eq!(c.sections.len(), 7);
        assert!(
            c.topic_count() >= 60,
            "expected >=60 topics, got {}",
            c.topic_count()
        );
    }

    #[test]
    fn every_section_carries_at_least_one_reference() {
        let c = default_curriculum();
        for section in &c.sections {
            assert!(
                !section.references.is_empty(),
                "section {} has no reference books listed",
                section.name
            );
        }
    }

    #[test]
    fn find_resolves_canonical_name() {
        let c = default_curriculum();
        let loc = c.find("Warps and warp scheduling").unwrap();
        assert_eq!(loc.section.name, "Core Architecture & Parallel Programming");
        assert_eq!(loc.topic.name, "Warps and warp scheduling");
    }

    #[test]
    fn find_resolves_alias_case_insensitive() {
        let c = default_curriculum();
        let loc = c.find("warp").unwrap();
        assert_eq!(loc.topic.name, "Warps and warp scheduling");
        let loc = c.find("GEMM").unwrap();
        assert_eq!(loc.topic.name, "Matrix multiplication kernel");
        let loc = c.find("simt").unwrap();
        assert_eq!(loc.topic.name, "SIMT execution model");
        let loc = c.find("CUBLAS_WORKSPACE_CONFIG").unwrap();
        assert_eq!(loc.topic.name, "Environment variables for determinism");
    }

    #[test]
    fn find_returns_not_found_for_unknown() {
        let c = default_curriculum();
        let err = c.find("quantum bogosort").unwrap_err();
        assert!(matches!(err, MentorError::TopicNotFound(_)));
    }

    #[test]
    fn system_prompt_includes_rules_outro_and_determinism_note() {
        let rules = default_teaching_rules();
        let s = build_system_prompt(&rules);
        assert!(s.contains("simple English"));
        assert!(s.contains("real-world analogies"));
        assert!(s.contains("summary"));
        assert!(s.contains("mental model"));
        assert!(s.contains("beginner mistakes"));
        assert!(s.contains("exercise"));
        // Determinism callout is load-bearing for the bitexact agent.
        assert!(
            s.to_lowercase().contains("determinism"),
            "system prompt must mention determinism"
        );
        assert!(s.to_lowercase().contains("bit-exact"));
    }

    #[test]
    fn topic_prompt_resolves_and_quotes_section() {
        let c = default_curriculum();
        let p = build_topic_prompt(&c, "shared memory").unwrap();
        assert_eq!(p.section_name, "Core Architecture & Parallel Programming");
        assert_eq!(p.topic_name, "Shared memory");
        assert!(p.user_prompt.contains("Shared memory"));
        assert!(p
            .user_prompt
            .contains("Core Architecture & Parallel Programming"));
        assert!(!p.system_prompt.is_empty());
    }

    #[test]
    fn topic_prompt_for_refineforge_specific_topic_resolves() {
        // The Refine-Forge tie-in section must be queryable by the
        // operator-facing alias `refine-bitexact`.
        let c = default_curriculum();
        let p = build_topic_prompt(&c, "refine-bitexact").unwrap();
        assert_eq!(p.section_name, "Refine-Forge Kernel Specifics");
        assert_eq!(p.topic_name, "Bit-exact reproducibility gate");
    }

    #[test]
    fn normalize_collapses_separators_and_case() {
        assert_eq!(normalize("KV-Cache"), "kv cache");
        assert_eq!(
            normalize("  Streaming   Multiprocessor  "),
            "streaming multiprocessor"
        );
        assert_eq!(normalize("cuda_streams"), "cuda streams");
        assert_eq!(normalize("WARP/DIVERGENCE"), "warp divergence");
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
    fn listing_includes_every_section_topic_and_references() {
        let c = default_curriculum();
        let s = format_curriculum_listing(&c);
        for section in &c.sections {
            assert!(
                s.contains(&section.name),
                "section {} missing",
                section.name
            );
            for topic in &section.topics {
                assert!(
                    s.contains(&topic.name),
                    "topic {} missing in listing",
                    topic.name
                );
            }
            for r in &section.references {
                assert!(s.contains(r), "reference {r:?} missing in listing");
            }
        }
        assert!(s.contains("7 sections"));
        assert!(s.contains("topics total"));
    }

    #[test]
    fn topic_names_are_unique_within_curriculum() {
        let c = default_curriculum();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for section in &c.sections {
            for topic in &section.topics {
                let key = normalize(&topic.name);
                assert!(
                    seen.insert(key.clone()),
                    "duplicate topic across curriculum: {key:?}"
                );
            }
        }
    }

    #[test]
    fn aliases_do_not_collide_across_topics() {
        let c = default_curriculum();
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for section in &c.sections {
            for topic in &section.topics {
                let canonical = normalize(&topic.name);
                seen.insert(canonical.clone(), topic.name.clone());
                for a in &topic.aliases {
                    let key = normalize(a);
                    if let Some(prev) = seen.get(&key) {
                        assert_eq!(
                            prev, &topic.name,
                            "alias {a:?} collides between {prev:?} and {:?}",
                            topic.name
                        );
                    } else {
                        seen.insert(key, topic.name.clone());
                    }
                }
            }
        }
    }
}
