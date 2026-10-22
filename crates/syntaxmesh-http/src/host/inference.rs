use super::*;
use syntaxmesh_api_model::{
    INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpointRequest, InferenceCheckpointResponse,
    InferencePageRequest, InferencePageResponse,
};
use syntaxmesh_inference_host::{
    InferenceReadBindings, inference_checkpoint_record, inference_page_response,
};

impl<E: LanguageExtractor + Send + 'static> SyntaxMeshHttp<E> {
    /// Install operator-configured retained inference reads without opening storage.
    ///
    /// # Errors
    /// Rejects registry scope different from this host's repository/worktree.
    pub fn with_inference(mut self, bindings: InferenceReadBindings) -> Result<Self, String> {
        if bindings.scope() != (self.repository, self.worktree) {
            return Err("inference registry outside HTTP host scope".to_owned());
        }
        self.inference = Some(Arc::new(bindings));
        Ok(self)
    }

    pub(crate) async fn inference_checkpoint(
        &self,
        request: InferenceCheckpointRequest,
    ) -> Result<InferenceCheckpointResponse, StatusCode> {
        let bindings = self
            .inference
            .as_ref()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        bindings
            .checkpoint(&request)
            .map_err(|_error| StatusCode::BAD_REQUEST)?;
        let bindings = Arc::clone(bindings);
        self.read_engine(move |_host, engine| {
            let (database, origin, generation) = bindings
                .checkpoint(&request)
                .map_err(|_error| StatusCode::BAD_REQUEST)?;
            let checkpoint = engine
                .inference_checkpoint(database, origin, generation)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?
                .map(inference_checkpoint_record)
                .transpose()
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(InferenceCheckpointResponse {
                schema_version: INFERENCE_READ_SCHEMA_VERSION,
                binding: request.binding,
                generation,
                checkpoint,
            })
        })
        .await
    }

    pub(crate) async fn inference_page(
        &self,
        request: InferencePageRequest,
    ) -> Result<InferencePageResponse, StatusCode> {
        let bindings = self
            .inference
            .as_ref()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        bindings
            .page(&request)
            .map_err(|_error| StatusCode::BAD_REQUEST)?;
        let bindings = Arc::clone(bindings);
        self.read_engine(move |_host, engine| {
            let admitted = bindings
                .page(&request)
                .map_err(|_error| StatusCode::BAD_REQUEST)?;
            let retained = engine
                .inference_checkpoint(
                    admitted.database,
                    admitted.request.origin,
                    request.generation,
                )
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            if retained.is_none() {
                return Err(StatusCode::NOT_FOUND);
            }
            let page = engine
                .inference_claims_page(admitted.database, &admitted.request)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)?;
            inference_page_response(request.binding, page)
                .map_err(|_error| StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await
    }
}
