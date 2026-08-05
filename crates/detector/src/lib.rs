#![forbid(unsafe_code)]

//! Deterministic, versioned technology detection over normalized parser evidence.

use regex::Regex;
use techatlas_models::{
    ActiveDetectionRule, Detection, DetectionConfidence, DetectionError, DetectionEvidence,
    DetectionEvidenceSource, DetectionMethod, ReprocessingEvaluation, RuleObservation,
    RuleObservationStatus, RuleSlug, RuleVersion, TechnologyCategorySlug, TechnologySlug,
};
use techatlas_parser::{ArtifactSource, NormalizedEvidence, ParsedArtifacts, SignalAvailability};
use thiserror::Error;

const DETECTION_THRESHOLD: u8 = 70;
const MAX_CONFIDENCE: u8 = 100;

/// Evaluates the initial published rule catalogue against parser output.
///
/// Only matching evidence is emitted. Unavailable and missing parser signals are never
/// interpreted as negative evidence.
pub fn evaluate(parsed: &ParsedArtifacts) -> Result<DetectionEvaluation, DetectorError> {
    let evaluations = initial_rules()
        .iter()
        .map(|rule| evaluate_rule(rule, parsed))
        .collect::<Result<Vec<_>, _>>()?;
    let mut detections = evaluations
        .iter()
        .filter_map(|evaluation| evaluation.detection.clone())
        .collect::<Vec<_>>();
    detections.sort_by(|left, right| {
        left.technology_slug()
            .as_str()
            .cmp(right.technology_slug().as_str())
    });
    Ok(DetectionEvaluation {
        detections,
        observations: evaluations
            .into_iter()
            .map(|evaluation| evaluation.observation)
            .collect(),
    })
}

/// Evaluates the active, database-owned deterministic rule catalogue.
pub fn evaluate_active(
    parsed: &ParsedArtifacts,
    rules: &[ActiveDetectionRule],
) -> Result<DetectionEvaluation, DetectorError> {
    let evaluations = rules
        .iter()
        .map(|rule| evaluate_active_rule(rule, parsed))
        .collect::<Result<Vec<_>, _>>()?;
    let mut detections = evaluations
        .iter()
        .filter_map(|evaluation| evaluation.detection.clone())
        .collect::<Vec<_>>();
    detections.sort_by(|left, right| {
        left.technology_slug()
            .as_str()
            .cmp(right.technology_slug().as_str())
    });
    Ok(DetectionEvaluation {
        detections,
        observations: evaluations
            .into_iter()
            .map(|evaluation| evaluation.observation)
            .collect(),
    })
}

/// Evaluates one immutable published rule for a historical reprocessing item.
/// Version evidence is optional and is never inferred when its capture is absent.
pub fn evaluate_reprocessing(
    parsed: &ParsedArtifacts,
    rule: &ActiveDetectionRule,
) -> Result<ReprocessingEvaluation, DetectorError> {
    let evaluation = evaluate_active_rule(rule, parsed)?;
    Ok(ReprocessingEvaluation {
        detection: evaluation.detection,
        observation: evaluation.observation,
        technology_version: evaluation.technology_version,
    })
}

/// The detections and coverage observations emitted by one deterministic evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectionEvaluation {
    detections: Vec<Detection>,
    observations: Vec<RuleObservation>,
}

impl DetectionEvaluation {
    pub fn detections(&self) -> &[Detection] {
        &self.detections
    }
    pub fn observations(&self) -> &[RuleObservation] {
        &self.observations
    }
    pub fn into_parts(self) -> (Vec<Detection>, Vec<RuleObservation>) {
        (self.detections, self.observations)
    }
}

/// The confidence threshold used by the published v1 catalogue.
pub const fn detection_threshold() -> u8 {
    DETECTION_THRESHOLD
}

struct RuleDefinition {
    technology_slug: &'static str,
    technology_category_slug: &'static str,
    rule_slug: &'static str,
    version: u16,
    threshold: u8,
    signals: &'static [SignalDefinition],
}

struct SignalDefinition {
    source: ArtifactSource,
    key: &'static str,
    matcher: SignalMatcher,
    weight: u8,
}

