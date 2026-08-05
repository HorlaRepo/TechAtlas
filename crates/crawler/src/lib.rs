#![forbid(unsafe_code)]

//! Safe, bounded HTTP acquisition primitives for TechAtlas crawls.

use async_trait::async_trait;
use regex::Regex;
use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue},
    redirect::Policy,
};
use rustls::{
    ClientConfig, RootCertStore,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, LazyLock},
    time::Duration,
};
use thiserror::Error;
use time::OffsetDateTime;
use tokio::{net::TcpStream, time::timeout};
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;
use url::{Host, Url};
use x509_parser::{extensions::GeneralName, parse_x509_certificate};

const DEFAULT_USER_AGENT: &str = "TechAtlas/0.1";
const DEFAULT_ROBOTS_USER_AGENT: &str = "TechAtlas";
const DEFAULT_CONTACT: &str = "mailto:ops@techatlas.invalid";

/// Configures bounded, identifiable crawl acquisition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlerConfig {
    pub user_agent: String,
    pub robots_user_agent: String,
    pub contact: String,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub total_timeout: Duration,
    pub dns_timeout: Duration,
    pub dns_max_addresses: usize,
    pub tls_timeout: Duration,
    pub tls_max_addresses: usize,
    pub max_redirects: usize,
    pub max_response_bytes: u64,
    pub politeness_delay: Duration,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            robots_user_agent: DEFAULT_ROBOTS_USER_AGENT.to_owned(),
            contact: DEFAULT_CONTACT.to_owned(),
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(15),
            total_timeout: Duration::from_secs(30),
            dns_timeout: Duration::from_secs(5),
            dns_max_addresses: 16,
            tls_timeout: Duration::from_secs(5),
            tls_max_addresses: 4,
            max_redirects: 10,
            max_response_bytes: 10 * 1024 * 1024,
            politeness_delay: Duration::from_secs(1),
        }
    }
}

impl CrawlerConfig {
    pub fn validate(&self) -> Result<(), CrawlerConfigError> {
        if self.user_agent.trim().is_empty() {
            return Err(CrawlerConfigError::EmptyUserAgent);
        }
        if HeaderValue::from_str(&format!("{} (+{})", self.user_agent, self.contact)).is_err() {
            return Err(CrawlerConfigError::InvalidUserAgent);
        }
        if self.robots_user_agent.trim().is_empty() {
            return Err(CrawlerConfigError::EmptyRobotsUserAgent);
        }
        if self.contact.trim().is_empty() {
            return Err(CrawlerConfigError::EmptyContact);
        }
        if self.connect_timeout.is_zero()
            || self.read_timeout.is_zero()
            || self.total_timeout.is_zero()
            || self.dns_timeout.is_zero()
            || self.tls_timeout.is_zero()
        {
            return Err(CrawlerConfigError::ZeroTimeout);
        }
        if self.max_redirects == 0 {
            return Err(CrawlerConfigError::ZeroRedirectLimit);
        }
        if self.tls_max_addresses == 0 {
            return Err(CrawlerConfigError::ZeroTlsAddressLimit);
        }
        if self.dns_max_addresses == 0 {
            return Err(CrawlerConfigError::ZeroDnsAddressLimit);
        }
        if self.max_response_bytes == 0 {
            return Err(CrawlerConfigError::ZeroBodyLimit);
        }
        if self.politeness_delay.is_zero() {
            return Err(CrawlerConfigError::ZeroPolitenessDelay);
        }

        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CrawlerConfigError {
    #[error("crawler user agent must not be empty")]
    EmptyUserAgent,
    #[error("crawler user agent is not a valid HTTP header value")]
    InvalidUserAgent,
    #[error("robots user agent must not be empty")]
    EmptyRobotsUserAgent,
    #[error("crawler contact must not be empty")]
    EmptyContact,
    #[error("crawler timeouts must be greater than zero")]
    ZeroTimeout,
    #[error("TLS address limit must be greater than zero")]
    ZeroTlsAddressLimit,
    #[error("DNS address limit must be greater than zero")]
    ZeroDnsAddressLimit,
    #[error("crawler redirect limit must be greater than zero")]
    ZeroRedirectLimit,
    #[error("crawler response body limit must be greater than zero")]
    ZeroBodyLimit,
    #[error("crawler politeness delay must be greater than zero")]
    ZeroPolitenessDelay,
}

/// A normalized HTTP target that cannot embed credentials, fragments, or IP literals.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CrawlUrl(Url);

impl CrawlUrl {
    pub fn parse(input: &str) -> Result<Self, CrawlUrlError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CrawlUrlError::Empty);
        }

        let candidate = if input.contains("://") {
            input.to_owned()
        } else {
            format!("https://{input}")
        };
        let url = Url::parse(&candidate).map_err(|_| CrawlUrlError::Invalid)?;
        Self::from_url(url)
    }

    pub fn as_url(&self) -> &Url {
        &self.0
    }

    pub fn host(&self) -> &str {
        self.0.host_str().unwrap_or_default()
    }

    pub fn robots_url(&self) -> Result<Self, CrawlUrlError> {
        let mut url = self.0.clone();
        url.set_path("/robots.txt");
        url.set_query(None);
        url.set_fragment(None);
        Self::from_url(url)
    }

    fn redirect_target(&self, location: &str) -> Result<Self, CrawlUrlError> {
        let target = self.0.join(location).map_err(|_| CrawlUrlError::Invalid)?;
        Self::from_url(target)
    }

    fn from_url(url: Url) -> Result<Self, CrawlUrlError> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(CrawlUrlError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(CrawlUrlError::CredentialsNotAllowed);
        }
        if url.fragment().is_some() {
            return Err(CrawlUrlError::FragmentNotAllowed);
        }

        let Host::Domain(host) = url.host().ok_or(CrawlUrlError::MissingHost)? else {
            return Err(CrawlUrlError::IpLiteralNotAllowed);
        };
        if host.eq_ignore_ascii_case("localhost") {
            return Err(CrawlUrlError::LocalhostNotAllowed);
        }

        Ok(Self(url))
    }
}

impl std::fmt::Display for CrawlUrl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CrawlUrlError {
    #[error("crawl URL is empty")]
    Empty,
    #[error("crawl URL is invalid")]
    Invalid,
    #[error("crawl URL must use HTTP or HTTPS")]
    UnsupportedScheme,
    #[error("crawl URL is missing a host")]
    MissingHost,
    #[error("crawl URL credentials are not allowed")]
    CredentialsNotAllowed,
    #[error("crawl URL fragments are not allowed")]
    FragmentNotAllowed,
    #[error("crawl URL IP literals are not allowed")]
    IpLiteralNotAllowed,
    #[error("crawl URL localhost is not allowed")]
    LocalhostNotAllowed,
}

