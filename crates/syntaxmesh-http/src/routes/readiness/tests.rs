use super::{EngineWorkflowDiagnostics, GenerationId, StatusCode, report};

#[test]
fn pending_recovery_blocks_readiness_but_terminal_rejections_do_not() {
    for (prepared_operations, rejected_operations, expected) in [
        (0, 0, StatusCode::OK),
        (1, 0, StatusCode::SERVICE_UNAVAILABLE),
        (0, 2, StatusCode::OK),
        (1, 2, StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let response = report(
            GenerationId::derive(&[b"ready-policy"]),
            EngineWorkflowDiagnostics {
                prepared_operations,
                completed_operations: 3,
                rejected_operations,
            },
        );
        assert_eq!(response.status(), expected);
    }
}
