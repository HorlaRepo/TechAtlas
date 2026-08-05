use thiserror::Error;

const MAX_SLUG_LENGTH: usize = 64;
const MAX_EVIDENCE_KEY_LENGTH: usize = 128;
const MAX_EVIDENCE_VALUE_LENGTH: usize = 2_048;

/// A stable, lowercase identifier for a normalized technology catalogue entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TechnologySlug(String);

impl TechnologySlug {
    pub fn parse(value: &str) -> Result<Self, DetectionError> {
        valid_slug(value)
            .map(Self)
            .ok_or(DetectionError::InvalidTechnologySlug)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A stable, lowercase identifier for a normalized technology category.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TechnologyCategorySlug(String);

impl TechnologyCategorySlug {
    pub fn parse(value: &str) -> Result<Self, DetectionError> {
        valid_slug(value)
            .map(Self)
            .ok_or(DetectionError::InvalidTechnologyCategorySlug)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A stable, lowercase identifier for a deterministic detection rule.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuleSlug(String);

impl RuleSlug {
    pub fn parse(value: &str) -> Result<Self, DetectionError> {
        valid_slug(value)
            .map(Self)
            .ok_or(DetectionError::InvalidRuleSlug)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The immutable publication sequence of a detection rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuleVersion(u16);

impl RuleVersion {
    pub fn new(value: u16) -> Result<Self, DetectionError> {
        if value == 0 {
            return Err(DetectionError::InvalidRuleVersion);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

/// A deterministic confidence percentage in the inclusive range 0 through 100.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DetectionConfidence(u8);

impl DetectionConfidence {
    pub fn new(value: u8) -> Result<Self, DetectionError> {
        if value > 100 {
            return Err(DetectionError::InvalidConfidence);
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

/// The deterministic evaluation method that emitted a detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectionMethod {
    DeterministicRule,
}

impl DetectionMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeterministicRule => "deterministic_rule",
        }
    }
}

/// The parser artifact source that supplied a matched rule signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DetectionEvidenceSource {
    Html,
    Header,
    Script,
    Dns,
    Tls,
}

impl DetectionEvidenceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Header => "header",
            Self::Script => "script",
            Self::Dns => "dns",
            Self::Tls => "tls",
        }
    }
}

/// A bounded, normalized evidence item that caused a rule signal to match.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DetectionEvidence {
    source: DetectionEvidenceSource,
    key: String,
    value: String,
}

impl DetectionEvidence {
    pub fn new(
        source: DetectionEvidenceSource,
        key: &str,
        value: &str,
    ) -> Result<Self, DetectionError> {
        let key = bounded_required(key, MAX_EVIDENCE_KEY_LENGTH)
            .ok_or(DetectionError::InvalidEvidenceKey)?;
        let value = bounded_required(value, MAX_EVIDENCE_VALUE_LENGTH)
            .ok_or(DetectionError::InvalidEvidenceValue)?;
        Ok(Self { source, key, value })
    }

    pub fn source(&self) -> DetectionEvidenceSource {
        self.source
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

/// One immutable deterministic observation for a successful crawl snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    technology_slug: TechnologySlug,
    technology_category_slug: TechnologyCategorySlug,
    rule_slug: RuleSlug,
    rule_version: RuleVersion,
    confidence: DetectionConfidence,
    method: DetectionMethod,
    evidence: Vec<DetectionEvidence>,
}

impl Detection {
    pub fn new(
        technology_slug: TechnologySlug,
        technology_category_slug: TechnologyCategorySlug,
        rule_slug: RuleSlug,
        rule_version: RuleVersion,
        confidence: DetectionConfidence,
        method: DetectionMethod,
        mut evidence: Vec<DetectionEvidence>,
    ) -> Result<Self, DetectionError> {
        evidence.sort();
        evidence.dedup();
        if evidence.is_empty() {
            return Err(DetectionError::MissingEvidence);
        }
        Ok(Self {
            technology_slug,
            technology_category_slug,
            rule_slug,
            rule_version,
            confidence,
            method,
            evidence,
        })
    }

    pub fn technology_slug(&self) -> &TechnologySlug {
        &self.technology_slug
    }

    pub fn technology_category_slug(&self) -> &TechnologyCategorySlug {
        &self.technology_category_slug
    }

    pub fn rule_slug(&self) -> &RuleSlug {
        &self.rule_slug
    }

    pub fn rule_version(&self) -> RuleVersion {
        self.rule_version
    }

    pub fn confidence(&self) -> DetectionConfidence {
        self.confidence
    }

    pub fn method(&self) -> DetectionMethod {
        self.method
    }

    pub fn evidence(&self) -> &[DetectionEvidence] {
        &self.evidence
    }
}

fn valid_slug(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_SLUG_LENGTH
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return None;
    }
    Some(value.to_owned())
}

fn bounded_required(value: &str, limit: usize) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.chars().count() <= limit).then(|| value.to_owned())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DetectionError {
    #[error("technology slug must be lowercase ASCII words separated by hyphens")]
    InvalidTechnologySlug,
    #[error("technology category slug must be lowercase ASCII words separated by hyphens")]
    InvalidTechnologyCategorySlug,
    #[error("rule slug must be lowercase ASCII words separated by hyphens")]
    InvalidRuleSlug,
    #[error("rule version must be greater than zero")]
    InvalidRuleVersion,
    #[error("detection confidence must be between 0 and 100")]
    InvalidConfidence,
    #[error("detection evidence key is invalid")]
    InvalidEvidenceKey,
    #[error("detection evidence value is invalid")]
    InvalidEvidenceValue,
    #[error("a detection requires at least one evidence item")]
    MissingEvidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_requires_valid_identifiers_and_evidence() {
        assert!(TechnologySlug::parse("Next-js").is_err());
        assert!(RuleSlug::parse("nextjs-v1").is_ok());
        assert!(RuleVersion::new(0).is_err());
        assert!(DetectionConfidence::new(101).is_err());

        let detection = Detection::new(
            TechnologySlug::parse("nextjs").expect("valid technology"),
            TechnologyCategorySlug::parse("framework").expect("valid category"),
            RuleSlug::parse("nextjs-v1").expect("valid rule"),
            RuleVersion::new(1).expect("valid version"),
            DetectionConfidence::new(90).expect("valid confidence"),
            DetectionMethod::DeterministicRule,
            vec![
                DetectionEvidence::new(DetectionEvidenceSource::Header, "x-powered-by", "Next.js")
                    .expect("valid evidence"),
                DetectionEvidence::new(DetectionEvidenceSource::Header, "x-powered-by", "Next.js")
                    .expect("valid evidence"),
            ],
        )
        .expect("detection should be valid");

        assert_eq!(detection.evidence().len(), 1);
    }
}