/// Resolves a hostname without coupling crawler policy to a DNS implementation.
#[async_trait]
pub trait DnsResolver: Send + Sync {
    async fn resolve(&self, host: &str) -> Result<Vec<SocketAddr>, DnsResolverError>;
}

/// The production DNS resolver used by the crawler.
#[derive(Debug, Default)]
pub struct SystemDnsResolver;

#[async_trait]
impl DnsResolver for SystemDnsResolver {
    async fn resolve(&self, host: &str) -> Result<Vec<SocketAddr>, DnsResolverError> {
        let addresses = tokio::net::lookup_host((host, 0))
            .await
            .map_err(|source| DnsResolverError::Lookup {
                host: host.to_owned(),
                source,
            })?
            .collect::<Vec<_>>();

        if addresses.is_empty() {
            return Err(DnsResolverError::NoAddresses {
                host: host.to_owned(),
            });
        }

        Ok(addresses)
    }
}

#[derive(Debug, Error)]
pub enum DnsResolverError {
    #[error("DNS lookup failed for {host}")]
    Lookup {
        host: String,
        #[source]
        source: std::io::Error,
    },
    #[error("DNS lookup returned no addresses for {host}")]
    NoAddresses { host: String },
}

#[derive(Debug, Error)]
pub enum DnsSafetyError {
    #[error(transparent)]
    Resolution(#[from] DnsResolverError),
    #[error("DNS resolution timed out")]
    Timeout,
    #[error("DNS resolution for {host} included blocked address {address}")]
    BlockedAddress { host: String, address: IpAddr },
}

/// Returns true only for addresses that are safe as public crawl destinations.
pub fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address.octets()),
        IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return is_public_address(IpAddr::V4(mapped));
            }

            let octets = address.octets();
            !address.is_loopback()
                && !address.is_unspecified()
                && !address.is_multicast()
                && !(octets[0] & 0xfe == 0xfc) // Unique local address: fc00::/7.
                && !(octets[0] == 0xfe && octets[1] & 0xc0 == 0x80) // Link-local: fe80::/10.
                && !(octets[0] == 0x20
                    && octets[1] == 0x01
                    && octets[2] == 0x0d
                    && octets[3] == 0xb8) // Documentation: 2001:db8::/32.
        }
    }
}

fn is_public_ipv4(octets: [u8; 4]) -> bool {
    let [a, b, c, _] = octets;
    !matches!(
        (a, b, c),
        (0, _, _) // Unspecified and this-network: 0.0.0.0/8.
            | (10, _, _) // Private: 10.0.0.0/8.
            | (100, 64..=127, _) // Shared address space: 100.64.0.0/10.
            | (127, _, _) // Loopback: 127.0.0.0/8.
            | (169, 254, _) // Link-local, including EC2 metadata.
            | (172, 16..=31, _) // Private: 172.16.0.0/12.
            | (192, 0, _) // IETF protocol assignments.
            | (192, 2, _) // Documentation: 192.0.2.0/24.
            | (192, 168, _) // Private: 192.168.0.0/16.
            | (198, 18..=19, _) // Benchmarking: 198.18.0.0/15.
            | (198, 51, 100) // Documentation: 198.51.100.0/24.
            | (203, 0, 113) // Documentation: 203.0.113.0/24.
            | (224..=255, _, _) // Multicast and reserved.
    )
}

fn validate_addresses(host: &str, addresses: &[SocketAddr]) -> Result<(), DnsSafetyError> {
    for address in addresses {
        if !is_public_address(address.ip()) {
            return Err(DnsSafetyError::BlockedAddress {
                host: host.to_owned(),
                address: address.ip(),
            });
        }
    }
    Ok(())
}

#[derive(Clone)]
struct ValidatingDnsResolver {
    inner: Arc<dyn DnsResolver>,
}

impl ValidatingDnsResolver {
    fn new(inner: Arc<dyn DnsResolver>) -> Self {
        Self { inner }
    }

    async fn resolve_checked(&self, host: &str) -> Result<Vec<SocketAddr>, DnsSafetyError> {
        let addresses = self.inner.resolve(host).await?;
        validate_addresses(host, &addresses)?;
        Ok(addresses)
    }
}

impl reqwest::dns::Resolve for ValidatingDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let resolver = self.clone();
        let host = name.as_str().to_owned();

        Box::pin(async move {
            let addresses = resolver.resolve_checked(&host).await.map_err(|error| {
                Box::new(std::io::Error::other(error.to_string()))
                    as Box<dyn std::error::Error + Send + Sync>
            })?;
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// A transport response after bounded body acquisition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn get(
        &self,
        target: &CrawlUrl,
        max_response_bytes: u64,
        cancellation: &CancellationToken,
    ) -> Result<HttpResponse, HttpTransportError>;
}

/// Reqwest transport configured for direct, manually-followed acquisition.
pub struct ReqwestHttpTransport {
    client: Client,
}

impl ReqwestHttpTransport {
    fn new(
        config: &CrawlerConfig,
        resolver: Arc<ValidatingDnsResolver>,
    ) -> Result<Self, CrawlerBuildError> {
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .user_agent(format!("{} (+{})", config.user_agent, config.contact))
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .timeout(config.total_timeout)
            .dns_resolver(resolver)
            .build()
            .map_err(CrawlerBuildError::HttpClient)?;

        Ok(Self { client })
    }
}

#[async_trait]
impl HttpTransport for ReqwestHttpTransport {
    async fn get(
        &self,
        target: &CrawlUrl,
        max_response_bytes: u64,
        cancellation: &CancellationToken,
    ) -> Result<HttpResponse, HttpTransportError> {
        let request = self.client.get(target.as_url().clone()).send();
        let mut response = tokio::select! {
            _ = cancellation.cancelled() => return Err(HttpTransportError::Cancelled),
            response = request => response.map_err(HttpTransportError::Request)?,
        };

        if let Some(content_length) = response.content_length()
            && content_length > max_response_bytes
        {
            return Err(HttpTransportError::ResponseTooLarge {
                limit: max_response_bytes,
                observed: content_length,
            });
        }

        let status = response.status().as_u16();
        let headers = safe_headers(response.headers());
        let mut body = Vec::new();
        while let Some(chunk) = tokio::select! {
            _ = cancellation.cancelled() => return Err(HttpTransportError::Cancelled),
            chunk = response.chunk() => chunk.map_err(HttpTransportError::Request)?,
        } {
            let observed = (body.len() as u64).saturating_add(chunk.len() as u64);
            if observed > max_response_bytes {
                return Err(HttpTransportError::ResponseTooLarge {
                    limit: max_response_bytes,
                    observed,
                });
            }
            body.extend_from_slice(&chunk);
        }

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn safe_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let name = name.as_str().to_ascii_lowercase();
            if is_sensitive_header(&name) {
                return None;
            }
            value.to_str().ok().map(|value| (name, value.to_owned()))
        })
        .collect()
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