#[derive(Clone, Copy)]
enum SignalMatcher {
    EqualsIgnoreCase(&'static str),
    ContainsIgnoreCase(&'static str),
    Present,
}

const NEXTJS_SIGNALS: &[SignalDefinition] = &[
    SignalDefinition {
        source: ArtifactSource::Header,
        key: "x-powered-by",
        matcher: SignalMatcher::EqualsIgnoreCase("next.js"),
        weight: 90,
    },
    SignalDefinition {
        source: ArtifactSource::Script,
        key: "src",
        matcher: SignalMatcher::ContainsIgnoreCase("/_next/"),
        weight: 75,
    },
    SignalDefinition {
        source: ArtifactSource::Html,
        key: "meta.generator",
        matcher: SignalMatcher::EqualsIgnoreCase("next.js"),
        weight: 80,
    },
];

const STRIPE_SIGNALS: &[SignalDefinition] = &[SignalDefinition {
    source: ArtifactSource::Script,
    key: "src",
    matcher: SignalMatcher::ContainsIgnoreCase("js.stripe.com"),
    weight: 95,
}];

const CLOUDFLARE_SIGNALS: &[SignalDefinition] = &[
    SignalDefinition {
        source: ArtifactSource::Header,
        key: "server",
        matcher: SignalMatcher::EqualsIgnoreCase("cloudflare"),
        weight: 90,
    },
    SignalDefinition {
        source: ArtifactSource::Header,
        key: "cf-ray",
        matcher: SignalMatcher::Present,
        weight: 80,
    },
];

const POSTHOG_SIGNALS: &[SignalDefinition] = &[SignalDefinition {
    source: ArtifactSource::Script,
    key: "src",
    matcher: SignalMatcher::ContainsIgnoreCase("posthog.com"),
    weight: 90,
}];

const SHOPIFY_SIGNALS: &[SignalDefinition] = &[
    SignalDefinition {
        source: ArtifactSource::Script,
        key: "src",
        matcher: SignalMatcher::ContainsIgnoreCase("cdn.shopify.com"),
        weight: 95,
    },
    SignalDefinition {
        source: ArtifactSource::Header,
        key: "x-shopify-stage",
        matcher: SignalMatcher::Present,
        weight: 90,
    },
];

const INITIAL_RULES: &[RuleDefinition] = &[
    RuleDefinition {
        technology_slug: "nextjs",
        technology_category_slug: "framework",
        rule_slug: "nextjs-v1",
        version: 1,
        threshold: DETECTION_THRESHOLD,
        signals: NEXTJS_SIGNALS,
    },
    RuleDefinition {
        technology_slug: "stripe",
        technology_category_slug: "payment-provider",
        rule_slug: "stripe-v1",
        version: 1,
        threshold: DETECTION_THRESHOLD,
        signals: STRIPE_SIGNALS,
    },
    RuleDefinition {
        technology_slug: "cloudflare",
        technology_category_slug: "hosting",
        rule_slug: "cloudflare-v1",
        version: 1,
        threshold: DETECTION_THRESHOLD,
        signals: CLOUDFLARE_SIGNALS,
    },
    RuleDefinition {
        technology_slug: "posthog",
        technology_category_slug: "analytics",
        rule_slug: "posthog-v1",
        version: 1,
        threshold: DETECTION_THRESHOLD,
        signals: POSTHOG_SIGNALS,
    },
    RuleDefinition {
        technology_slug: "shopify",
        technology_category_slug: "ecommerce",
        rule_slug: "shopify-v1",
        version: 1,
        threshold: DETECTION_THRESHOLD,
        signals: SHOPIFY_SIGNALS,
    },
];

fn initial_rules() -> &'static [RuleDefinition] {
    INITIAL_RULES
}

fn evaluate_rule(
    rule: &RuleDefinition,
    parsed: &ParsedArtifacts,
) -> Result<RuleEvaluation, DetectorError> {
    let mut confidence = 0u8;
    let mut matched_evidence = Vec::new();

    for signal in rule.signals {
        let matches = parsed
            .evidence()
            .iter()
            .filter(|item| signal.matches(item))
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        confidence = confidence.saturating_add(signal.weight).min(MAX_CONFIDENCE);
        for item in matches {
            matched_evidence.push(to_detection_evidence(item)?);
        }
    }

    let technology_slug = TechnologySlug::parse(rule.technology_slug)?;
    let technology_category_slug = TechnologyCategorySlug::parse(rule.technology_category_slug)?;
    let rule_slug = RuleSlug::parse(rule.rule_slug)?;
    let rule_version = RuleVersion::new(rule.version)?;
    let has_complete_coverage = rule
        .signals
        .iter()
        .all(|signal| parsed.availability(signal.source) == SignalAvailability::Available);
    let detection = if confidence >= rule.threshold {
        Some(Detection::new(
            technology_slug.clone(),
            technology_category_slug.clone(),
            rule_slug.clone(),
            rule_version,
            DetectionConfidence::new(confidence)?,
            DetectionMethod::DeterministicRule,
            matched_evidence.clone(),
        )?)
    } else {
        None
    };
    let status = if detection.is_some() {
        RuleObservationStatus::Detected
    } else if matched_evidence.is_empty() && has_complete_coverage {
        RuleObservationStatus::ConfirmedAbsent
    } else {
        RuleObservationStatus::Unknown
    };
    Ok(RuleEvaluation {
        detection,
        observation: RuleObservation::new(
            technology_slug,
            technology_category_slug,
            rule_slug,
            rule_version,
            status,
        ),
        technology_version: None,
    })
}

fn evaluate_active_rule(
    rule: &ActiveDetectionRule,
    parsed: &ParsedArtifacts,
) -> Result<RuleEvaluation, DetectorError> {
    let definition = rule
        .definition
        .as_object()
        .ok_or(DetectorError::InvalidRuleDefinition)?;
    let threshold = definition
        .get("threshold")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or(DetectorError::InvalidRuleDefinition)?;
    let signals = definition
        .get("signals")
        .and_then(serde_json::Value::as_array)
        .filter(|signals| !signals.is_empty())
        .ok_or(DetectorError::InvalidRuleDefinition)?;
    let mut confidence = 0_u8;
    let mut matched_evidence = Vec::new();
    let mut complete_coverage = true;
    for signal in signals {
        let signal = signal
            .as_object()
            .ok_or(DetectorError::InvalidRuleDefinition)?;
        let source = parse_source(signal.get("source").and_then(serde_json::Value::as_str))?;
        let key = signal
            .get("key")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or(DetectorError::InvalidRuleDefinition)?;
        let matcher = signal
            .get("match")
            .and_then(serde_json::Value::as_str)
            .ok_or(DetectorError::InvalidRuleDefinition)?;
        let expected = signal.get("value").and_then(serde_json::Value::as_str);
        let weight = signal
            .get("weight")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u8::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or(DetectorError::InvalidRuleDefinition)?;
        complete_coverage &= parsed.availability(source) == SignalAvailability::Available;
        let matched = parsed
            .evidence()
            .iter()
            .filter(|item| {
                item.source() == source
                    && item.key().eq_ignore_ascii_case(key)
                    && matches_dynamic(item.value(), matcher, expected)
            })
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            confidence = confidence.saturating_add(weight).min(MAX_CONFIDENCE);
            for item in matched {
                matched_evidence.push(to_detection_evidence(item)?);
            }
        }
    }
    let technology_slug = TechnologySlug::parse(&rule.technology_slug)?;
    let technology_category_slug = TechnologyCategorySlug::parse(&rule.technology_category_slug)?;
    let rule_slug = RuleSlug::parse(&rule.rule_slug)?;
    let rule_version = RuleVersion::new(rule.version)?;
    let detection = if confidence >= threshold {
        Some(Detection::new(
            technology_slug.clone(),
            technology_category_slug.clone(),
            rule_slug.clone(),
            rule_version,
            DetectionConfidence::new(confidence)?,
            DetectionMethod::DeterministicRule,
            matched_evidence.clone(),
        )?)
    } else {
        None
    };
    let status = if detection.is_some() {
        RuleObservationStatus::Detected
    } else if matched_evidence.is_empty() && complete_coverage {
        RuleObservationStatus::ConfirmedAbsent
    } else {
        RuleObservationStatus::Unknown
    };
    let technology_version = if detection.is_some() {
        extract_dynamic_version(definition, parsed)?
    } else {
        None
    };
    Ok(RuleEvaluation {
        detection,
        observation: RuleObservation::new(
            technology_slug,
            technology_category_slug,
            rule_slug,
            rule_version,
            status,
        ),
        technology_version,
    })
}

