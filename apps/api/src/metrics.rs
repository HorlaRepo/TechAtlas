use crate::app::AppState;
use axum::{
    body::Body,
    extract::State,
    http::{Request, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::time::Instant;

pub async fn metrics(State(state): State<AppState>) -> Response {
    let (postgres, redis, meilisearch) = tokio::join!(
        state.postgres.check(),
        state.redis.check(),
        state.meilisearch.check(),
    );
    state
        .metrics
        .set_dependency_ready("postgres", postgres.is_ready());
    state
        .metrics
        .set_dependency_ready("redis", redis.is_ready());
    state
        .metrics
        .set_dependency_ready("meilisearch", meilisearch.is_ready());
    match state.metrics.encode() {
        Ok(body) => (
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        Err(_) => axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub async fn observe_http(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    state
        .metrics
        .observe_http(&method, response.status().as_u16(), started.elapsed());
    response
}