#[derive(Debug, Error)]
pub enum HttpTransportError {
    #[error("HTTP request was cancelled")]
    Cancelled,
    #[error("HTTP request failed")]
    Request(#[source] reqwest::Error),
    #[error("response body exceeded the {limit}-byte limit ({observed} bytes)")]
    ResponseTooLarge { limit: u64, observed: u64 },
    #[error("HTTP transport failed: {message}")]
    Failed { message: String },
}

/// The result of a policy-compliant crawl acquisition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlResponse {
    pub final_url: CrawlUrl,
    /// Public addresses validated immediately before acquiring the final response.
    pub resolved_addresses: Vec<IpAddr>,
    pub redirects: Vec<CrawlRedirect>,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub dns: Capture<DnsCapture>,
    pub tls: Capture<TlsCapture>,
}

/// An optional capture result that remains explicit when a successful crawl could not obtain it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Capture<T> {
    Captured(T),
    Unavailable(CaptureUnavailableReason),
}

/// The acquisition-level reason an optional fact was unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureUnavailableReason {
    NotCaptured,
    NotApplicable,
    Timeout,
    ConnectionFailed,
    Invalid,
}

/// Public DNS inputs obtained from the validated final response target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsCapture {
    pub queried_name: String,
    pub addresses: Vec<IpAddr>,
}

/// Whether the TLS connection's certificate chain was validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsValidation {
    Verified,
    Failed,
}

/// Sanitized TLS inputs obtained without retaining certificate bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlsCapture {
    pub validation: TlsValidation,
    pub protocol: Option<String>,
    pub cipher_suite: Option<String>,
    pub certificate_subject: Option<String>,
    pub certificate_issuer: Option<String>,
    pub subject_alternative_names: Vec<String>,
    pub certificate_not_before: Option<OffsetDateTime>,
    pub certificate_not_after: Option<OffsetDateTime>,
}

#[derive(Debug, Error)]
enum TlsProbeError {
    #[error("TLS connection failed")]
    Connection,
    #[error("TLS certificate could not be parsed")]
    InvalidCertificate,
}

async fn tls_handshake(
    host: &str,
    address: SocketAddr,
    verify_certificate: bool,
) -> Result<TlsCapture, TlsProbeError> {
    let stream = TcpStream::connect(address)
        .await
        .map_err(|_| TlsProbeError::Connection)?;
    let server_name =
        ServerName::try_from(host.to_owned()).map_err(|_| TlsProbeError::Connection)?;
    let connector = TlsConnector::from(Arc::new(tls_client_config(verify_certificate)));
    let stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|_| TlsProbeError::Connection)?;
    let (_, connection) = stream.get_ref();
    let certificate = connection
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .ok_or(TlsProbeError::InvalidCertificate)?;
    let mut capture = certificate_capture(certificate)?;
    capture.validation = if verify_certificate {
        TlsValidation::Verified
    } else {
        TlsValidation::Failed
    };
    capture.protocol = connection.protocol_version().map(protocol_name);
    capture.cipher_suite = connection
        .negotiated_cipher_suite()
        .map(|suite| format!("{:?}", suite.suite()));
    Ok(capture)
}

fn tls_client_config(verify_certificate: bool) -> ClientConfig {
    if verify_certificate {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    } else {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerifier))
            .with_no_client_auth()
    }
}

#[derive(Debug)]
struct NoCertificateVerifier;

impl ServerCertVerifier for NoCertificateVerifier {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn certificate_capture(certificate: &CertificateDer<'_>) -> Result<TlsCapture, TlsProbeError> {
    let (_, certificate) = parse_x509_certificate(certificate.as_ref())
        .map_err(|_| TlsProbeError::InvalidCertificate)?;
    let subject = certificate
        .subject()
        .iter_common_name()
        .next()
        .and_then(|name| name.as_str().ok())
        .and_then(sanitized_hostname);
    let issuer = certificate
        .issuer()
        .iter_organization()
        .next()
        .or_else(|| certificate.issuer().iter_common_name().next())
        .and_then(|name| name.as_str().ok())
        .and_then(sanitized_certificate_label);
    let subject_alternative_names = certificate
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|extension| {
            extension
                .value
                .general_names
                .iter()
                .filter_map(|name| match name {
                    GeneralName::DNSName(value) => sanitized_hostname(value),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(TlsCapture {
        validation: TlsValidation::Failed,
        protocol: None,
        cipher_suite: None,
        certificate_subject: subject,
        certificate_issuer: issuer,
        subject_alternative_names,
        certificate_not_before: Some(certificate.validity().not_before.to_datetime()),
        certificate_not_after: Some(certificate.validity().not_after.to_datetime()),
    })
}

fn protocol_name(version: rustls::ProtocolVersion) -> String {
    match version {
        rustls::ProtocolVersion::TLSv1_2 => "TLS 1.2".to_owned(),
        rustls::ProtocolVersion::TLSv1_3 => "TLS 1.3".to_owned(),
        other => format!("{other:?}"),
    }
}

fn sanitized_hostname(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    (!value.is_empty()
        && value.len() <= 253
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'*')))
    .then_some(value)
}

fn sanitized_certificate_label(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= 256
        && !value.contains('@')
        && value
            .chars()
            .all(|character| character.is_ascii_graphic() || character == ' '))
    .then(|| value.to_owned())
}

/// Returns a UTF-8 HTML body suitable for persistence, with obvious credential-bearing
/// form and metadata values redacted. Other response bodies deliberately remain unavailable
/// to raw artifact storage until a safe persistence policy is defined for them.
pub fn sanitized_html_for_storage(response: &CrawlResponse) -> Option<Vec<u8>> {
    let content_type = response.headers.get("content-type")?;
    let media_type = content_type.split(';').next()?.trim();
    if !media_type.eq_ignore_ascii_case("text/html")
        && !media_type.eq_ignore_ascii_case("application/xhtml+xml")
    {
        return None;
    }
    let html = std::str::from_utf8(&response.body).ok()?;
    Some(redact_html_persistence_values(html).into_bytes())
}

fn redact_html_persistence_values(html: &str) -> String {
    static TAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<(?P<tag>input|meta)\b[^>]*>")
            .expect("raw artifact redaction tag pattern is valid")
    });
    TAG.replace_all(html, |captures: &regex::Captures<'_>| {
        let tag = captures
            .name("tag")
            .map(|value| value.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let original = captures.get(0).map_or("", |value| value.as_str());
        if tag_has_sensitive_identifier(original) {
            if tag == "input" {
                redact_tag_attribute(original, "value")
            } else {
                redact_tag_attribute(original, "content")
            }
        } else {
            original.to_owned()
        }
    })
    .into_owned()
}

