use super::*;
use std::sync::Mutex;

struct RefreshProvider {
    identity: ModuleResolverIdentity,
    calls: Arc<Mutex<Vec<usize>>>,
    ordinal: usize,
    failure: bool,
}

impl ModuleResolutionProvider for RefreshProvider {
    fn identity(&self) -> &ModuleResolverIdentity {
        &self.identity
    }

    fn refresh(&self) -> Result<(), String> {
        self.calls
            .lock()
            .map_err(|error| error.to_string())?
            .push(self.ordinal);
        if self.failure {
            Err("fixture refresh failed".to_owned())
        } else {
            Ok(())
        }
    }

    fn resolve(&self, _request: &ModuleResolutionRequest) -> ModuleResolutionOutcome {
        ModuleResolutionOutcome::Unresolved("fixture".to_owned())
    }

    fn observed_inputs(&self) -> Result<Vec<String>, String> {
        if self.failure {
            Err("fixture inventory failed".to_owned())
        } else {
            Ok(vec!["shared".to_owned(), self.ordinal.to_string()])
        }
    }
}

#[test]
fn composite_refresh_preserves_order_and_stops_on_failure() -> Result<(), String> {
    for failure in [false, true] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let providers = (0..3)
            .map(|ordinal| {
                Arc::new(RefreshProvider {
                    identity: ModuleResolverIdentity {
                        namespace: format!("fixture/{ordinal}"),
                        version: "1".to_owned(),
                        settings_fingerprint: Vec::new(),
                    },
                    calls: Arc::clone(&calls),
                    ordinal,
                    failure: failure && ordinal == 1,
                }) as Arc<dyn ModuleResolutionProvider>
            })
            .collect();
        let composite = CompositeModuleResolutionProvider::new(providers);
        let result = composite.refresh();
        let expected_inputs = if failure {
            Err("fixture inventory failed".to_owned())
        } else {
            Ok(vec![
                "0".to_owned(),
                "1".to_owned(),
                "2".to_owned(),
                "shared".to_owned(),
            ])
        };
        if composite.observed_inputs() != expected_inputs {
            return Err("composite inventory union or error propagation differs".to_owned());
        }
        let expected_result = if failure {
            Err("fixture refresh failed".to_owned())
        } else {
            Ok(())
        };
        let expected_calls = if failure { vec![0, 1] } else { vec![0, 1, 2] };
        if result != expected_result
            || *calls.lock().map_err(|error| error.to_string())? != expected_calls
        {
            return Err("composite refresh order or failure propagation differs".to_owned());
        }
    }
    Ok(())
}
