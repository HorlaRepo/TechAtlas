#![forbid(unsafe_code)]

use opentelemetry::{KeyValue, global, trace::TracerProvider};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
use prometheus_client::{
    encoding::{EncodeLabelSet, text::encode},
    metrics::{
        counter::Counter,
        family::Family,
        gauge::Gauge,
        histogram::{Histogram, exponential_buckets},
    },
    registry::Registry,
};
use std::{
    env,
    sync::{Arc, Mutex},
    time::Duration,
};
use thiserror::Error;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const METRIC_PREFIX: &str = "techatlas";

#[derive(Clone)]
pub struct TelemetryMetrics {
    registry: Arc<Mutex<Registry>>,
    service: String,
    http_requests: Family<HttpLabels, Counter>,
    http_request_duration: Family<HttpRouteLabels, Histogram>,
    dependency_ready: Family<DependencyLabels, Gauge>,
    operations: Family<OperationLabels, Counter>,
    operation_duration: Family<OperationName, Histogram>,
    values: Family<ValueName, Gauge>,
    in_flight_work: Gauge,
    last_successful_work_timestamp: Gauge,
}

#[derive(Clone, Debug, EncodeLabelSet, Hash, PartialEq, Eq)]
struct HttpLabels {
    method: String,
    status: String,
}

#[derive(Clone, Debug, EncodeLabelSet, Hash, PartialEq, Eq)]
struct HttpRouteLabels {
    method: String,
}

#[derive(Clone, Debug, EncodeLabelSet, Hash, PartialEq, Eq)]
struct DependencyLabels {
    dependency: String,
}

#[derive(Clone, Debug, EncodeLabelSet, Hash, PartialEq, Eq)]
struct OperationLabels {
    operation: String,
    outcome: String,
}

#[derive(Clone, Debug, EncodeLabelSet, Hash, PartialEq, Eq)]
struct OperationName {
    operation: String,
}

#[derive(Clone, Debug, EncodeLabelSet, Hash, PartialEq, Eq)]
struct ValueName {
    name: String,
}

#[derive(Clone, Debug, EncodeLabelSet, Hash, PartialEq, Eq)]
struct BuildInfoLabels {
    service: String,
    version: String,
}

impl TelemetryMetrics {
    pub fn new(service: impl Into<String>) -> Result<Self, TelemetryError> {
        let service = service.into();
        let mut registry = Registry::with_prefix(METRIC_PREFIX);
        let http_requests = Family::<HttpLabels, Counter>::default();
        let http_request_duration =
            Family::<HttpRouteLabels, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(0.005, 2.0, 12))
            });
        let dependency_ready = Family::<DependencyLabels, Gauge>::default();
        let operations = Family::<OperationLabels, Counter>::default();
        let operation_duration = Family::<OperationName, Histogram>::new_with_constructor(|| {
            Histogram::new(exponential_buckets(0.001, 2.0, 16))
        });
        let values = Family::<ValueName, Gauge>::default();
        let in_flight_work = Gauge::default();
        let last_successful_work_timestamp = Gauge::default();
        let build_info = Family::<BuildInfoLabels, Gauge>::default();
        let version = env::var("TECHATLAS_DEPLOYMENT_VERSION")
            .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned());

        registry.register(
            "http_requests",
            "HTTP requests completed",
            http_requests.clone(),
        );
        registry.register(
            "http_request_duration_seconds",
            "HTTP request duration in seconds",
            http_request_duration.clone(),
        );
        registry.register("value", "Bounded operational gauge values", values.clone());
        registry.register(
            "dependency_ready",
            "Dependency readiness where 1 is ready",
            dependency_ready.clone(),
        );
        registry.register(
            "operations_total",
            "Completed bounded operational actions",
            operations.clone(),
        );
        registry.register(
            "operation_duration_seconds",
            "Operational action duration in seconds",
            operation_duration.clone(),
        );
        registry.register(
            "in_flight_work",
            "Current in-flight worker jobs",
            in_flight_work.clone(),
        );
        registry.register(
            "last_successful_work_timestamp_seconds",
            "Unix timestamp of the last successful scheduler or worker action",
            last_successful_work_timestamp.clone(),
        );
        registry.register("build", "Build and service information", build_info.clone());
        build_info
            .get_or_create(&BuildInfoLabels {
                service: service.clone(),
                version,
            })
            .set(1);

        Ok(Self {
            registry: Arc::new(Mutex::new(registry)),
            service,
            http_requests,
            http_request_duration,
            dependency_ready,
            operations,
            operation_duration,
            values,
            in_flight_work,
            last_successful_work_timestamp,
        })
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    pub fn observe_http(&self, method: &str, status: u16, duration: Duration) {
        self.http_requests
            .get_or_create(&HttpLabels {
                method: method.to_owned(),
                status: status.to_string(),
            })
            .inc();
        self.http_request_duration
            .get_or_create(&HttpRouteLabels {
                method: method.to_owned(),
            })
            .observe(duration.as_secs_f64());
    }

    pub fn set_dependency_ready(&self, dependency: &str, ready: bool) {
        self.dependency_ready
            .get_or_create(&DependencyLabels {
                dependency: dependency.to_owned(),
            })
            .set(i64::from(ready));
    }

    pub fn record_operation(&self, operation: &str, outcome: &str, duration: Duration) {
        self.operations
            .get_or_create(&OperationLabels {
                operation: operation.to_owned(),
                outcome: outcome.to_owned(),
            })
            .inc();
        self.operation_duration
            .get_or_create(&OperationName {
                operation: operation.to_owned(),
            })
            .observe(duration.as_secs_f64());
    }

    pub fn set_in_flight_work(&self, value: i64) {
        self.in_flight_work.set(value);
    }

    pub fn set_value(&self, name: &str, value: i64) {
        self.values
            .get_or_create(&ValueName {
                name: name.to_owned(),
            })
            .set(value);
    }

    pub fn mark_successful_work(&self, unix_timestamp_seconds: i64) {
        self.last_successful_work_timestamp
            .set(unix_timestamp_seconds);
    }

    pub fn encode(&self) -> Result<String, TelemetryError> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| TelemetryError::MetricsUnavailable)?;
        let mut output = String::new();
        encode(&mut output, &registry).map_err(|_| TelemetryError::MetricsEncoding)?;
        Ok(output)
    }
}

