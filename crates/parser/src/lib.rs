#![forbid(unsafe_code)]

//! Pure extraction and normalization of captured crawl artifacts.

use scraper::{Html, Selector};
use std::{collections::BTreeMap, net::IpAddr, sync::LazyLock};

static TITLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("title is a valid selector"));
static META_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("meta").expect("meta is a valid selector"));
static SCRIPT_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script").expect("script is a valid selector"));

/// Captured artifacts accepted by [`parse`]. None of the variants trigger I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactBundle {
    pub html: ArtifactInput<Vec<u8>>,
    pub headers: ArtifactInput<BTreeMap<String, String>>,
    pub dns: ArtifactInput<DnsArtifact>,
    pub tls: ArtifactInput<TlsArtifact>,
}

impl Default for ArtifactBundle {
    fn default() -> Self {
        Self {
            html: ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
            headers: ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
            dns: ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
            tls: ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
        }
    }
}

/// A captured signal, or the explicit reason it was unavailable during acquisition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactInput<T> {
    Captured(T),
    Unavailable(UnavailableReason),
}

/// Reasons a signal cannot be interpreted as evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    NotCaptured,
    Invalid,
    Unsupported,
}

/// DNS facts captured by an acquisition layer. The parser does not resolve names itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsArtifact {
    pub queried_name: String,
    pub addresses: Vec<IpAddr>,
}

/// TLS facts captured by an acquisition layer. Any individual fact can be absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TlsArtifact {
    pub protocol: Option<String>,
    pub cipher_suite: Option<String>,
    pub certificate_subject: Option<String>,
    pub certificate_issuer: Option<String>,
    pub subject_alternative_names: Vec<String>,
}

/// A parsed, normalized evidence item suitable for deterministic detector rules.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NormalizedEvidence {
    source: ArtifactSource,
    key: String,
    value: String,
}

impl NormalizedEvidence {
    pub fn source(&self) -> ArtifactSource {
        self.source
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

/// The captured artifact that supplied a normalized evidence item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactSource {
    Html,
    Header,
    Script,
    Dns,
    Tls,
}

/// Whether parsing produced a usable signal for an artifact source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalAvailability {
    Available,
    Unavailable(UnavailableReason),
}

/// The deterministic result of parsing an [`ArtifactBundle`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedArtifacts {
    evidence: Vec<NormalizedEvidence>,
    availability: BTreeMap<ArtifactSource, SignalAvailability>,
}

impl ParsedArtifacts {
    pub fn evidence(&self) -> &[NormalizedEvidence] {
        &self.evidence
    }

    pub fn availability(&self, source: ArtifactSource) -> SignalAvailability {
        self.availability
            .get(&source)
            .copied()
            .unwrap_or(SignalAvailability::Unavailable(
                UnavailableReason::NotCaptured,
            ))
    }
}

/// Parses available artifacts without network, filesystem, clock, or persistence access.
pub fn parse(input: &ArtifactBundle) -> ParsedArtifacts {
    let mut evidence = Vec::new();
    let mut availability = BTreeMap::new();

    parse_html(&input.html, &mut evidence, &mut availability);
    parse_headers(&input.headers, &mut evidence, &mut availability);
    parse_dns(&input.dns, &mut evidence, &mut availability);
    parse_tls(&input.tls, &mut evidence, &mut availability);

    evidence.sort();
    evidence.dedup();
    ParsedArtifacts {
        evidence,
        availability,
    }
}

fn parse_html(
    input: &ArtifactInput<Vec<u8>>,
    evidence: &mut Vec<NormalizedEvidence>,
    availability: &mut BTreeMap<ArtifactSource, SignalAvailability>,
) {
    let ArtifactInput::Captured(bytes) = input else {
        let reason = unavailable_reason(input);
        availability.insert(
            ArtifactSource::Html,
            SignalAvailability::Unavailable(reason),
        );
        availability.insert(
            ArtifactSource::Script,
            SignalAvailability::Unavailable(reason),
        );
        return;
    };
    let Ok(html) = std::str::from_utf8(bytes) else {
        availability.insert(
            ArtifactSource::Html,
            SignalAvailability::Unavailable(UnavailableReason::Invalid),
        );
        availability.insert(
            ArtifactSource::Script,
            SignalAvailability::Unavailable(UnavailableReason::Invalid),
        );
        return;
    };

    let document = Html::parse_document(html);
    if let Some(title) = document
        .select(&TITLE_SELECTOR)
        .next()
        .and_then(|element| normalized(element.text().collect::<String>()))
    {
        evidence.push(item(ArtifactSource::Html, "title", title));
    }
    for element in document.select(&META_SELECTOR) {
        let Some(key) = element
            .value()
            .attr("name")
            .or_else(|| element.value().attr("property"))
            .or_else(|| element.value().attr("http-equiv"))
            .and_then(normalized_key)
        else {
            continue;
        };
        let Some(value) = element.value().attr("content").and_then(normalized) else {
            continue;
        };
        evidence.push(item(ArtifactSource::Html, format!("meta.{key}"), value));
    }
    for element in document.select(&SCRIPT_SELECTOR) {
        let Some(source) = element.value().attr("src").and_then(normalized) else {
            continue;
        };
        evidence.push(item(ArtifactSource::Script, "src", source));
    }
    availability.insert(ArtifactSource::Html, SignalAvailability::Available);
    availability.insert(ArtifactSource::Script, SignalAvailability::Available);
}

