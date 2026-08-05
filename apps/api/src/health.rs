use crate::app::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use techatlas_common::DependencyStatus;

#[derive(Serialize)]
pub struct LivenessResponse {
    status: &'static str,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    status: &'static str,
    dependencies: DependencyReadiness,
}

#[derive(Serialize)]
pub struct DependencyReadiness {
    postgres: DependencyStatus,
    redis: DependencyStatus,
    meilisearch: DependencyStatus,
}

pub async fn healthz() -> Json<LivenessResponse> {
    Json(LivenessResponse { status: "ok" })
}

pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let (postgres, redis, meilisearch) = tokio::join!(
        state.postgres.check(),
        state.redis.check(),
        state.meilisearch.check(),
    );
    let dependencies = DependencyReadiness {
        postgres,
        redis,
        meilisearch,
    };
    state
        .metrics
        .set_dependency_ready("postgres", postgres.is_ready());
    state
        .metrics
        .set_dependency_ready("redis", redis.is_ready());
    state
        .metrics
        .set_dependency_ready("meilisearch", meilisearch.is_ready());
    let status = if postgres.is_ready() && redis.is_ready() && meilisearch.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let status_label = if status.is_success() {
        "ready"
    } else {
        "not_ready"
    };

    (
        status,
        Json(ReadinessResponse {
            status: status_label,
            dependencies,
        }),
    )
}