fn tag_has_sensitive_identifier(tag: &str) -> bool {
    static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)(?P<name>[\w:-]+)\s*=\s*(?:\"(?P<double>[^\"]*)\"|'(?P<single>[^']*)'|(?P<bare>[^\s\"'=<>`]+))"#,
        )
        .expect("raw artifact redaction attribute pattern is valid")
    });
    ATTRIBUTE.captures_iter(tag).any(|captures| {
        let name = captures
            .name("name")
            .map_or("", |value| value.as_str())
            .to_ascii_lowercase();
        if !matches!(
            name.as_str(),
            "name" | "id" | "autocomplete" | "property" | "http-equiv"
        ) {
            return false;
        }
        let value = captures
            .name("double")
            .or_else(|| captures.name("single"))
            .or_else(|| captures.name("bare"))
            .map_or("", |value| value.as_str())
            .to_ascii_lowercase();
        [
            "password",
            "passphrase",
            "token",
            "secret",
            "api-key",
            "apikey",
            "credential",
            "auth",
            "cookie",
            "session",
            "private-key",
            "email",
            "phone",
            "telephone",
            "address",
            "social-security",
            "ssn",
            "birth",
        ]
        .iter()
        .any(|needle| value.contains(needle))
    })
}

fn redact_tag_attribute(tag: &str, attribute: &str) -> String {
    let pattern = format!(
        r#"(?is)(?P<prefix>\b{}\s*=\s*)(?:\"[^\"]*\"|'[^']*'|[^\s\"'=<>`]+)"#,
        regex::escape(attribute)
    );
    let expression = match Regex::new(&pattern) {
        Ok(expression) => expression,
        Err(_) => return tag.to_owned(),
    };
    expression
        .replace_all(tag, |captures: &regex::Captures<'_>| {
            let prefix = captures.name("prefix").map_or("", |value| value.as_str());
            format!("{prefix}\"[redacted]\"")
        })
        .into_owned()
}

/// A validated redirect hop followed while acquiring a crawl response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlRedirect {
    pub from: CrawlUrl,
    pub to: CrawlUrl,
    pub status: u16,
}

/// Acquires public targets only after robots policy and network safety checks.
pub struct Crawler {
    config: CrawlerConfig,
    resolver: Arc<dyn DnsResolver>,
    transport: Arc<dyn HttpTransport>,
    tls_capture_enabled: bool,
}

impl Crawler {
    pub fn new(
        config: CrawlerConfig,
        resolver: Arc<dyn DnsResolver>,
        transport: Arc<dyn HttpTransport>,
    ) -> Result<Self, CrawlerBuildError> {
        config
            .validate()
            .map_err(CrawlerBuildError::InvalidConfig)?;
        Ok(Self {
            config,
            resolver,
            transport,
            tls_capture_enabled: false,
        })
    }

    pub fn with_system_resolver(config: CrawlerConfig) -> Result<Self, CrawlerBuildError> {
        config
            .validate()
            .map_err(CrawlerBuildError::InvalidConfig)?;
        let resolver: Arc<dyn DnsResolver> = Arc::new(SystemDnsResolver);
        let validating_resolver = Arc::new(ValidatingDnsResolver::new(resolver.clone()));
        let transport = Arc::new(ReqwestHttpTransport::new(&config, validating_resolver)?);
        let mut crawler = Self::new(config, resolver, transport)?;
        crawler.tls_capture_enabled = true;
        Ok(crawler)
    }

    pub fn config(&self) -> &CrawlerConfig {
        &self.config
    }

    /// Fetches robots.txt and then acquires the target when it is allowed.
    pub async fn fetch(
        &self,
        target: CrawlUrl,
        cancellation: &CancellationToken,
    ) -> Result<CrawlResponse, CrawlError> {
        if cancellation.is_cancelled() {
            return Err(CrawlError::Cancelled);
        }
        let robots = self.robots_policy(&target, cancellation).await?;
        if !robots.allows(&target) {
            return Err(CrawlError::RobotsDenied {
                target: target.to_string(),
            });
        }

        self.fetch_unchecked(target, cancellation, true).await
    }

    /// Retrieves the target's robots policy through the same guarded transport.
    pub async fn robots_policy(
        &self,
        target: &CrawlUrl,
        cancellation: &CancellationToken,
    ) -> Result<RobotsPolicy, CrawlError> {
        if cancellation.is_cancelled() {
            return Err(CrawlError::Cancelled);
        }
        let robots_url = target
            .robots_url()
            .map_err(|source| CrawlError::RobotsUnavailable {
                reason: source.to_string(),
            })?;
        let response = self
            .fetch_unchecked(robots_url, cancellation, false)
            .await
            .map_err(map_robots_fetch_error)?;

        if response.status == 404 {
            return Ok(RobotsPolicy::allow_all());
        }
        if !(200..300).contains(&response.status) {
            return Err(CrawlError::RobotsUnavailable {
                reason: format!("robots.txt returned HTTP {}", response.status),
            });
        }

        let content =
            std::str::from_utf8(&response.body).map_err(|_| CrawlError::RobotsUnavailable {
                reason: "robots.txt was not valid UTF-8".to_owned(),
            })?;
        RobotsPolicy::parse(&self.config.robots_user_agent, content).map_err(|error| {
            CrawlError::RobotsUnavailable {
                reason: error.to_string(),
            }
        })
    }

    async fn fetch_unchecked(
        &self,
        initial: CrawlUrl,
        cancellation: &CancellationToken,
        capture_optional_facts: bool,
    ) -> Result<CrawlResponse, CrawlError> {
        let mut target = initial;
        let mut redirects = 0_usize;
        let mut redirect_chain = Vec::new();

        loop {
            let resolved_addresses = self.validate_target(&target, cancellation).await?;
            let response = tokio::select! {
                _ = cancellation.cancelled() => return Err(CrawlError::Cancelled),
                response = self.transport.get(&target, self.config.max_response_bytes, cancellation) => {
                    response.map_err(map_transport_error)?
                }
            };

            let observed = response.body.len() as u64;
            if observed > self.config.max_response_bytes {
                return Err(CrawlError::ResponseTooLarge {
                    limit: self.config.max_response_bytes,
                    observed,
                });
            }

            if is_redirect(response.status) {
                if redirects >= self.config.max_redirects {
                    return Err(CrawlError::RedirectLimitExceeded {
                        limit: self.config.max_redirects,
                    });
                }
                let location = response
                    .header("location")
                    .ok_or(CrawlError::MissingRedirectLocation)?;
                let next_target = target
                    .redirect_target(location)
                    .map_err(|source| CrawlError::InvalidRedirectTarget { source })?;
                redirect_chain.push(CrawlRedirect {
                    from: target,
                    to: next_target.clone(),
                    status: response.status,
                });
                target = next_target;
                redirects = redirects.saturating_add(1);
                continue;
            }

            let dns = if capture_optional_facts {
                Capture::Captured(DnsCapture {
                    queried_name: target.host().to_owned(),
                    addresses: resolved_addresses.clone(),
                })
            } else {
                Capture::Unavailable(CaptureUnavailableReason::NotCaptured)
            };
            let tls = if capture_optional_facts {
                self.capture_tls(&target, &resolved_addresses, cancellation)
                    .await
            } else {
                Capture::Unavailable(CaptureUnavailableReason::NotCaptured)
            };
            return Ok(CrawlResponse {
                final_url: target.clone(),
                dns,
                tls,
                resolved_addresses,
                redirects: redirect_chain,
                status: response.status,
                headers: response.headers,
                body: response.body,
            });
        }
    }

