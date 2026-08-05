#![forbid(unsafe_code)]

use clap::{Args, Parser, Subcommand};
use secrecy::SecretString;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};
use techatlas_database::{
    PostgresAdoptionProjectionRepository, PostgresCsvImportRepository, PostgresProbeError,
    PostgresSearchIndexRepository, RawArtifactRetentionError, connect_lazy_pool,
    expired_raw_artifact_locations, run_migrations,
};
use techatlas_models::{
    AdoptionProjectionOperations, CsvDomainImport, CsvImportParseError, CsvImportService,
    CsvImportServiceError,
};
use techatlas_search::{
    DomainSearchDocument, DomainSearchIndex, MeilisearchDomainIndex, SearchIndexError,
};
use thiserror::Error;
use url::Url;

const DEFAULT_DATABASE_CONNECTION_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Parser)]
#[command(name = "techatlas-cli", about = "TechAtlas maintenance commands")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ImportCsv(ImportCsvArgs),
    RebuildSearchIndex,
    RebuildAdoptionHistory(AdoptionHistoryArgs),
    MigrateDatabase,
    PruneExpiredArtifacts,
}

#[derive(Debug, Args)]
struct ImportCsvArgs {
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    source_name: String,
    #[arg(long)]
    initiated_by: String,
}

#[derive(Debug, Args)]
struct AdoptionHistoryArgs {
    /// Inclusive UTC date in YYYY-MM-DD format.
    #[arg(long)]
    from: String,
    /// Inclusive UTC date in YYYY-MM-DD format.
    #[arg(long)]
    to: String,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::ImportCsv(arguments) => import_csv(arguments).await,
        Command::RebuildSearchIndex => rebuild_search_index().await,
        Command::RebuildAdoptionHistory(arguments) => rebuild_adoption_history(arguments).await,
        Command::MigrateDatabase => migrate_database().await,
        Command::PruneExpiredArtifacts => prune_expired_artifacts().await,
    }
}

async fn rebuild_adoption_history(arguments: AdoptionHistoryArgs) -> Result<(), CliError> {
    let from = parse_date(&arguments.from)?;
    let to = parse_date(&arguments.to)?;
    if from > to {
        return Err(CliError::InvalidAdoptionHistoryRange);
    }
    let database_url = required_database_url()?;
    let pool = connect_lazy_pool(&database_url, database_connection_timeout()?)?;
    let repository = PostgresAdoptionProjectionRepository::new(pool);
    let result = repository
        .rebuild_adoption_history(from, to, time::OffsetDateTime::now_utc())
        .await?;
    println!(
        "{{\"from\":\"{from}\",\"to\":\"{to}\",\"rebuilt_days\":{},\"inserted_rows\":{}}}",
        result.rebuilt_days, result.inserted_rows
    );
    Ok(())
}

fn parse_date(value: &str) -> Result<time::Date, CliError> {
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
        .map_err(|_| CliError::InvalidAdoptionHistoryDate)?;
    time::Date::parse(value, &format).map_err(|_| CliError::InvalidAdoptionHistoryDate)
}

async fn migrate_database() -> Result<(), CliError> {
    let database_url = required_database_url()?;
    let pool = connect_lazy_pool(&database_url, database_connection_timeout()?)?;
    run_migrations(&pool)
        .await
        .map_err(|_| CliError::Migration)?;
    println!("{{\"migration_status\":\"complete\"}}");
    Ok(())
}

async fn prune_expired_artifacts() -> Result<(), CliError> {
    let database_url = required_database_url()?;
    let root = required_artifact_root()?;
    let pool = connect_lazy_pool(&database_url, database_connection_timeout()?)?;
    let locations = expired_raw_artifact_locations(&pool, time::OffsetDateTime::now_utc()).await?;
    let result = prune_artifact_files(&root, &locations)?;
    println!(
        "{{\"removed_artifacts\":{},\"already_absent_artifacts\":{}}}",
        result.removed, result.already_absent
    );
    Ok(())
}

async fn rebuild_search_index() -> Result<(), CliError> {
    let database_url = required_database_url()?;
    let pool = connect_lazy_pool(&database_url, database_connection_timeout()?)?;
    let meilisearch_url = required_meilisearch_url()?;
    let master_key = required_meilisearch_master_key()?;
    let index = MeilisearchDomainIndex::new(
        &meilisearch_url,
        &master_key,
        std::time::Duration::from_secs(10),
    )?;
    index.recreate().await?;
    let repository = PostgresSearchIndexRepository::new(pool);
    let projections = repository.all_projections().await?;
    let count = projections.len();
    for documents in projections
        .into_iter()
        .map(DomainSearchDocument::from)
        .collect::<Vec<_>>()
        .chunks(100)
    {
        index.upsert(documents.to_vec()).await?;
    }
    println!("{{\"indexed_domains\":{count}}}");
    Ok(())
}

