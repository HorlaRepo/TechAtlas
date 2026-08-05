use secrecy::SecretString;
use std::time::{Duration, Instant};
use techatlas_crawler::{PolitenessGate, PolitenessRequest};
use techatlas_queue::{RedisPolitenessConfig, RedisPolitenessGate};
use testcontainers_modules::testcontainers::{
    GenericImage,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn shared_redis_lease_serializes_domain_politeness_across_gate_instances()
-> Result<(), Box<dyn std::error::Error>> {
    let container = GenericImage::new("redis", "7-alpine")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await?;
    let port = container.get_host_port_ipv4(6379).await?;
    let redis_url = SecretString::from(format!("redis://127.0.0.1:{port}"));
    let config = RedisPolitenessConfig::new("test:politeness:v1", Duration::from_secs(2))?;
    let first_gate = RedisPolitenessGate::new(&redis_url, config.clone())?;
    let second_gate = RedisPolitenessGate::new(&redis_url, config)?;
    let request = PolitenessRequest {
        domain: "fixture.example".to_owned(),
        minimum_delay: Duration::from_millis(30),
    };

    first_gate
        .acquire(&request, &CancellationToken::new())
        .await?;
    let started = Instant::now();
    second_gate
        .acquire(&request, &CancellationToken::new())
        .await?;

    assert!(started.elapsed() >= Duration::from_millis(20));
    Ok(())
}
