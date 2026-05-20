use refineforge_repair_api::{Diagnostic, Position, Range, Severity};
use serde_json::json;

fn diagnostic() -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 7,
            },
        },
        severity: Severity::Error,
        message: "unsolved goals".into(),
        source: Some("lean".into()),
    }
}

#[test]
fn local_finetune_manifest_command_returns_patch_and_usage() {
    let td = tempfile::tempdir().unwrap();
    let weights_dir = td.path().join("weights");
    std::fs::create_dir(&weights_dir).unwrap();

    let response_path = td.path().join("response.json");
    std::fs::write(
        &response_path,
        json!({
            "patch": {
                "start_line": 0,
                "start_char": 19,
                "end_line": 0,
                "end_char": 24,
                "new_text": "trivial",
                "rationale": "fixture local fine-tune patch"
            },
            "usage": {
                "input_tokens": 11,
                "output_tokens": 3
            },
            "stop_reason": "end_turn"
        })
        .to_string(),
    )
    .unwrap();

    #[cfg(windows)]
    let command = vec![
        "cmd".to_string(),
        "/C".to_string(),
        "type".to_string(),
        response_path.display().to_string(),
    ];
    #[cfg(not(windows))]
    let command = vec!["cat".to_string(), response_path.display().to_string()];

    std::fs::write(
        weights_dir.join("refineforge-local-finetune.json"),
        json!({
            "runtime": "command",
            "model_id": "fixture-qwen-proof-repair",
            "command": command
        })
        .to_string(),
    )
    .unwrap();

    let (strategy, usage) =
        refineforge_strategies::local_finetune_from_path_with_usage(&weights_dir).unwrap();
    assert_eq!(strategy.name(), "local-finetune");

    let patch = strategy
        .propose_patch(&diagnostic(), "theorem t : True := by sorry")
        .unwrap()
        .expect("fixture backend should propose a patch");

    assert_eq!(patch.range.start.line, 0);
    assert_eq!(patch.range.start.character, 19);
    assert_eq!(patch.new_text, "trivial");
    assert_eq!(patch.rationale, "fixture local fine-tune patch");

    let usage = usage.lock().unwrap().clone();
    assert_eq!(usage.calls, 1);
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 3);
    assert_eq!(usage.stop_reasons, vec![Some("end_turn".into())]);
}

#[test]
fn local_finetune_requires_runtime_manifest() {
    let td = tempfile::tempdir().unwrap();
    let err = match refineforge_strategies::local_finetune_from_path(td.path()) {
        Ok(_) => panic!("missing runtime manifest should fail"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("refineforge-local-finetune.json"),
        "unexpected error: {err}"
    );
}