async fn import_csv(arguments: ImportCsvArgs) -> Result<(), CliError> {
    let content = std::fs::read(arguments.file)?;
    let document = CsvDomainImport::parse(&content)?;
    let database_url = required_database_url()?;
    let pool = connect_lazy_pool(&database_url, database_connection_timeout()?)?;
    let service = CsvImportService::with_default_policy(PostgresCsvImportRepository::new(pool));
    let result = service
        .import(&arguments.source_name, &arguments.initiated_by, document)
        .await?;
    let output = serde_json::to_string(&result).map_err(|_| CliError::OutputSerialization)?;

    println!("{output}");
    Ok(())
}

fn required_database_url() -> Result<SecretString, CliError> {
    let value = env::var("DATABASE_URL").map_err(|_| CliError::MissingDatabaseUrl)?;
    if value.trim().is_empty() {
        return Err(CliError::MissingDatabaseUrl);
    }

    Ok(SecretString::from(value))
}

fn required_meilisearch_url() -> Result<Url, CliError> {
    let value = env::var("MEILISEARCH_URL").map_err(|_| CliError::MissingMeilisearchUrl)?;
    Url::parse(&value).map_err(|_| CliError::InvalidMeilisearchUrl)
}

fn required_meilisearch_master_key() -> Result<SecretString, CliError> {
    let value = env::var("MEILI_MASTER_KEY").map_err(|_| CliError::MissingMeilisearchMasterKey)?;
    if value.trim().is_empty() {
        return Err(CliError::MissingMeilisearchMasterKey);
    }
    Ok(SecretString::from(value))
}

fn required_artifact_root() -> Result<PathBuf, CliError> {
    let value = env::var("WORKER_ARTIFACT_ROOT").map_err(|_| CliError::MissingArtifactRoot)?;
    if value.trim().is_empty() {
        return Err(CliError::MissingArtifactRoot);
    }
    Ok(PathBuf::from(value))
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ArtifactPruneResult {
    removed: usize,
    already_absent: usize,
}

fn prune_artifact_files(
    root: &Path,
    locations: &[String],
) -> Result<ArtifactPruneResult, CliError> {
    let mut result = ArtifactPruneResult::default();
    for location in locations {
        let relative = validated_artifact_location(location)?;
        let path = root.join(relative);
        match fs::remove_file(path) {
            Ok(()) => result.removed += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                result.already_absent += 1
            }
            Err(_) => return Err(CliError::ArtifactPrune),
        }
    }
    Ok(result)
}

fn validated_artifact_location(location: &str) -> Result<&Path, CliError> {
    let mut components = Path::new(location).components();
    let (
        Some(std::path::Component::Normal(prefix)),
        Some(std::path::Component::Normal(shard)),
        Some(std::path::Component::Normal(file)),
        None,
    ) = (
        components.next(),
        components.next(),
        components.next(),
        components.next(),
    )
    else {
        return Err(CliError::InvalidArtifactLocation);
    };
    let prefix = prefix.to_str().ok_or(CliError::InvalidArtifactLocation)?;
    let shard = shard.to_str().ok_or(CliError::InvalidArtifactLocation)?;
    let file = file.to_str().ok_or(CliError::InvalidArtifactLocation)?;
    let checksum = file
        .strip_suffix(".zst")
        .ok_or(CliError::InvalidArtifactLocation)?;
    if prefix != "sha256"
        || shard.len() != 2
        || checksum.len() != 64
        || !shard.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !checksum.starts_with(shard)
    {
        return Err(CliError::InvalidArtifactLocation);
    }
    Ok(Path::new(location))
}

fn database_connection_timeout() -> Result<std::time::Duration, CliError> {
    parse_database_connection_timeout(env::var("CLI_DATABASE_CONNECT_TIMEOUT_MS").ok().as_deref())
}

fn parse_database_connection_timeout(value: Option<&str>) -> Result<std::time::Duration, CliError> {
    let milliseconds = value.filter(|value| !value.trim().is_empty()).map_or(
        Ok(DEFAULT_DATABASE_CONNECTION_TIMEOUT_MS),
        |value| {
            value
                .parse::<u64>()
                .map_err(|_| CliError::InvalidDatabaseTimeout)
        },
    )?;
    if milliseconds == 0 {
        return Err(CliError::InvalidDatabaseTimeout);
    }

    Ok(std::time::Duration::from_millis(milliseconds))
}