    async fn validate_target(
        &self,
        target: &CrawlUrl,
        cancellation: &CancellationToken,
    ) -> Result<Vec<IpAddr>, CrawlError> {
        let addresses = tokio::select! {
            _ = cancellation.cancelled() => return Err(CrawlError::Cancelled),
            result = timeout(self.config.dns_timeout, self.resolver.resolve(target.host())) => {
                result.map_err(|_| CrawlError::Dns(DnsSafetyError::Timeout))?
                    .map_err(DnsSafetyError::from)?
            },
        };
        validate_addresses(target.host(), &addresses).map_err(CrawlError::Dns)?;
        let mut public_addresses = addresses
            .into_iter()
            .map(|address| address.ip())
            .collect::<Vec<_>>();
        public_addresses.sort();
        public_addresses.dedup();
        public_addresses.truncate(self.config.dns_max_addresses);
        Ok(public_addresses)
    }

    async fn capture_tls(
        &self,
        target: &CrawlUrl,
        addresses: &[IpAddr],
        cancellation: &CancellationToken,
    ) -> Capture<TlsCapture> {
        if !self.tls_capture_enabled {
            return Capture::Unavailable(CaptureUnavailableReason::NotCaptured);
        }
        if target.as_url().scheme() != "https" {
            return Capture::Unavailable(CaptureUnavailableReason::NotApplicable);
        }
        let candidates = addresses
            .iter()
            .copied()
            .take(self.config.tls_max_addresses)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Capture::Unavailable(CaptureUnavailableReason::Invalid);
        }
        let port = target.as_url().port_or_known_default().unwrap_or(443);
        let probe = async {
            let mut fallback = None;
            for address in candidates {
                if let Ok(capture) =
                    tls_handshake(target.host(), SocketAddr::new(address, port), true).await
                {
                    return Capture::Captured(capture);
                }
                if let Ok(capture) =
                    tls_handshake(target.host(), SocketAddr::new(address, port), false).await
                {
                    fallback.get_or_insert(capture);
                }
            }
            fallback.map_or_else(
                || Capture::Unavailable(CaptureUnavailableReason::ConnectionFailed),
                Capture::Captured,
            )
        };
        tokio::select! {
            _ = cancellation.cancelled() => Capture::Unavailable(CaptureUnavailableReason::ConnectionFailed),
            result = timeout(self.config.tls_timeout, probe) => result.unwrap_or(Capture::Unavailable(CaptureUnavailableReason::Timeout)),
        }
    }
}

fn is_redirect(status: u16) -> bool {
    (300..400).contains(&status)
}

fn map_robots_fetch_error(error: CrawlError) -> CrawlError {
    match error {
        CrawlError::Cancelled | CrawlError::Dns(DnsSafetyError::BlockedAddress { .. }) => error,
        other => CrawlError::RobotsUnavailable {
            reason: other.to_string(),
        },
    }
}

fn map_transport_error(error: HttpTransportError) -> CrawlError {
    match error {
        HttpTransportError::Cancelled => CrawlError::Cancelled,
        HttpTransportError::Request(error) if error.is_timeout() => CrawlError::Timeout,
        HttpTransportError::Request(error) => {
            CrawlError::Transport(HttpTransportError::Request(error))
        }
        HttpTransportError::ResponseTooLarge { limit, observed } => {
            CrawlError::ResponseTooLarge { limit, observed }
        }
        other => CrawlError::Transport(other),
    }
}

#[derive(Debug, Error)]
pub enum CrawlerBuildError {
    #[error(transparent)]
    InvalidConfig(#[from] CrawlerConfigError),
    #[error("could not initialise the crawler HTTP client")]
    HttpClient(#[source] reqwest::Error),
}

#[derive(Debug, Error)]
pub enum CrawlError {
    #[error("crawl was cancelled")]
    Cancelled,
    #[error("crawl request timed out")]
    Timeout,
    #[error(transparent)]
    Dns(#[from] DnsSafetyError),
    #[error(transparent)]
    Transport(HttpTransportError),
    #[error("response body exceeded the {limit}-byte limit ({observed} bytes)")]
    ResponseTooLarge { limit: u64, observed: u64 },
    #[error("redirect limit of {limit} exceeded")]
    RedirectLimitExceeded { limit: usize },
    #[error("redirect response did not include a valid Location header")]
    MissingRedirectLocation,
    #[error("redirect target is invalid")]
    InvalidRedirectTarget {
        #[source]
        source: CrawlUrlError,
    },
    #[error("robots policy denies {target}")]
    RobotsDenied { target: String },
    #[error("robots policy could not be safely obtained: {reason}")]
    RobotsUnavailable { reason: String },
}

impl CrawlError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Dns(DnsSafetyError::Resolution(_))
                | Self::Dns(DnsSafetyError::Timeout)
                | Self::Timeout
                | Self::Transport(HttpTransportError::Request(_))
                | Self::Transport(HttpTransportError::Failed { .. })
                | Self::RobotsUnavailable { .. }
        )
    }
}

/// The policy request a worker sends to its distributed politeness coordinator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolitenessRequest {
    pub domain: String,
    pub minimum_delay: Duration,
}

impl PolitenessRequest {
    pub fn for_target(target: &CrawlUrl, config: &CrawlerConfig) -> Self {
        Self {
            domain: target.host().to_owned(),
            minimum_delay: config.politeness_delay,
        }
    }
}

#[async_trait]
pub trait PolitenessGate: Send + Sync {
    async fn acquire(
        &self,
        request: &PolitenessRequest,
        cancellation: &CancellationToken,
    ) -> Result<(), PolitenessGateError>;
}