pub fn init_tracing(service: &str, log_filter: &str) -> Result<(), TelemetryError> {
    let filter = EnvFilter::try_new(log_filter).map_err(|_| TelemetryError::InvalidLogFilter)?;
    let format = fmt::layer()
        .json()
        .with_target(false)
        .with_current_span(true);
    let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty());
    if let Some(endpoint) = endpoint {
        let exporter = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(|_| TelemetryError::InvalidOtlpEndpoint)?;
        let resource = Resource::builder()
            .with_service_name(service.to_owned())
            .with_attributes([KeyValue::new(
                "service.version",
                env::var("TECHATLAS_DEPLOYMENT_VERSION")
                    .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned()),
            )])
            .build();
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build();
        let tracer = provider.tracer(service.to_owned());
        global::set_tracer_provider(provider);
        tracing_subscriber::registry()
            .with(filter)
            .with(format)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .try_init()
            .map_err(|_| TelemetryError::AlreadyInitialised)?;
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(format)
            .try_init()
            .map_err(|_| TelemetryError::AlreadyInitialised)?;
    }
    tracing::info!(
        service,
        otlp_configured = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok(),
        "telemetry initialised"
    );
    Ok(())
}

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("RUST_LOG contains an invalid tracing filter")]
    InvalidLogFilter,
    #[error("tracing has already been initialised")]
    AlreadyInitialised,
    #[error("OTEL_EXPORTER_OTLP_ENDPOINT could not initialise an OTLP exporter")]
    InvalidOtlpEndpoint,
    #[error("telemetry metrics are unavailable")]
    MetricsUnavailable,
    #[error("telemetry metric encoding failed")]
    MetricsEncoding,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_encode_stable_low_cardinality_labels() {
        let metrics = TelemetryMetrics::new("test-service").expect("metrics should initialise");
        metrics.observe_http("GET", 200, Duration::from_millis(12));
        metrics.set_dependency_ready("postgres", true);
        metrics.record_operation("scheduler_tick", "success", Duration::from_millis(2));
        let encoded = metrics.encode().expect("metrics should encode");

        assert!(encoded.contains("techatlas_http_requests_total"));
        assert!(encoded.contains("method=\"GET\""));
        assert!(encoded.contains("dependency=\"postgres\""));
        assert!(encoded.contains("operation=\"scheduler_tick\""));
    }
}