#[derive(Debug, Error)]
enum CliError {
    #[error("DATABASE_URL is required")]
    MissingDatabaseUrl,
    #[error("MEILISEARCH_URL is required")]
    MissingMeilisearchUrl,
    #[error("MEILI_MASTER_KEY is required")]
    MissingMeilisearchMasterKey,
    #[error("WORKER_ARTIFACT_ROOT is required")]
    MissingArtifactRoot,
    #[error("MEILISEARCH_URL is invalid")]
    InvalidMeilisearchUrl,
    #[error("unable to read CSV input")]
    ReadInput(#[from] std::io::Error),
    #[error(transparent)]
    InvalidCsv(#[from] CsvImportParseError),
    #[error(transparent)]
    InvalidDatabaseUrl(#[from] PostgresProbeError),
    #[error("CLI_DATABASE_CONNECT_TIMEOUT_MS must be a positive integer")]
    InvalidDatabaseTimeout,
    #[error("adoption history dates must use YYYY-MM-DD")]
    InvalidAdoptionHistoryDate,
    #[error("adoption history --from must be on or before --to")]
    InvalidAdoptionHistoryRange,
    #[error("database migration failed")]
    Migration,
    #[error("raw artifact location is invalid")]
    InvalidArtifactLocation,
    #[error("unable to remove expired raw artifact")]
    ArtifactPrune,
    #[error(transparent)]
    Import(#[from] CsvImportServiceError),
    #[error(transparent)]
    SearchRepository(#[from] techatlas_database::SearchIndexRepositoryError),
    #[error(transparent)]
    Search(#[from] SearchIndexError),
    #[error(transparent)]
    ArtifactRetention(#[from] RawArtifactRetentionError),
    #[error(transparent)]
    AdoptionProjection(#[from] techatlas_models::SchedulerRepositoryError),
    #[error("unable to serialize import result")]
    OutputSerialization,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn import_csv_requires_explicit_provenance_arguments() {
        assert!(Cli::try_parse_from(["techatlas-cli", "import-csv"]).is_err());

        assert!(
            Cli::try_parse_from([
                "techatlas-cli",
                "import-csv",
                "--file",
                "domains.csv",
                "--source-name",
                "partner-list",
                "--initiated-by",
                "ops@example.test",
            ])
            .is_ok()
        );
    }

    #[test]
    fn parses_the_explicit_search_rebuild_command() {
        assert!(Cli::try_parse_from(["techatlas-cli", "rebuild-search-index"]).is_ok());
    }

    #[test]
    fn parses_the_release_maintenance_commands() {
        assert!(Cli::try_parse_from(["techatlas-cli", "migrate-database"]).is_ok());
        assert!(Cli::try_parse_from(["techatlas-cli", "prune-expired-artifacts"]).is_ok());
        assert!(
            Cli::try_parse_from([
                "techatlas-cli",
                "rebuild-adoption-history",
                "--from",
                "2026-08-01",
                "--to",
                "2026-08-05",
            ])
            .is_ok()
        );
    }

    #[test]
    fn pruning_removes_only_valid_content_addressed_files() {
        let root =
            std::env::temp_dir().join(format!("techatlas-prune-test-{}", uuid::Uuid::new_v4()));
        let checksum = "ab".to_owned() + &"c".repeat(62);
        let location = format!("sha256/ab/{checksum}.zst");
        let file = root.join(&location);
        fs::create_dir_all(file.parent().expect("fixture has a parent"))
            .expect("fixture directory should be created");
        fs::write(&file, b"fixture").expect("fixture should be written");

        assert_eq!(
            prune_artifact_files(&root, &[location]).expect("valid fixture should prune"),
            ArtifactPruneResult {
                removed: 1,
                already_absent: 0
            }
        );
        assert!(!file.exists());
        assert!(matches!(
            prune_artifact_files(&root, &["../outside".to_owned()]),
            Err(CliError::InvalidArtifactLocation)
        ));
        fs::remove_dir_all(root).expect("fixture root should be removed");
    }

    #[test]
    fn validates_the_optional_database_connection_timeout() {
        assert_eq!(
            parse_database_connection_timeout(None).expect("default timeout should parse"),
            std::time::Duration::from_millis(5_000)
        );
        assert!(matches!(
            parse_database_connection_timeout(Some("0")),
            Err(CliError::InvalidDatabaseTimeout)
        ));
        assert!(matches!(
            parse_database_connection_timeout(Some("invalid")),
            Err(CliError::InvalidDatabaseTimeout)
        ));
    }

    #[test]
    fn validates_adoption_history_dates_and_ranges() {
        assert!(parse_date("2026-08-05").is_ok());
        assert!(matches!(
            parse_date("05-08-2026"),
            Err(CliError::InvalidAdoptionHistoryDate)
        ));
    }
}