#[derive(Debug, Error)]
pub enum PolitenessGateError {
    #[error("politeness wait was cancelled")]
    Cancelled,
    #[error("politeness gate is unavailable: {reason}")]
    Unavailable { reason: String },
}

/// Parsed robots directives applicable to one user agent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RobotsPolicy {
    rules: Vec<RobotsRule>,
}

impl RobotsPolicy {
    pub fn allow_all() -> Self {
        Self::default()
    }

    pub fn parse(user_agent: &str, document: &str) -> Result<Self, RobotsParseError> {
        if user_agent.trim().is_empty() {
            return Err(RobotsParseError::EmptyUserAgent);
        }

        let groups = parse_robots_groups(document)?;
        let agent = user_agent.trim().to_ascii_lowercase();
        let specific = groups
            .iter()
            .filter(|group| group.agents.iter().any(|candidate| candidate == &agent))
            .collect::<Vec<_>>();
        let matching_groups = if specific.is_empty() {
            groups
                .iter()
                .filter(|group| group.agents.iter().any(|candidate| candidate == "*"))
                .collect::<Vec<_>>()
        } else {
            specific
        };

        Ok(Self {
            rules: matching_groups
                .into_iter()
                .flat_map(|group| group.rules.iter().cloned())
                .collect(),
        })
    }