fn parse_headers(
    input: &ArtifactInput<BTreeMap<String, String>>,
    evidence: &mut Vec<NormalizedEvidence>,
    availability: &mut BTreeMap<ArtifactSource, SignalAvailability>,
) {
    let ArtifactInput::Captured(headers) = input else {
        availability.insert(
            ArtifactSource::Header,
            SignalAvailability::Unavailable(unavailable_reason(input)),
        );
        return;
    };
    for (name, value) in headers {
        let Some(name) = normalized_key(name) else {
            continue;
        };
        if is_sensitive_header(&name) {
            continue;
        }
        let Some(value) = normalized(value) else {
            continue;
        };
        evidence.push(item(ArtifactSource::Header, name, value));
    }
    availability.insert(ArtifactSource::Header, SignalAvailability::Available);
}

fn parse_dns(
    input: &ArtifactInput<DnsArtifact>,
    evidence: &mut Vec<NormalizedEvidence>,
    availability: &mut BTreeMap<ArtifactSource, SignalAvailability>,
) {
    let ArtifactInput::Captured(dns) = input else {
        availability.insert(
            ArtifactSource::Dns,
            SignalAvailability::Unavailable(unavailable_reason(input)),
        );
        return;
    };
    let Some(name) = normalized_hostname(&dns.queried_name) else {
        availability.insert(
            ArtifactSource::Dns,
            SignalAvailability::Unavailable(UnavailableReason::Invalid),
        );
        return;
    };
    evidence.push(item(ArtifactSource::Dns, "queried_name", name));
    for address in &dns.addresses {
        evidence.push(item(ArtifactSource::Dns, "address", address.to_string()));
    }
    availability.insert(ArtifactSource::Dns, SignalAvailability::Available);
}

fn parse_tls(
    input: &ArtifactInput<TlsArtifact>,
    evidence: &mut Vec<NormalizedEvidence>,
    availability: &mut BTreeMap<ArtifactSource, SignalAvailability>,
) {
    let ArtifactInput::Captured(tls) = input else {
        availability.insert(
            ArtifactSource::Tls,
            SignalAvailability::Unavailable(unavailable_reason(input)),
        );
        return;
    };

    let mut emitted = false;
    for (key, value) in [
        ("protocol", tls.protocol.as_deref()),
        ("cipher_suite", tls.cipher_suite.as_deref()),
        ("certificate_subject", tls.certificate_subject.as_deref()),
        ("certificate_issuer", tls.certificate_issuer.as_deref()),
    ] {
        if let Some(value) = value.and_then(normalized) {
            evidence.push(item(ArtifactSource::Tls, key, value));
            emitted = true;
        }
    }
    for name in &tls.subject_alternative_names {
        if let Some(name) = normalized_hostname(name) {
            evidence.push(item(ArtifactSource::Tls, "subject_alternative_name", name));
            emitted = true;
        }
    }
    availability.insert(
        ArtifactSource::Tls,
        if emitted {
            SignalAvailability::Available
        } else {
            SignalAvailability::Unavailable(UnavailableReason::Invalid)
        },
    );
}

fn unavailable_reason<T>(input: &ArtifactInput<T>) -> UnavailableReason {
    match input {
        ArtifactInput::Captured(_) => UnavailableReason::Invalid,
        ArtifactInput::Unavailable(reason) => *reason,
    }
}

fn item(
    source: ArtifactSource,
    key: impl Into<String>,
    value: impl Into<String>,
) -> NormalizedEvidence {
    NormalizedEvidence {
        source,
        key: key.into(),
        value: value.into(),
    }
}

fn normalized(value: impl AsRef<str>) -> Option<String> {
    let value = value
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!value.is_empty()).then_some(value)
}

fn normalized_key(value: impl AsRef<str>) -> Option<String> {
    normalized(value).map(|value| value.to_ascii_lowercase())
}