fn extract_dynamic_version(
    definition: &serde_json::Map<String, serde_json::Value>,
    parsed: &ParsedArtifacts,
) -> Result<Option<String>, DetectorError> {
    let Some(version_evidence) = definition.get("version_evidence") else {
        return Ok(None);
    };
    let version_evidence = version_evidence
        .as_object()
        .ok_or(DetectorError::InvalidRuleDefinition)?;
    let source = parse_source(
        version_evidence
            .get("source")
            .and_then(serde_json::Value::as_str),
    )?;
    let key = version_evidence
        .get("key")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 128)
        .ok_or(DetectorError::InvalidRuleDefinition)?;
    let pattern = version_evidence
        .get("pattern")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .ok_or(DetectorError::InvalidRuleDefinition)?;
    let regex = Regex::new(pattern).map_err(|_| DetectorError::InvalidRuleDefinition)?;
    if regex.captures_len() != 2 {
        return Err(DetectorError::InvalidRuleDefinition);
    }
    let version = parsed
        .evidence()
        .iter()
        .filter(|item| item.source() == source && item.key().eq_ignore_ascii_case(key))
        .find_map(|item| {
            regex
                .captures(item.value())
                .and_then(|captures| captures.get(1))
        })
        .map(|capture| capture.as_str().trim().to_owned())
        .filter(|value| !value.is_empty() && value.chars().count() <= 128);
    Ok(version)
}