    pub fn allows(&self, target: &CrawlUrl) -> bool {
        let mut path = target.as_url().path().to_owned();
        if let Some(query) = target.as_url().query() {
            path.push('?');
            path.push_str(query);
        }

        let mut winner: Option<(usize, bool)> = None;
        for rule in &self.rules {
            if robots_pattern_matches(&rule.pattern, &path) {
                let specificity = rule
                    .pattern
                    .chars()
                    .filter(|character| !matches!(character, '*' | '$'))
                    .count();
                if winner.is_none_or(|(best_specificity, best_allow)| {
                    specificity > best_specificity
                        || (specificity == best_specificity && rule.allow && !best_allow)
                }) {
                    winner = Some((specificity, rule.allow));
                }
            }
        }

        winner.is_none_or(|(_, allow)| allow)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RobotsRule {
    pattern: String,
    allow: bool,
}

#[derive(Clone, Debug, Default)]
struct RobotsGroup {
    agents: Vec<String>,
    rules: Vec<RobotsRule>,
}

fn parse_robots_groups(document: &str) -> Result<Vec<RobotsGroup>, RobotsParseError> {
    let mut groups = Vec::new();
    let mut current: Option<RobotsGroup> = None;

    for (line_number, raw_line) in document.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (field, value) = line
            .split_once(':')
            .ok_or(RobotsParseError::InvalidDirective {
                line: line_number + 1,
            })?;
        let field = field.trim().to_ascii_lowercase();
        let value = value.trim();

        match field.as_str() {
            "user-agent" => {
                if value.is_empty() {
                    return Err(RobotsParseError::EmptyAgent {
                        line: line_number + 1,
                    });
                }
                if current
                    .as_ref()
                    .is_some_and(|group| !group.rules.is_empty())
                    && let Some(group) = current.take()
                {
                    groups.push(group);
                }
                let group = current.get_or_insert_with(RobotsGroup::default);
                group.agents.push(value.to_ascii_lowercase());
            }
            "allow" | "disallow" => {
                let Some(group) = current.as_mut() else {
                    return Err(RobotsParseError::RuleBeforeAgent {
                        line: line_number + 1,
                    });
                };
                if !value.is_empty() {
                    group.rules.push(RobotsRule {
                        pattern: value.to_owned(),
                        allow: field == "allow",
                    });
                }
            }
            _ => {}
        }
    }

    if let Some(group) = current {
        groups.push(group);
    }
    Ok(groups)
}

fn robots_pattern_matches(pattern: &str, path: &str) -> bool {
    let anchored = pattern.ends_with('$');
    let pattern = pattern.strip_suffix('$').unwrap_or(pattern);
    let mut cursor = 0_usize;

    for (index, segment) in pattern.split('*').enumerate() {
        if segment.is_empty() {
            continue;
        }
        if index == 0 {
            if !path.starts_with(segment) {
                return false;
            }
            cursor = segment.len();
            continue;
        }
        let Some(offset) = path[cursor..].find(segment) else {
            return false;
        };
        cursor = cursor.saturating_add(offset).saturating_add(segment.len());
    }

    !anchored || pattern.ends_with('*') || cursor == path.len()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RobotsParseError {
    #[error("robots user agent must not be empty")]
    EmptyUserAgent,
    #[error("robots directive on line {line} is missing ':'")]
    InvalidDirective { line: usize },
    #[error("robots user-agent on line {line} is empty")]
    EmptyAgent { line: usize },
    #[error("robots allow/disallow rule on line {line} appears before a user-agent")]
    RuleBeforeAgent { line: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeDnsResolver {
        addresses: BTreeMap<String, Vec<SocketAddr>>,
    }

    #[async_trait]
    impl DnsResolver for FakeDnsResolver {
        async fn resolve(&self, host: &str) -> Result<Vec<SocketAddr>, DnsResolverError> {
            self.addresses
                .get(host)
                .cloned()
                .ok_or_else(|| DnsResolverError::NoAddresses {
                    host: host.to_owned(),
                })
        }
    }

    #[derive(Default)]
    struct FakeTransport {
        responses: BTreeMap<String, HttpResponse>,
        requests: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl HttpTransport for FakeTransport {
        async fn get(
            &self,
            target: &CrawlUrl,
            _: u64,
            _: &CancellationToken,
        ) -> Result<HttpResponse, HttpTransportError> {
            self.requests
                .lock()
                .expect("test request lock should not be poisoned")
                .push(target.to_string());
            self.responses
                .get(target.as_url().as_str())
                .cloned()
                .ok_or_else(|| HttpTransportError::Failed {
                    message: format!("no fake response for {target}"),
                })
        }
    }

    fn public_address() -> SocketAddr {
        SocketAddr::from(([93, 184, 216, 34], 0))
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: BTreeMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn crawler(
        addresses: BTreeMap<String, Vec<SocketAddr>>,
        responses: BTreeMap<String, HttpResponse>,
    ) -> (Crawler, Arc<FakeTransport>) {
        crawler_with_config(CrawlerConfig::default(), addresses, responses)
    }

    fn crawler_with_config(
        config: CrawlerConfig,
        addresses: BTreeMap<String, Vec<SocketAddr>>,
        responses: BTreeMap<String, HttpResponse>,
    ) -> (Crawler, Arc<FakeTransport>) {
        let transport = Arc::new(FakeTransport {
            responses,
            requests: Mutex::new(Vec::new()),
        });
        let crawler = Crawler::new(
            config,
            Arc::new(FakeDnsResolver { addresses }),
            transport.clone(),
        )
        .expect("test crawler configuration should be valid");
        (crawler, transport)
    }

    #[test]
    fn default_config_uses_conservative_bounds() {
        let config = CrawlerConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert_eq!(config.read_timeout, Duration::from_secs(15));
        assert_eq!(config.total_timeout, Duration::from_secs(30));
        assert_eq!(config.dns_timeout, Duration::from_secs(5));
        assert_eq!(config.dns_max_addresses, 16);
        assert_eq!(config.tls_timeout, Duration::from_secs(5));
        assert_eq!(config.tls_max_addresses, 4);
        assert_eq!(config.max_redirects, 10);
        assert_eq!(config.max_response_bytes, 10 * 1024 * 1024);
        assert_eq!(config.politeness_delay, Duration::from_secs(1));
    }

    #[test]
    fn production_crawler_initialises_with_the_safe_defaults() {
        Crawler::with_system_resolver(CrawlerConfig::default())
            .expect("default crawler configuration should initialise Reqwest safely");
    }

    #[test]
    fn target_url_fixture_accepts_public_domains_and_rejects_unsafe_forms() {
        let fixture = include_str!("../../../tests/fixtures/crawler/target-urls.csv");
        for line in fixture.lines().skip(1) {
            let mut fields = line.splitn(3, ',');
            let expected = fields.next().expect("fixture has expected value");
            let input = fields.next().expect("fixture has URL value");
            let result = CrawlUrl::parse(input);
            assert_eq!(
                result.is_ok(),
                expected == "accept",
                "fixture input: {input}"
            );
        }
    }

    #[test]
    fn address_policy_rejects_non_public_ranges() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "192.168.1.1",
            "224.0.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "ff02::1",
        ] {
            assert!(!is_public_address(
                address.parse().expect("valid fixture address")
            ));
        }
        assert!(is_public_address(
            "93.184.216.34".parse().expect("valid fixture address")
        ));
    }

    #[test]
    fn response_header_redaction_excludes_credentials_and_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert("server", HeaderValue::from_static("fixture"));
        headers.insert("authorization", HeaderValue::from_static("Bearer secret"));
        headers.insert("x-session-token", HeaderValue::from_static("secret-token"));
        headers.insert("x-api-key", HeaderValue::from_static("secret-key"));

        let redacted = safe_headers(&headers);

        assert_eq!(redacted.get("server").map(String::as_str), Some("fixture"));
        assert!(!redacted.contains_key("authorization"));
        assert!(!redacted.contains_key("x-session-token"));
        assert!(!redacted.contains_key("x-api-key"));
    }

    #[test]
    fn certificate_fact_sanitizers_reject_personal_or_malformed_values() {
        assert_eq!(
            sanitized_hostname("WWW.Example.Test."),
            Some("www.example.test".to_owned())
        );
        assert_eq!(sanitized_hostname("person@example.test"), None);
        assert_eq!(
            sanitized_certificate_label("Example Certificate Authority"),
            Some("Example Certificate Authority".to_owned())
        );
        assert_eq!(sanitized_certificate_label("operator@example.test"), None);
    }

    #[tokio::test]
    async fn fetch_accepts_a_public_target_and_404_robots_policy() {
        let target = CrawlUrl::parse("https://public.example/path").expect("valid target");
        let mut addresses = BTreeMap::new();
        addresses.insert("public.example".to_owned(), vec![public_address()]);
        let mut responses = BTreeMap::new();
        responses.insert(
            "https://public.example/robots.txt".to_owned(),
            response(404, ""),
        );
        responses.insert(target.to_string(), response(200, "public response"));
        let (crawler, _) = crawler(addresses, responses);

        let response = crawler
            .fetch(target, &CancellationToken::new())
            .await
            .expect("public target should be acquired");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"public response");
    }

    #[tokio::test]
    async fn fetch_rejects_unsafe_dns_before_a_request() {
        let target = CrawlUrl::parse("https://unsafe.example/").expect("valid target");
        let mut addresses = BTreeMap::new();
        addresses.insert(
            "unsafe.example".to_owned(),
            vec![public_address(), SocketAddr::from(([127, 0, 0, 1], 0))],
        );
        let (crawler, transport) = crawler(addresses, BTreeMap::new());

        let error = crawler
            .fetch(target, &CancellationToken::new())
            .await
            .expect_err("unsafe DNS address must be blocked");

        assert!(matches!(
            error,
            CrawlError::Dns(DnsSafetyError::BlockedAddress { .. })
        ));
        assert!(
            transport
                .requests
                .lock()
                .expect("test request lock should not be poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn redirects_are_revalidated_before_requesting_the_destination() {
        let target = CrawlUrl::parse("https://public.example/").expect("valid target");
        let mut addresses = BTreeMap::new();
        addresses.insert("public.example".to_owned(), vec![public_address()]);
        addresses.insert(
            "private.example".to_owned(),
            vec![SocketAddr::from(([192, 168, 1, 20], 0))],
        );
        let mut redirect = response(302, "");
        redirect.headers.insert(
            "location".to_owned(),
            "https://private.example/internal".to_owned(),
        );
        let mut responses = BTreeMap::new();
        responses.insert(
            "https://public.example/robots.txt".to_owned(),
            response(404, ""),
        );
        responses.insert(target.to_string(), redirect);
        let (crawler, transport) = crawler(addresses, responses);

        let error = crawler
            .fetch(target, &CancellationToken::new())
            .await
            .expect_err("unsafe redirect destination must be blocked");

        assert!(matches!(
            error,
            CrawlError::Dns(DnsSafetyError::BlockedAddress { .. })
        ));
        assert_eq!(
            transport
                .requests
                .lock()
                .expect("test request lock should not be poisoned")
                .as_slice(),
            [
                "https://public.example/robots.txt",
                "https://public.example/"
            ]
        );
    }

    #[tokio::test]
    async fn robots_denial_prevents_target_acquisition() {
        let target =
            CrawlUrl::parse("https://public.example/private/report").expect("valid target");
        let mut addresses = BTreeMap::new();
        addresses.insert("public.example".to_owned(), vec![public_address()]);
        let mut responses = BTreeMap::new();
        responses.insert(
            "https://public.example/robots.txt".to_owned(),
            response(
                200,
                include_str!("../../../tests/fixtures/crawler/robots-deny-private.txt"),
            ),
        );
        let (crawler, transport) = crawler(addresses, responses);

        let error = crawler
            .fetch(target, &CancellationToken::new())
            .await
            .expect_err("robots denial must prevent acquisition");

        assert!(matches!(error, CrawlError::RobotsDenied { .. }));
        assert_eq!(
            transport
                .requests
                .lock()
                .expect("test request lock should not be poisoned")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn oversized_response_is_rejected_after_robots_allows_the_target() {
        let target = CrawlUrl::parse("https://public.example/report").expect("valid target");
        let config = CrawlerConfig {
            max_response_bytes: 4,
            ..CrawlerConfig::default()
        };
        let mut addresses = BTreeMap::new();
        addresses.insert("public.example".to_owned(), vec![public_address()]);
        let mut responses = BTreeMap::new();
        responses.insert(
            "https://public.example/robots.txt".to_owned(),
            response(404, ""),
        );
        responses.insert(target.to_string(), response(200, "oversized"));
        let (crawler, _) = crawler_with_config(config, addresses, responses);

        let error = crawler
            .fetch(target, &CancellationToken::new())
            .await
            .expect_err("oversized body must be rejected");

        assert!(matches!(
            error,
            CrawlError::ResponseTooLarge { limit: 4, .. }
        ));
    }

    #[tokio::test]
    async fn redirect_limit_is_enforced() {
        let target = CrawlUrl::parse("https://public.example/").expect("valid target");
        let config = CrawlerConfig {
            max_redirects: 1,
            ..CrawlerConfig::default()
        };
        let mut addresses = BTreeMap::new();
        addresses.insert("public.example".to_owned(), vec![public_address()]);
        let mut first_redirect = response(302, "");
        first_redirect.headers.insert(
            "location".to_owned(),
            "https://public.example/next".to_owned(),
        );
        let mut second_redirect = response(302, "");
        second_redirect.headers.insert(
            "location".to_owned(),
            "https://public.example/final".to_owned(),
        );
        let mut responses = BTreeMap::new();
        responses.insert(
            "https://public.example/robots.txt".to_owned(),
            response(404, ""),
        );
        responses.insert(target.to_string(), first_redirect);
        responses.insert("https://public.example/next".to_owned(), second_redirect);
        let (crawler, _) = crawler_with_config(config, addresses, responses);

        let error = crawler
            .fetch(target, &CancellationToken::new())
            .await
            .expect_err("second redirect must exceed the one-hop limit");

        assert!(matches!(
            error,
            CrawlError::RedirectLimitExceeded { limit: 1 }
        ));
    }

    #[tokio::test]
    async fn cancelled_crawl_does_not_start_dns_or_http_work() {
        let target = CrawlUrl::parse("https://public.example/").expect("valid target");
        let (crawler, transport) = crawler(BTreeMap::new(), BTreeMap::new());
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let error = crawler
            .fetch(target, &cancellation)
            .await
            .expect_err("cancelled crawl must stop immediately");

        assert!(matches!(error, CrawlError::Cancelled));
        assert!(
            transport
                .requests
                .lock()
                .expect("test request lock should not be poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn malformed_robots_policy_fails_closed_and_is_retryable() {
        let target = CrawlUrl::parse("https://public.example/").expect("valid target");
        let mut addresses = BTreeMap::new();
        addresses.insert("public.example".to_owned(), vec![public_address()]);
        let mut responses = BTreeMap::new();
        responses.insert(
            "https://public.example/robots.txt".to_owned(),
            response(200, "Disallow: /private"),
        );
        let (crawler, transport) = crawler(addresses, responses);

        let error = crawler
            .fetch(target, &CancellationToken::new())
            .await
            .expect_err("malformed robots policy must block acquisition");

        assert!(matches!(error, CrawlError::RobotsUnavailable { .. }));
        assert!(error.is_retryable());
        assert_eq!(
            transport
                .requests
                .lock()
                .expect("test request lock should not be poisoned")
                .len(),
            1
        );
    }

    #[test]
    fn robots_policy_uses_specific_agent_and_longest_allow_precedence() {
        let document = include_str!("../../../tests/fixtures/crawler/robots-matching.txt");
        let policy = RobotsPolicy::parse("TechAtlas", document).expect("valid robots fixture");

        assert!(
            !policy.allows(&CrawlUrl::parse("https://example.com/private").expect("valid URL"))
        );
        assert!(
            policy
                .allows(&CrawlUrl::parse("https://example.com/private/public").expect("valid URL"))
        );
        assert!(!policy.allows(
            &CrawlUrl::parse("https://example.com/private/public/archive").expect("valid URL")
        ));
        assert!(policy.allows(&CrawlUrl::parse("https://example.com/same").expect("valid URL")));
        assert!(policy.allows(&CrawlUrl::parse("https://example.com/other").expect("valid URL")));
    }

    #[test]
    fn robots_parser_rejects_rules_before_an_agent() {
        assert_eq!(
            RobotsPolicy::parse("TechAtlas", "Disallow: /private"),
            Err(RobotsParseError::RuleBeforeAgent { line: 1 })
        );
    }

    #[test]
    fn persisted_html_redacts_sensitive_form_and_metadata_values() {
        let response = CrawlResponse {
            final_url: CrawlUrl::parse("https://public.example/").expect("valid URL"),
            resolved_addresses: Vec::new(),
            redirects: Vec::new(),
            status: 200,
            headers: BTreeMap::from([("content-type".to_owned(), "text/html; charset=utf-8".to_owned())]),
            body: b"<html><head><meta name=\"api-token\" content=\"do-not-store\"></head><body><input name=\"password\" value=\"do-not-store\"><input name=\"email\" value=\"person@example.test\"><script src=\"/app.js\"></script></body></html>".to_vec(),
            dns: Capture::Unavailable(CaptureUnavailableReason::ConnectionFailed),
            tls: Capture::Unavailable(CaptureUnavailableReason::NotApplicable),
        };

        let persisted = String::from_utf8(
            sanitized_html_for_storage(&response).expect("HTML should be persistable"),
        )
        .expect("fixture is UTF-8");
        assert!(!persisted.contains("do-not-store"));
        assert!(!persisted.contains("person@example.test"));
        assert!(persisted.contains("value=\"[redacted]\""));
        assert!(persisted.contains("content=\"[redacted]\""));
        assert!(persisted.contains("<script src=\"/app.js\"></script>"));
    }

    #[test]
    fn non_html_or_invalid_utf8_bodies_are_not_persistable() {
        let response = CrawlResponse {
            final_url: CrawlUrl::parse("https://public.example/").expect("valid URL"),
            resolved_addresses: Vec::new(),
            redirects: Vec::new(),
            status: 200,
            headers: BTreeMap::from([("content-type".to_owned(), "application/json".to_owned())]),
            body: b"{}".to_vec(),
            dns: Capture::Unavailable(CaptureUnavailableReason::ConnectionFailed),
            tls: Capture::Unavailable(CaptureUnavailableReason::NotApplicable),
        };
        assert!(sanitized_html_for_storage(&response).is_none());
    }
}
