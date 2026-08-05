#![forbid(unsafe_code)]

use async_trait::async_trait;
use redis::{
    AsyncCommands, FromRedisValue,
    streams::{StreamAutoClaimOptions, StreamId, StreamReadOptions},
};
use secrecy::{ExposeSecret, SecretString};
use std::{future::Future, time::Duration};
use techatlas_common::{DependencyProbe, DependencyStatus};
use techatlas_crawler::{PolitenessGate, PolitenessGateError, PolitenessRequest};
use techatlas_models::{AdminOperationError, AdminQueueRead, AdminQueueSnapshot, CrawlJobV1};
use thiserror::Error;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use tracing::warn;

const PAYLOAD_FIELD: &str = "payload";
const DEAD_LETTER_SCRIPT: &str = r#"
if redis.call('HEXISTS', KEYS[3], ARGV[1]) == 0 then
    redis.call(
        'XADD', KEYS[2], '*',
        'source_stream', KEYS[1],
        'source_entry_id', ARGV[1],
        'payload', ARGV[2],
        'delivery_count', ARGV[3],
        'reason', ARGV[4]
    )
    redis.call('HSET', KEYS[3], ARGV[1], '1')
end
return redis.call('XACK', KEYS[1], ARGV[5], ARGV[1])
"#;
const POLITENESS_LEASE_SCRIPT: &str = r#"
local time = redis.call('TIME')
local now_millis = (tonumber(time[1]) * 1000) + math.floor(tonumber(time[2]) / 1000)
local delay_millis = tonumber(ARGV[1])
local next_allowed = tonumber(redis.call('GET', KEYS[1]) or '0')
if next_allowed > now_millis then
    return next_allowed - now_millis
end
redis.call('SET', KEYS[1], now_millis + delay_millis, 'PX', delay_millis)
return 0
"#;

pub struct RedisProbe {
    client: redis::Client,
    timeout: Duration,
}

impl RedisProbe {
    pub fn new(redis_url: &SecretString, timeout: Duration) -> Result<Self, RedisProbeError> {
        let client = redis::Client::open(redis_url.expose_secret())
            .map_err(|_| RedisProbeError::InvalidConnectionConfiguration)?;

        Ok(Self { client, timeout })
    }
}

#[async_trait]
impl DependencyProbe for RedisProbe {
    async fn check(&self) -> DependencyStatus {
        let Ok(Ok(mut connection)) =
            timeout(self.timeout, self.client.get_multiplexed_async_connection()).await
        else {
            return DependencyStatus::Unavailable;
        };

        match timeout(self.timeout, connection.ping::<String>()).await {
            Ok(Ok(response)) if response == "PONG" => DependencyStatus::Ready,
            _ => DependencyStatus::Unavailable,
        }
    }
}

#[derive(Debug, Error)]
pub enum RedisProbeError {
    #[error("invalid Redis connection configuration")]
    InvalidConnectionConfiguration,
}

/// Configuration for the distributed, per-domain crawl politeness lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedisPolitenessConfig {
    key_prefix: String,
    operation_timeout: Duration,
}

impl Default for RedisPolitenessConfig {
    fn default() -> Self {
        Self {
            key_prefix: "techatlas:crawl-politeness:v1".to_owned(),
            operation_timeout: Duration::from_secs(5),
        }
    }
}

impl RedisPolitenessConfig {
    pub fn new(
        key_prefix: &str,
        operation_timeout: Duration,
    ) -> Result<Self, RedisPolitenessConfigError> {
        let key_prefix = key_prefix.trim();
        if key_prefix.is_empty() {
            return Err(RedisPolitenessConfigError::EmptyKeyPrefix);
        }
        if operation_timeout.is_zero() {
            return Err(RedisPolitenessConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            key_prefix: key_prefix.to_owned(),
            operation_timeout,
        })
    }