fn parse_source(value: Option<&str>) -> Result<ArtifactSource, DetectorError> {
    match value {
        Some("html") => Ok(ArtifactSource::Html),
        Some("header") => Ok(ArtifactSource::Header),
        Some("script") => Ok(ArtifactSource::Script),
        Some("dns") => Ok(ArtifactSource::Dns),
        Some("tls") => Ok(ArtifactSource::Tls),
        _ => Err(DetectorError::InvalidRuleDefinition),
    }
}

fn matches_dynamic(value: &str, matcher: &str, expected: Option<&str>) -> bool {
    match matcher {
        "present" => !value.is_empty(),
        "equals_ignore_case" => {
            expected.is_some_and(|expected| value.eq_ignore_ascii_case(expected))
        }
        "contains_ignore_case" => {
            expected.is_some_and(|expected| contains_ignore_case(value, expected))
        }
        _ => false,
    }
}

struct RuleEvaluation {
    detection: Option<Detection>,
    observation: RuleObservation,
    technology_version: Option<String>,
}

impl SignalDefinition {
    fn matches(&self, evidence: &NormalizedEvidence) -> bool {
        evidence.source() == self.source
            && evidence.key().eq_ignore_ascii_case(self.key)
            && match self.matcher {
                SignalMatcher::EqualsIgnoreCase(expected) => {
                    evidence.value().eq_ignore_ascii_case(expected)
                }
                SignalMatcher::ContainsIgnoreCase(expected) => {
                    contains_ignore_case(evidence.value(), expected)
                }
                SignalMatcher::Present => !evidence.value().is_empty(),
            }
    }
}

fn contains_ignore_case(value: &str, expected: &str) -> bool {
    value
        .to_ascii_lowercase()
        .contains(&expected.to_ascii_lowercase())
}

fn to_detection_evidence(
    evidence: &NormalizedEvidence,
) -> Result<DetectionEvidence, DetectorError> {
    DetectionEvidence::new(
        match evidence.source() {
            ArtifactSource::Html => DetectionEvidenceSource::Html,
            ArtifactSource::Header => DetectionEvidenceSource::Header,
            ArtifactSource::Script => DetectionEvidenceSource::Script,
            ArtifactSource::Dns => DetectionEvidenceSource::Dns,
            ArtifactSource::Tls => DetectionEvidenceSource::Tls,
        },
        evidence.key(),
        evidence.value(),
    )
    .map_err(DetectorError::InvalidDetection)
}

