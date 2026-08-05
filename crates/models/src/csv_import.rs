use crate::{CanonicalDomain, DomainCreationDefaults};
use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvDomainImport {
    rows: Vec<CsvImportRowInput>,
}

impl CsvDomainImport {
    pub fn parse(input: &[u8]) -> Result<Self, CsvImportParseError> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(false)
            .from_reader(input);
        let headers = reader
            .headers()
            .map_err(|_| CsvImportParseError::MalformedCsv)?;

        if headers.is_empty() {
            return Err(CsvImportParseError::MissingDomainHeader);
        }
        if headers.len() != 1 {
            return Err(CsvImportParseError::UnsupportedColumns);
        }
        let Some(header) = headers.get(0) else {
            return Err(CsvImportParseError::MissingDomainHeader);
        };
        let header = header.trim().trim_start_matches('\u{feff}');
        if !header.eq_ignore_ascii_case("domain") {
            return Err(CsvImportParseError::MissingDomainHeader);
        }
        if contains_blank_data_line(input) {
            return Err(CsvImportParseError::MalformedCsv);
        }

        let mut rows = Vec::new();
        for (index, record) in reader.records().enumerate() {
            let record = record.map_err(|_| CsvImportParseError::MalformedCsv)?;
            let Some(domain) = record.get(0) else {
                return Err(CsvImportParseError::MalformedCsv);
            };
            let row_number = u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or(CsvImportParseError::TooManyRows)?;
            rows.push(CsvImportRowInput {
                row_number,
                domain: domain.to_owned(),
            });
        }

        Ok(Self { rows })
    }

    pub fn rows(&self) -> &[CsvImportRowInput] {
        &self.rows
    }
}