    fn lease_key(&self, domain: &str) -> String {
        format!("{}:{domain}", self.key_prefix)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RedisPolitenessConfigError {
    #[error("Redis politeness key prefix must not be empty")]
    EmptyKeyPrefix,
    #[error("Redis politeness operation timeout must be greater than zero")]
    ZeroOperationTimeout,
}

/// Redis-server-time lease that coordinates crawl delays across worker replicas.
pub struct RedisPolitenessGate {
    client: redis::Client,
    config: RedisPolitenessConfig,
}

impl RedisPolitenessGate {
    pub fn new(
        redis_url: &SecretString,
        config: RedisPolitenessConfig,
    ) -> Result<Self, RedisPolitenessGateError> {
        let client = redis::Client::open(redis_url.expose_secret())
            .map_err(|_| RedisPolitenessGateError::InvalidConnectionConfiguration)?;
        Ok(Self { client, config })
    }

    async fn lease_wait(
        &self,
        domain: &str,
        delay: Duration,
    ) -> Result<Duration, RedisPolitenessGateError> {
        let delay_millis =
            u64::try_from(delay.as_millis()).map_err(|_| RedisPolitenessGateError::Unavailable)?;
        if delay_millis == 0 {
            return Err(RedisPolitenessGateError::Unavailable);
        }
        let mut connection = timeout(
            self.config.operation_timeout,
            self.client.get_multiplexed_async_connection(),
        )
        .await
        .map_err(|_| RedisPolitenessGateError::TimedOut)?
        .map_err(|_| RedisPolitenessGateError::Unavailable)?;
        let wait_millis: i64 = timeout(
            self.config.operation_timeout,
            redis::cmd("EVAL")
                .arg(POLITENESS_LEASE_SCRIPT)
                .arg(1)
                .arg(self.config.lease_key(domain))
                .arg(delay_millis)
                .query_async(&mut connection),
        )
        .await
        .map_err(|_| RedisPolitenessGateError::TimedOut)?
        .map_err(|_| RedisPolitenessGateError::Unavailable)?;
        let wait_millis =
            u64::try_from(wait_millis).map_err(|_| RedisPolitenessGateError::Unavailable)?;
        Ok(Duration::from_millis(wait_millis))
    }
}

#[async_trait]
impl PolitenessGate for RedisPolitenessGate {
    async fn acquire(
        &self,
        request: &PolitenessRequest,
        cancellation: &CancellationToken,
    ) -> Result<(), PolitenessGateError> {
        if request.domain.trim().is_empty() {
            return Err(PolitenessGateError::Unavailable {
                reason: "empty crawl domain".to_owned(),
            });
        }

        loop {
            if cancellation.is_cancelled() {
                return Err(PolitenessGateError::Cancelled);
            }
            let wait = self
                .lease_wait(&request.domain, request.minimum_delay)
                .await
                .map_err(|error| PolitenessGateError::Unavailable {
                    reason: error.to_string(),
                })?;
            if wait.is_zero() {
                return Ok(());
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Err(PolitenessGateError::Cancelled),
                _ = sleep(wait) => {}
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum RedisPolitenessGateError {
    #[error("invalid Redis connection configuration")]
    InvalidConnectionConfiguration,
    #[error("Redis politeness operation timed out")]
    TimedOut,
    #[error("Redis politeness gate is unavailable")]
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedisStreamsConfig {
    stream_key: String,
    consumer_group: String,
    dead_letter_stream_key: String,
    dead_letter_index_key: String,
    batch_size: usize,
    read_block: Duration,
    operation_timeout: Duration,
    claim_idle: Duration,
    max_deliveries: usize,
}

impl Default for RedisStreamsConfig {
    fn default() -> Self {
        Self {
            stream_key: "techatlas:crawl-jobs:v1".to_owned(),
            consumer_group: "techatlas:crawl-workers:v1".to_owned(),
            dead_letter_stream_key: "techatlas:crawl-jobs:dead-letter:v1".to_owned(),
            dead_letter_index_key: "techatlas:crawl-jobs:dead-letter-index:v1".to_owned(),
            batch_size: 10,
            read_block: Duration::from_secs(1),
            operation_timeout: Duration::from_secs(5),
            claim_idle: Duration::from_secs(60),
            max_deliveries: 5,
        }
    }
}

impl RedisStreamsConfig {
    pub fn with_stream_names(
        mut self,
        stream_key: &str,
        consumer_group: &str,
        dead_letter_stream_key: &str,
        dead_letter_index_key: &str,
    ) -> Result<Self, RedisStreamsConfigError> {
        self.stream_key = required_name("stream key", stream_key)?;
        self.consumer_group = required_name("consumer group", consumer_group)?;
        self.dead_letter_stream_key =
            required_name("dead-letter stream key", dead_letter_stream_key)?;
        self.dead_letter_index_key = required_name("dead-letter index key", dead_letter_index_key)?;
        Ok(self)
    }

    pub fn with_read_options(
        mut self,
        batch_size: usize,
        read_block: Duration,
        operation_timeout: Duration,
    ) -> Result<Self, RedisStreamsConfigError> {
        if batch_size == 0 {
            return Err(RedisStreamsConfigError::ZeroBatchSize);
        }
        if operation_timeout.is_zero() {
            return Err(RedisStreamsConfigError::ZeroOperationTimeout);
        }
        if operation_timeout < read_block {
            return Err(RedisStreamsConfigError::OperationTimeoutBeforeReadBlock);
        }

        self.batch_size = batch_size;
        self.read_block = read_block;
        self.operation_timeout = operation_timeout;
        Ok(self)
    }

    pub fn with_recovery_policy(
        mut self,
        claim_idle: Duration,
        max_deliveries: usize,
    ) -> Result<Self, RedisStreamsConfigError> {
        if max_deliveries == 0 {
            return Err(RedisStreamsConfigError::ZeroMaxDeliveries);
        }

        self.claim_idle = claim_idle;
        self.max_deliveries = max_deliveries;
        Ok(self)
    }

    pub fn stream_key(&self) -> &str {
        &self.stream_key
    }

    pub fn consumer_group(&self) -> &str {
        &self.consumer_group
    }

    pub fn dead_letter_stream_key(&self) -> &str {
        &self.dead_letter_stream_key
    }

    fn dead_letter_index_key(&self) -> &str {
        &self.dead_letter_index_key
    }

    fn batch_size(&self) -> usize {
        self.batch_size
    }

    fn read_block_millis(&self) -> Result<usize, RedisStreamsConfigError> {
        duration_millis(self.read_block)
    }

    fn operation_timeout(&self) -> Duration {
        self.operation_timeout
    }

    fn claim_idle_millis(&self) -> Result<usize, RedisStreamsConfigError> {
        duration_millis(self.claim_idle)
    }

    fn max_deliveries(&self) -> usize {
        self.max_deliveries
    }
}

fn required_name(setting: &'static str, value: &str) -> Result<String, RedisStreamsConfigError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(RedisStreamsConfigError::EmptyName { setting });
    }

    Ok(value.to_owned())
}

fn duration_millis(value: Duration) -> Result<usize, RedisStreamsConfigError> {
    usize::try_from(value.as_millis()).map_err(|_| RedisStreamsConfigError::DurationTooLarge)
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RedisStreamsConfigError {
    #[error("Redis Streams {setting} must not be empty")]
    EmptyName { setting: &'static str },
    #[error("Redis Streams batch size must be greater than zero")]
    ZeroBatchSize,
    #[error("Redis Streams operation timeout must be greater than zero")]
    ZeroOperationTimeout,
    #[error("Redis Streams operation timeout must not be shorter than the read block duration")]
    OperationTimeoutBeforeReadBlock,
    #[error("Redis Streams maximum delivery count must be greater than zero")]
    ZeroMaxDeliveries,
    #[error("Redis Streams duration is too large")]
    DurationTooLarge,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("crawl-job consumer name must not be empty")]
pub struct CrawlJobConsumerNameError;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CrawlJobConsumerName(String);

impl CrawlJobConsumerName {
    pub fn parse(input: &str) -> Result<Self, CrawlJobConsumerNameError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CrawlJobConsumerNameError);
        }

        Ok(Self(input.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CrawlJobReceipt(String);

impl CrawlJobReceipt {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedCrawlJob {
    receipt: CrawlJobReceipt,
    job: CrawlJobV1,
    delivery_count: usize,
}

impl ReceivedCrawlJob {
    pub fn receipt(&self) -> &CrawlJobReceipt {
        &self.receipt
    }

    pub fn job(&self) -> &CrawlJobV1 {
        &self.job
    }

    pub fn delivery_count(&self) -> usize {
        self.delivery_count
    }
}

#[async_trait]
pub trait CrawlJobPublisher: Send + Sync {
    async fn publish(&self, job: &CrawlJobV1) -> Result<CrawlJobReceipt, CrawlJobQueueError>;
}

#[async_trait]
pub trait CrawlJobConsumer: Send + Sync {
    async fn receive(
        &self,
        consumer: &CrawlJobConsumerName,
    ) -> Result<Vec<ReceivedCrawlJob>, CrawlJobQueueError>;

    async fn reclaim_stale(
        &self,
        consumer: &CrawlJobConsumerName,
    ) -> Result<Vec<ReceivedCrawlJob>, CrawlJobQueueError>;

    async fn acknowledge(&self, receipt: &CrawlJobReceipt) -> Result<(), CrawlJobQueueError>;
}

pub struct RedisStreamsCrawlJobQueue {
    client: redis::Client,
    config: RedisStreamsConfig,
}

impl RedisStreamsCrawlJobQueue {
    pub fn new(
        redis_url: &SecretString,
        config: RedisStreamsConfig,
    ) -> Result<Self, CrawlJobQueueError> {
        let client = redis::Client::open(redis_url.expose_secret())
            .map_err(|_| CrawlJobQueueError::InvalidConnectionConfiguration)?;

        Ok(Self { client, config })
    }

    pub fn config(&self) -> &RedisStreamsConfig {
        &self.config
    }

    async fn connection(&self) -> Result<redis::aio::MultiplexedConnection, CrawlJobQueueError> {
        timeout(
            self.config.operation_timeout(),
            self.client.get_multiplexed_async_connection(),
        )
        .await
        .map_err(|_| CrawlJobQueueError::TimedOut)?
        .map_err(CrawlJobQueueError::Unavailable)
    }

    async fn execute<T, F>(&self, command: F) -> Result<T, CrawlJobQueueError>
    where
        F: Future<Output = redis::RedisResult<T>>,
    {
        timeout(self.config.operation_timeout(), command)
            .await
            .map_err(|_| CrawlJobQueueError::TimedOut)?
            .map_err(CrawlJobQueueError::Unavailable)
    }

    async fn ensure_group(&self) -> Result<(), CrawlJobQueueError> {
        let mut connection = self.connection().await?;
        let result: Result<String, CrawlJobQueueError> = self
            .execute(
                redis::cmd("XGROUP")
                    .arg("CREATE")
                    .arg(self.config.stream_key())
                    .arg(self.config.consumer_group())
                    .arg("0-0")
                    .arg("MKSTREAM")
                    .query_async(&mut connection),
            )
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(CrawlJobQueueError::Unavailable(error)) if error.code() == Some("BUSYGROUP") => {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    async fn decode_entry(
        &self,
        entry: StreamId,
        delivery_count: usize,
    ) -> Result<Option<ReceivedCrawlJob>, CrawlJobQueueError> {
        let payload = entry
            .map
            .get(PAYLOAD_FIELD)
            .and_then(|value| String::from_redis_value(value).ok());

        let Some(payload) = payload else {
            self.dead_letter(
                &entry.id,
                "",
                delivery_count,
                DeadLetterReason::InvalidPayload,
            )
            .await?;
            warn!(source_entry_id = %entry.id, reason = "invalid_payload", "dead-lettered invalid crawl job message");
            return Ok(None);
        };

        let job = match serde_json::from_str::<CrawlJobV1>(&payload) {
            Ok(job) => job,
            Err(_) => {
                self.dead_letter(
                    &entry.id,
                    &payload,
                    delivery_count,
                    DeadLetterReason::InvalidPayload,
                )
                .await?;
                warn!(source_entry_id = %entry.id, reason = "invalid_payload", "dead-lettered invalid crawl job message");
                return Ok(None);
            }
        };

        Ok(Some(ReceivedCrawlJob {
            receipt: CrawlJobReceipt(entry.id),
            job,
            delivery_count,
        }))
    }

    async fn delivery_count(&self, entry_id: &str) -> Result<usize, CrawlJobQueueError> {
        let mut connection = self.connection().await?;
        let pending: redis::streams::StreamPendingCountReply = self
            .execute(connection.xpending_count(
                self.config.stream_key(),
                self.config.consumer_group(),
                entry_id,
                entry_id,
                1,
            ))
            .await?;
        pending
            .ids
            .first()
            .map(|entry| entry.times_delivered)
            .ok_or_else(|| CrawlJobQueueError::MissingPendingEntry {
                entry_id: entry_id.to_owned(),
            })
    }

    fn entry_payload(entry: &StreamId) -> String {
        entry
            .map
            .get(PAYLOAD_FIELD)
            .and_then(|value| String::from_redis_value(value).ok())
            .unwrap_or_default()
    }

    async fn dead_letter(
        &self,
        entry_id: &str,
        payload: &str,
        delivery_count: usize,
        reason: DeadLetterReason,
    ) -> Result<(), CrawlJobQueueError> {
        let mut connection = self.connection().await?;
        let _: i64 = self
            .execute(
                redis::cmd("EVAL")
                    .arg(DEAD_LETTER_SCRIPT)
                    .arg(3)
                    .arg(self.config.stream_key())
                    .arg(self.config.dead_letter_stream_key())
                    .arg(self.config.dead_letter_index_key())
                    .arg(entry_id)
                    .arg(payload)
                    .arg(delivery_count)
                    .arg(reason.as_str())
                    .arg(self.config.consumer_group())
                    .query_async(&mut connection),
            )
            .await?;

        Ok(())
    }
}

#[async_trait]
impl CrawlJobPublisher for RedisStreamsCrawlJobQueue {
    async fn publish(&self, job: &CrawlJobV1) -> Result<CrawlJobReceipt, CrawlJobQueueError> {
        let payload = serde_json::to_string(job).map_err(CrawlJobQueueError::Serialization)?;
        let mut connection = self.connection().await?;
        let entry_id: Option<String> = self
            .execute(connection.xadd(
                self.config.stream_key(),
                "*",
                &[(PAYLOAD_FIELD, payload.as_str())],
            ))
            .await?;
        let entry_id = entry_id.ok_or(CrawlJobQueueError::MissingPublishedEntryId)?;

        Ok(CrawlJobReceipt(entry_id))
    }
}

#[async_trait]
impl AdminQueueRead for RedisStreamsCrawlJobQueue {
    async fn snapshot(&self) -> Result<AdminQueueSnapshot, AdminOperationError> {
        self.ensure_group()
            .await
            .map_err(|_| AdminOperationError::Unavailable)?;
        let mut connection = self
            .connection()
            .await
            .map_err(|_| AdminOperationError::Unavailable)?;
        let groups: redis::streams::StreamInfoGroupsReply = self
            .execute(connection.xinfo_groups(self.config.stream_key()))
            .await
            .map_err(|_| AdminOperationError::Unavailable)?;
        let group = groups
            .groups
            .into_iter()
            .find(|group| group.name == self.config.consumer_group())
            .ok_or(AdminOperationError::Unavailable)?;
        Ok(AdminQueueSnapshot {
            ready_count: u64::try_from(group.lag.unwrap_or(0))
                .map_err(|_| AdminOperationError::Unavailable)?,
            processing_count: u64::try_from(group.pending)
                .map_err(|_| AdminOperationError::Unavailable)?,
        })
    }
}

#[async_trait]
impl CrawlJobConsumer for RedisStreamsCrawlJobQueue {
    async fn receive(
        &self,
        consumer: &CrawlJobConsumerName,
    ) -> Result<Vec<ReceivedCrawlJob>, CrawlJobQueueError> {
        self.ensure_group().await?;
        let options = StreamReadOptions::default()
            .group(self.config.consumer_group(), consumer.as_str())
            .block(self.config.read_block_millis()?)
            .count(self.config.batch_size());
        let mut connection = self.connection().await?;
        let reply: Option<redis::streams::StreamReadReply> = self
            .execute(connection.xread_options(&[self.config.stream_key()], &[">"], &options))
            .await?;

        let mut jobs = Vec::new();
        for entry in reply
            .into_iter()
            .flat_map(|reply| reply.keys)
            .flat_map(|key| key.ids)
        {
            if let Some(job) = self.decode_entry(entry, 1).await? {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    async fn reclaim_stale(
        &self,
        consumer: &CrawlJobConsumerName,
    ) -> Result<Vec<ReceivedCrawlJob>, CrawlJobQueueError> {
        self.ensure_group().await?;
        let options = StreamAutoClaimOptions::default().count(self.config.batch_size());
        let mut connection = self.connection().await?;
        let reply: redis::streams::StreamAutoClaimReply = self
            .execute(connection.xautoclaim_options(
                self.config.stream_key(),
                self.config.consumer_group(),
                consumer.as_str(),
                self.config.claim_idle_millis()?,
                "0-0",
                options,
            ))
            .await?;

        let mut jobs = Vec::new();
        for entry in reply.claimed {
            let delivery_count = self.delivery_count(&entry.id).await?;
            if delivery_count >= self.config.max_deliveries() {
                let payload = Self::entry_payload(&entry);
                self.dead_letter(
                    &entry.id,
                    &payload,
                    delivery_count,
                    DeadLetterReason::DeliveryLimitExceeded,
                )
                .await?;
                warn!(
                    source_entry_id = %entry.id,
                    delivery_count,
                    reason = "delivery_limit_exceeded",
                    "dead-lettered exhausted crawl job message"
                );
                continue;
            }

            if let Some(job) = self.decode_entry(entry, delivery_count).await? {
                warn!(
                    source_entry_id = %job.receipt().as_str(),
                    job_id = %job.job().job_id().as_uuid(),
                    domain_id = %job.job().domain_id().as_uuid(),
                    correlation_id = %job.job().correlation_id().as_uuid(),
                    delivery_count,
                    "reclaimed stale crawl job message"
                );
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    async fn acknowledge(&self, receipt: &CrawlJobReceipt) -> Result<(), CrawlJobQueueError> {
        let mut connection = self.connection().await?;
        let acknowledged: usize = self
            .execute(connection.xack(
                self.config.stream_key(),
                self.config.consumer_group(),
                &[receipt.as_str()],
            ))
            .await?;
        if acknowledged == 1 {
            let deleted: usize = self
                .execute(connection.xdel(self.config.stream_key(), &[receipt.as_str()]))
                .await?;
            if deleted == 1 {
                Ok(())
            } else {
                Err(CrawlJobQueueError::AcknowledgementMissing {
                    entry_id: receipt.as_str().to_owned(),
                })
            }
        } else {
            Err(CrawlJobQueueError::AcknowledgementMissing {
                entry_id: receipt.as_str().to_owned(),
            })
        }
    }
}

#[derive(Clone, Copy)]
enum DeadLetterReason {
    DeliveryLimitExceeded,
    InvalidPayload,
}

impl DeadLetterReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::DeliveryLimitExceeded => "delivery_limit_exceeded",
            Self::InvalidPayload => "invalid_payload",
        }
    }
}

#[derive(Debug, Error)]
pub enum CrawlJobQueueError {
    #[error("invalid Redis connection configuration")]
    InvalidConnectionConfiguration,
    #[error("Redis Streams operation timed out")]
    TimedOut,
    #[error("Redis Streams is unavailable")]
    Unavailable(#[source] redis::RedisError),
    #[error("crawl job serialization failed")]
    Serialization(#[source] serde_json::Error),
    #[error("Redis Streams did not return a published entry identifier")]
    MissingPublishedEntryId,
    #[error("Redis Streams pending entry was not found: {entry_id}")]
    MissingPendingEntry { entry_id: String },
    #[error("Redis Streams entry was not pending when acknowledged: {entry_id}")]
    AcknowledgementMissing { entry_id: String },
    #[error(transparent)]
    InvalidConfiguration(#[from] RedisStreamsConfigError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_queue_configuration() {
        assert_eq!(
            RedisStreamsConfig::default().with_recovery_policy(Duration::ZERO, 0),
            Err(RedisStreamsConfigError::ZeroMaxDeliveries)
        );
        assert_eq!(
            RedisStreamsConfig::default().with_read_options(
                10,
                Duration::from_secs(2),
                Duration::from_secs(1),
            ),
            Err(RedisStreamsConfigError::OperationTimeoutBeforeReadBlock)
        );
    }

    #[test]
    fn rejects_blank_consumer_names() {
        assert_eq!(
            CrawlJobConsumerName::parse(" \t "),
            Err(CrawlJobConsumerNameError)
        );
    }
}