#[derive(Debug, Error)]
pub enum DetectorError {
    #[error("active rule definition is invalid")]
    InvalidRuleDefinition,
    #[error("the static detector catalogue produced an invalid detection")]
    InvalidDetection(#[from] DetectionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use techatlas_parser::{ArtifactBundle, ArtifactInput, parse};

    fn parsed(html: &str, headers: &[(&str, &str)]) -> ParsedArtifacts {
        parse(&ArtifactBundle {
            html: ArtifactInput::Captured(html.as_bytes().to_vec()),
            headers: ArtifactInput::Captured(
                headers
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                    .collect::<BTreeMap<_, _>>(),
            ),
            ..ArtifactBundle::default()
        })
    }

    #[test]
    fn detects_the_initial_catalogue_from_documented_signals() {
        let detections = evaluate(&parsed(
            include_str!("../../../tests/fixtures/detector/initial-catalogue.html"),
            &[
                ("server", "cloudflare"),
                ("x-powered-by", "Next.js"),
                ("cf-ray", "123"),
                ("x-shopify-stage", "production"),
            ],
        ))
        .expect("static rules should be valid");

        assert_eq!(detections.detections().len(), 5);
        assert!(
            detections
                .detections()
                .iter()
                .all(|detection| detection.confidence().get() >= detection_threshold())
        );
        assert_eq!(
            detections
                .detections()
                .iter()
                .find(|detection| detection.technology_slug().as_str() == "nextjs")
                .expect("Next.js should be detected")
                .confidence()
                .get(),
            100
        );
    }

    #[test]
    fn ignores_lookalikes_and_does_not_count_duplicate_signals_twice() {
        let detections = evaluate(&parsed(
            include_str!("../../../tests/fixtures/detector/lookalikes.html"),
            &[("server", "cloudflared")],
        ))
        .expect("static rules should be valid");

        assert_eq!(detections.detections().len(), 1);
        assert_eq!(
            detections.detections()[0].technology_slug().as_str(),
            "stripe"
        );
        assert_eq!(detections.detections()[0].confidence().get(), 95);
        assert_eq!(detections.detections()[0].evidence().len(), 1);
    }

    #[test]
    fn unavailable_or_missing_signals_do_not_imply_absence_or_emit_a_detection() {
        let detections = evaluate(&parse(&ArtifactBundle::default())).expect("rules are valid");

        assert!(detections.detections().is_empty());
        assert!(
            detections
                .observations()
                .iter()
                .all(|observation| observation.status() == RuleObservationStatus::Unknown)
        );
    }

    #[test]
    fn evaluates_only_the_active_database_rule_version() {
        let rules = vec![ActiveDetectionRule {
            technology_slug: "nextjs".to_owned(),
            technology_category_slug: "framework".to_owned(),
            rule_slug: "nextjs-v1".to_owned(),
            version: 2,
            definition: serde_json::json!({
                "threshold": 70,
                "signals": [{"source":"header", "key":"x-powered-by", "match":"equals_ignore_case", "value":"next.js", "weight":90}]
            }),
        }];
        let evaluation = evaluate_active(&parsed("", &[("x-powered-by", "Next.js")]), &rules)
            .expect("active rule should evaluate");
        assert_eq!(evaluation.detections().len(), 1);
        assert_eq!(evaluation.detections()[0].rule_version().get(), 2);
    }

    #[test]
    fn reprocessing_extracts_only_explicit_version_captures() {
        let rule = ActiveDetectionRule {
            technology_slug: "nextjs".to_owned(),
            technology_category_slug: "framework".to_owned(),
            rule_slug: "nextjs-v1".to_owned(),
            version: 3,
            definition: serde_json::json!({
                "threshold": 70,
                "signals": [{"source":"header", "key":"x-powered-by", "match":"contains_ignore_case", "value":"next", "weight":90}],
                "version_evidence": {"source":"header", "key":"x-powered-by", "pattern":"(?i)next\\.js/([0-9.]+)"}
            }),
        };
        let versioned =
            evaluate_reprocessing(&parsed("", &[("x-powered-by", "Next.js/14.2.1")]), &rule)
                .expect("valid version rule should evaluate");
        assert_eq!(versioned.technology_version.as_deref(), Some("14.2.1"));

        let unknown = evaluate_reprocessing(&parsed("", &[("x-powered-by", "Next.js")]), &rule)
            .expect("missing capture is not an error");
        assert!(unknown.technology_version.is_none());
    }
}