fn contains_blank_data_line(input: &[u8]) -> bool {
    let mut lines = input.split_inclusive(|byte| *byte == b'\n');
    let _ = lines.next();

    lines.any(|line| {
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        line.is_empty()
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvImportRowInput {
    row_number: u32,
    domain: String,
}

impl CsvImportRowInput {
    pub fn row_number(&self) -> u32 {
        self.row_number
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvImportCommand {
    source_name: String,
    initiated_by: String,
    document: CsvDomainImport,
}

impl CsvImportCommand {
    pub fn new(
        source_name: &str,
        initiated_by: &str,
        document: CsvDomainImport,
    ) -> Result<Self, CsvImportCommandError> {
        let source_name = source_name.trim();
        if source_name.is_empty() {
            return Err(CsvImportCommandError::EmptySourceName);
        }
        let initiated_by = initiated_by.trim();
        if initiated_by.is_empty() {
            return Err(CsvImportCommandError::EmptyInitiator);
        }

        Ok(Self {
            source_name: source_name.to_owned(),
            initiated_by: initiated_by.to_owned(),
            document,
        })
    }

    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    pub fn initiated_by(&self) -> &str {
        &self.initiated_by
    }

    pub fn document(&self) -> &CsvDomainImport {
        &self.document
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CsvImportResult {
    pub import_id: Uuid,
    pub source_name: String,
    pub accepted_row_count: u32,
    pub duplicate_row_count: u32,
    pub rejected_row_count: u32,
    pub rows: Vec<CsvImportRowResult>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CsvImportRowResult {
    pub row_number: u32,
    pub input_domain: String,
    pub canonical_domain: Option<String>,
    pub status: CsvImportRowStatus,
    pub domain_id: Option<Uuid>,
    pub error: Option<CsvImportRowError>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CsvImportRowStatus {
    Accepted,
    Duplicate,
    Rejected,
}

impl CsvImportRowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Duplicate => "duplicate",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CsvImportRowError {
    EmptyDomain,
    InvalidDomain,
}

impl CsvImportRowError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmptyDomain => "empty_domain",
            Self::InvalidDomain => "invalid_domain",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::EmptyDomain => "domain input is empty",
            Self::InvalidDomain => "domain input is invalid",
        }
    }
}

#[async_trait]
pub trait CsvImportRepository: Send + Sync {
    async fn import_csv(
        &self,
        command: CsvImportCommand,
        defaults: DomainCreationDefaults,
    ) -> Result<CsvImportResult, CsvImportRepositoryError>;
}

pub struct CsvImportService<R> {
    repository: R,
    defaults: DomainCreationDefaults,
}

impl<R> CsvImportService<R>
where
    R: CsvImportRepository,
{
    pub fn new(repository: R, defaults: DomainCreationDefaults) -> Self {
        Self {
            repository,
            defaults,
        }
    }

    pub fn with_default_policy(repository: R) -> Self {
        Self::new(repository, DomainCreationDefaults::default())
    }

    pub async fn import(
        &self,
        source_name: &str,
        initiated_by: &str,
        document: CsvDomainImport,
    ) -> Result<CsvImportResult, CsvImportServiceError> {
        let command = CsvImportCommand::new(source_name, initiated_by, document)?;

        self.repository
            .import_csv(command, self.defaults)
            .await
            .map_err(Into::into)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CsvImportParseError {
    #[error("CSV must have a single domain header")]
    MissingDomainHeader,
    #[error("CSV contains unsupported columns")]
    UnsupportedColumns,
    #[error("CSV document is malformed")]
    MalformedCsv,
    #[error("CSV contains too many rows")]
    TooManyRows,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CsvImportCommandError {
    #[error("CSV source name must not be empty")]
    EmptySourceName,
    #[error("CSV import initiator must not be empty")]
    EmptyInitiator,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CsvImportRepositoryError {
    #[error("CSV import repository is unavailable")]
    Unavailable,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CsvImportServiceError {
    #[error(transparent)]
    InvalidCommand(#[from] CsvImportCommandError),
    #[error("CSV import service is unavailable")]
    Unavailable,
}

impl From<CsvImportRepositoryError> for CsvImportServiceError {
    fn from(_: CsvImportRepositoryError) -> Self {
        Self::Unavailable
    }
}

pub fn classify_domain_error(error: &crate::CanonicalDomainError) -> CsvImportRowError {
    match error {
        crate::CanonicalDomainError::Empty => CsvImportRowError::EmptyDomain,
        _ => CsvImportRowError::InvalidDomain,
    }
}

pub fn parse_row_domain(input: &str) -> Result<CanonicalDomain, CsvImportRowError> {
    CanonicalDomain::parse(input).map_err(|error| classify_domain_error(&error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_domain_only_csv_with_one_based_data_rows() {
        let document =
            CsvDomainImport::parse(b"domain\nExample.COM\ninvalid\n").expect("CSV should parse");

        assert_eq!(document.rows().len(), 2);
        assert_eq!(document.rows()[0].row_number(), 1);
        assert_eq!(document.rows()[0].domain(), "Example.COM");
        assert_eq!(document.rows()[1].row_number(), 2);
    }

    #[test]
    fn rejects_missing_or_unsupported_headers() {
        assert_eq!(
            CsvDomainImport::parse(b""),
            Err(CsvImportParseError::MissingDomainHeader)
        );
        assert_eq!(
            CsvDomainImport::parse(b"url\nexample.com\n"),
            Err(CsvImportParseError::MissingDomainHeader)
        );
        assert_eq!(
            CsvDomainImport::parse(b"domain,priority\nexample.com,high\n"),
            Err(CsvImportParseError::UnsupportedColumns)
        );
    }

    #[test]
    fn does_not_silently_drop_empty_data_records() {
        assert_eq!(
            CsvDomainImport::parse(b"domain\n\n"),
            Err(CsvImportParseError::MalformedCsv)
        );
    }

    #[test]
    fn preserves_explicit_empty_fields_for_row_validation() {
        let document = CsvDomainImport::parse(b"domain\n\"\"\n")
            .expect("explicit empty field should be retained");

        assert_eq!(document.rows().len(), 1);
        assert_eq!(document.rows()[0].domain(), "");
    }

    #[test]
    fn classifies_empty_and_invalid_domain_rows_without_leaking_input() {
        assert_eq!(parse_row_domain(""), Err(CsvImportRowError::EmptyDomain));
        assert_eq!(
            parse_row_domain("ftp://example.com"),
            Err(CsvImportRowError::InvalidDomain)
        );
    }
}
