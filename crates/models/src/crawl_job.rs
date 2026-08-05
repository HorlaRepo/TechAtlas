use crate::DomainId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use uuid::Uuid;

pub const CRAWL_JOB_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CrawlJobId(Uuid);

impl CrawlJobId {
    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CorrelationId(Uuid);

impl CorrelationId {
    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn parse(input: &str) -> Result<Self, CrawlJobError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CrawlJobError::EmptyIdempotencyKey);
        }

        Ok(Self(input.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrawlJobOwner {
    Scheduler,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlJobV1 {
    job_id: CrawlJobId,
    domain_id: DomainId,
    correlation_id: CorrelationId,
    idempotency_key: IdempotencyKey,
}

impl CrawlJobV1 {
    pub fn new(
        job_id: CrawlJobId,
        domain_id: DomainId,
        correlation_id: CorrelationId,
        idempotency_key: IdempotencyKey,
    ) -> Self {
        Self {
            job_id,
            domain_id,
            correlation_id,
            idempotency_key,
        }
    }

    pub fn schema_version(&self) -> u16 {
        CRAWL_JOB_SCHEMA_VERSION
    }

    pub fn owner(&self) -> CrawlJobOwner {
        CrawlJobOwner::Scheduler
    }

    pub fn job_id(&self) -> &CrawlJobId {
        &self.job_id
    }

    pub fn domain_id(&self) -> &DomainId {
        &self.domain_id
    }

    pub fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }

    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CrawlJobWire {
    schema_version: u16,
    owner: CrawlJobOwner,
    job_id: Uuid,
    domain_id: Uuid,
    correlation_id: Uuid,
    idempotency_key: String,
}

impl Serialize for CrawlJobV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CrawlJobWire {
            schema_version: self.schema_version(),
            owner: self.owner(),
            job_id: self.job_id.as_uuid(),
            domain_id: self.domain_id.as_uuid(),
            correlation_id: self.correlation_id.as_uuid(),
            idempotency_key: self.idempotency_key.as_str().to_owned(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CrawlJobV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CrawlJobWire::deserialize(deserializer)?;
        if wire.schema_version != CRAWL_JOB_SCHEMA_VERSION {
            return Err(serde::de::Error::custom(
                CrawlJobError::UnsupportedSchemaVersion {
                    found: wire.schema_version,
                },
            ));
        }
        let idempotency_key =
            IdempotencyKey::parse(&wire.idempotency_key).map_err(serde::de::Error::custom)?;
        Ok(Self::new(
            CrawlJobId::from_uuid(wire.job_id),
            DomainId::from_uuid(wire.domain_id),
            CorrelationId::from_uuid(wire.correlation_id),
            idempotency_key,
        ))
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CrawlJobError {
    #[error("crawl job idempotency key must not be empty")]
    EmptyIdempotencyKey,
    #[error("unsupported crawl job schema version: {found}")]
    UnsupportedSchemaVersion { found: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> CrawlJobV1 {
        CrawlJobV1::new(
            CrawlJobId::from_uuid(Uuid::from_u128(1)),
            DomainId::from_uuid(Uuid::from_u128(2)),
            CorrelationId::from_uuid(Uuid::from_u128(3)),
            IdempotencyKey::parse("crawl:1").expect("key should be valid"),
        )
    }

    #[test]
    fn serializes_a_versioned_scheduler_owned_job() {
        let value = serde_json::to_value(job()).expect("job should serialize");

        assert_eq!(value["schema_version"], CRAWL_JOB_SCHEMA_VERSION);
        assert_eq!(value["owner"], "scheduler");
        assert_eq!(value["idempotency_key"], "crawl:1");
    }

    #[test]
    fn deserialization_rejects_unknown_versions_and_empty_keys() {
        let unsupported = r#"{
            "schema_version": 2,
            "owner": "scheduler",
            "job_id": "00000000-0000-0000-0000-000000000001",
            "domain_id": "00000000-0000-0000-0000-000000000002",
            "correlation_id": "00000000-0000-0000-0000-000000000003",
            "idempotency_key": "crawl:1"
        }"#;
        assert!(serde_json::from_str::<CrawlJobV1>(unsupported).is_err());

        assert_eq!(
            IdempotencyKey::parse("  "),
            Err(CrawlJobError::EmptyIdempotencyKey)
        );
    }
}