fn normalized_hostname(value: impl AsRef<str>) -> Option<String> {
    let value = normalized(value)?;
    let value = value.trim_end_matches('.').to_ascii_lowercase();
    (!value.is_empty() && !value.contains(char::is_whitespace)).then_some(value)
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name,
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "set-cookie2"
            | "x-api-key"
            | "x-auth-token"
            | "x-amz-security-token"
    ) || name.contains("token")
        || name.contains("secret")
        || name == "api-key"
        || name.ends_with("-key")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_bundle() -> ArtifactBundle {
        ArtifactBundle {
            html: ArtifactInput::Captured(
                include_bytes!("../../../tests/fixtures/parser/mixed-artifacts.html").to_vec(),
            ),
            headers: ArtifactInput::Captured(BTreeMap::from([
                ("Server".to_owned(), " nginx  ".to_owned()),
                ("server".to_owned(), "nginx".to_owned()),
                ("Set-Cookie".to_owned(), "session=private".to_owned()),
            ])),
            dns: ArtifactInput::Captured(DnsArtifact {
                queried_name: "Example.TEST.".to_owned(),
                addresses: vec![
                    "2001:db8::1".parse().expect("fixture address is valid"),
                    "192.0.2.10".parse().expect("fixture address is valid"),
                ],
            }),
            tls: ArtifactInput::Captured(TlsArtifact {
                protocol: Some(" TLSv1.3 ".to_owned()),
                cipher_suite: Some("TLS_AES_256_GCM_SHA384".to_owned()),
                certificate_subject: Some("CN=example.test".to_owned()),
                certificate_issuer: Some("CN=Example Issuer".to_owned()),
                subject_alternative_names: vec![
                    "EXAMPLE.TEST.".to_owned(),
                    "www.example.test".to_owned(),
                ],
            }),
        }
    }

    #[test]
    fn parses_and_normalizes_available_fixture_artifacts() {
        let parsed = parse(&fixture_bundle());

        assert_eq!(
            parsed.availability(ArtifactSource::Html),
            SignalAvailability::Available
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Script),
            SignalAvailability::Available
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Header),
            SignalAvailability::Available
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Dns),
            SignalAvailability::Available
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Tls),
            SignalAvailability::Available
        );
        assert!(parsed.evidence().windows(2).all(|pair| pair[0] < pair[1]));
        assert!(parsed.evidence().iter().any(|item| {
            item.source() == ArtifactSource::Html
                && item.key() == "meta.generator"
                && item.value() == "Tech Atlas"
        }));
        assert!(parsed.evidence().iter().any(|item| {
            item.source() == ArtifactSource::Script
                && item.key() == "src"
                && item.value() == "/assets/app.js"
        }));
        assert!(parsed.evidence().iter().any(|item| {
            item.source() == ArtifactSource::Dns
                && item.key() == "queried_name"
                && item.value() == "example.test"
        }));
        assert!(
            parsed
                .evidence()
                .iter()
                .all(|item| item.key() != "set-cookie")
        );
        assert_eq!(
            parsed
                .evidence()
                .iter()
                .filter(|item| item.source() == ArtifactSource::Header && item.key() == "server")
                .count(),
            1
        );
    }

    #[test]
    fn classifies_unavailable_and_invalid_signals_without_failing_the_parse() {
        let parsed = parse(&ArtifactBundle {
            html: ArtifactInput::Captured(vec![0xff]),
            headers: ArtifactInput::Unavailable(UnavailableReason::Unsupported),
            dns: ArtifactInput::Captured(DnsArtifact {
                queried_name: "   ".to_owned(),
                addresses: Vec::new(),
            }),
            tls: ArtifactInput::Captured(TlsArtifact::default()),
        });

        assert_eq!(
            parsed.availability(ArtifactSource::Html),
            SignalAvailability::Unavailable(UnavailableReason::Invalid)
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Script),
            SignalAvailability::Unavailable(UnavailableReason::Invalid)
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Header),
            SignalAvailability::Unavailable(UnavailableReason::Unsupported)
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Dns),
            SignalAvailability::Unavailable(UnavailableReason::Invalid)
        );
        assert_eq!(
            parsed.availability(ArtifactSource::Tls),
            SignalAvailability::Unavailable(UnavailableReason::Invalid)
        );
        assert!(parsed.evidence().is_empty());
    }

    #[test]
    fn malformed_html_is_parsed_without_panicking() {
        let parsed = parse(&ArtifactBundle {
            html: ArtifactInput::Captured(
                include_bytes!("../../../tests/fixtures/parser/malformed.html").to_vec(),
            ),
            ..ArtifactBundle::default()
        });

        assert_eq!(
            parsed.availability(ArtifactSource::Html),
            SignalAvailability::Available
        );
        assert!(parsed.evidence().iter().any(|item| {
            item.source() == ArtifactSource::Html
                && item.key() == "meta.framework"
                && item.value() == "Example Framework"
        }));
    }
}
