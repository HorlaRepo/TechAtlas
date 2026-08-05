#![forbid(unsafe_code)]

//! Compressed, integrity-verifiable storage for immutable crawl artifacts.

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use techatlas_models::{RawArtifactCompression, RawArtifactMetadata, RawArtifactMetadataError};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

const COMPRESSION_LEVEL: i32 = 3;

#[async_trait]
pub trait RawArtifactStore: Send + Sync {
    async fn store(
        &self,
        contents: Vec<u8>,
        retention_expires_at: OffsetDateTime,
    ) -> Result<RawArtifactMetadata, RawArtifactStoreError>;

    async fn retrieve(
        &self,
        artifact: &RawArtifactMetadata,
    ) -> Result<Vec<u8>, RawArtifactStoreError>;
}

#[derive(Clone, Debug)]
pub struct LocalRawArtifactStore {
    root: PathBuf,
}

impl LocalRawArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, RawArtifactStoreError> {
        let root = root.into();
        if root.as_os_str().is_empty() {
            return Err(RawArtifactStoreError::InvalidRoot);
        }
        Ok(Self { root })
    }

    fn location_for(checksum_sha256: &str) -> String {
        let shard = &checksum_sha256[..2];
        format!("sha256/{shard}/{checksum_sha256}.zst")
    }
}

#[async_trait]
impl RawArtifactStore for LocalRawArtifactStore {
    async fn store(
        &self,
        contents: Vec<u8>,
        retention_expires_at: OffsetDateTime,
    ) -> Result<RawArtifactMetadata, RawArtifactStoreError> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || store_blocking(root, contents, retention_expires_at))
            .await
            .map_err(|_| RawArtifactStoreError::Unavailable)?
    }

    async fn retrieve(
        &self,
        artifact: &RawArtifactMetadata,
    ) -> Result<Vec<u8>, RawArtifactStoreError> {
        let root = self.root.clone();
        let artifact = artifact.clone();
        tokio::task::spawn_blocking(move || retrieve_blocking(root, artifact))
            .await
            .map_err(|_| RawArtifactStoreError::Unavailable)?
    }
}

fn store_blocking(
    root: PathBuf,
    contents: Vec<u8>,
    retention_expires_at: OffsetDateTime,
) -> Result<RawArtifactMetadata, RawArtifactStoreError> {
    let checksum_sha256 = checksum(&contents);
    let location = LocalRawArtifactStore::location_for(&checksum_sha256);
    let path = root.join(&location);
    let compressed = zstd::stream::encode_all(contents.as_slice(), COMPRESSION_LEVEL)
        .map_err(|_| RawArtifactStoreError::Compression)?;
    let uncompressed_size_bytes =
        u64::try_from(contents.len()).map_err(|_| RawArtifactStoreError::Size)?;

    write_atomically(&path, &compressed)?;
    let stored_compressed = fs::read(&path).map_err(map_read_error)?;
    let stored_contents = zstd::stream::decode_all(stored_compressed.as_slice())
        .map_err(|_| RawArtifactStoreError::Integrity)?;
    if stored_contents != contents {
        return Err(RawArtifactStoreError::Integrity);
    }
    let compressed_size_bytes =
        u64::try_from(stored_compressed.len()).map_err(|_| RawArtifactStoreError::Size)?;
    RawArtifactMetadata::response_body(
        &location,
        &checksum_sha256,
        RawArtifactCompression::Zstd,
        uncompressed_size_bytes,
        compressed_size_bytes,
        retention_expires_at,
    )
    .map_err(map_metadata_error)
}

fn retrieve_blocking(
    root: PathBuf,
    artifact: RawArtifactMetadata,
) -> Result<Vec<u8>, RawArtifactStoreError> {
    if artifact.compression() != RawArtifactCompression::Zstd {
        return Err(RawArtifactStoreError::UnsupportedCompression);
    }
    let expected_location = LocalRawArtifactStore::location_for(artifact.checksum_sha256());
    if artifact.storage_location() != expected_location {
        return Err(RawArtifactStoreError::InvalidLocation);
    }
    let compressed = fs::read(root.join(expected_location)).map_err(map_read_error)?;
    if u64::try_from(compressed.len()).map_err(|_| RawArtifactStoreError::Size)?
        != artifact.compressed_size_bytes()
    {
        return Err(RawArtifactStoreError::Integrity);
    }
    let contents = zstd::stream::decode_all(compressed.as_slice())
        .map_err(|_| RawArtifactStoreError::Integrity)?;
    if u64::try_from(contents.len()).map_err(|_| RawArtifactStoreError::Size)?
        != artifact.uncompressed_size_bytes()
        || checksum(&contents) != artifact.checksum_sha256()
    {
        return Err(RawArtifactStoreError::Integrity);
    }
    Ok(contents)
}

fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), RawArtifactStoreError> {
    if path.exists() {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or(RawArtifactStoreError::InvalidLocation)?;
    fs::create_dir_all(parent).map_err(RawArtifactStoreError::Io)?;
    let temporary = parent.join(format!(".artifact-{}.tmp", Uuid::new_v4()));
    let write_result = (|| -> Result<(), io::Error> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    match write_result {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => Ok(()),
        Err(error) => Err(RawArtifactStoreError::Io(error)),
    }
}

fn checksum(contents: &[u8]) -> String {
    format!("{:x}", Sha256::digest(contents))
}

fn map_read_error(error: io::Error) -> RawArtifactStoreError {
    if error.kind() == io::ErrorKind::NotFound {
        RawArtifactStoreError::NotFound
    } else {
        RawArtifactStoreError::Io(error)
    }
}

fn map_metadata_error(_: RawArtifactMetadataError) -> RawArtifactStoreError {
    RawArtifactStoreError::Integrity
}

#[derive(Debug, Error)]
pub enum RawArtifactStoreError {
    #[error("raw artifact store root is invalid")]
    InvalidRoot,
    #[error("raw artifact location is invalid")]
    InvalidLocation,
    #[error("raw artifact was not found")]
    NotFound,
    #[error("raw artifact compression failed")]
    Compression,
    #[error("raw artifact integrity verification failed")]
    Integrity,
    #[error("raw artifact uses an unsupported compression format")]
    UnsupportedCompression,
    #[error("raw artifact size cannot be represented")]
    Size,
    #[error("raw artifact storage is unavailable")]
    Unavailable,
    #[error("raw artifact storage is unavailable")]
    Io(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root() -> PathBuf {
        std::env::temp_dir().join(format!("techatlas-storage-test-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn round_trips_and_verifies_a_compressed_artifact() {
        let root = temporary_root();
        let store = LocalRawArtifactStore::new(root.clone()).expect("temporary root is valid");
        let artifact = store
            .store(b"<html>fixture</html>".to_vec(), OffsetDateTime::now_utc())
            .await
            .expect("storage should succeed");

        assert!(artifact.storage_location().starts_with("sha256/"));
        assert_eq!(artifact.compression(), RawArtifactCompression::Zstd);
        assert_eq!(
            store
                .retrieve(&artifact)
                .await
                .expect("retrieval should succeed"),
            b"<html>fixture</html>"
        );
        fs::remove_dir_all(root).expect("temporary root should be removable");
    }

    #[tokio::test]
    async fn detects_tampered_compressed_content() {
        let root = temporary_root();
        let store = LocalRawArtifactStore::new(root.clone()).expect("temporary root is valid");
        let artifact = store
            .store(b"fixture".to_vec(), OffsetDateTime::now_utc())
            .await
            .expect("storage should succeed");
        fs::write(root.join(artifact.storage_location()), b"tampered")
            .expect("fixture object should be writable");

        assert!(matches!(
            store.retrieve(&artifact).await,
            Err(RawArtifactStoreError::Integrity)
        ));
        fs::remove_dir_all(root).expect("temporary root should be removable");
    }

    #[tokio::test]
    async fn duplicate_content_reuses_its_location() {
        let root = temporary_root();
        let store = LocalRawArtifactStore::new(root.clone()).expect("temporary root is valid");
        let first = store
            .store(b"fixture".to_vec(), OffsetDateTime::now_utc())
            .await
            .expect("first write should succeed");
        let second = store
            .store(b"fixture".to_vec(), OffsetDateTime::now_utc())
            .await
            .expect("second write should succeed");

        assert_eq!(first.storage_location(), second.storage_location());
        assert_eq!(first.checksum_sha256(), second.checksum_sha256());
        fs::remove_dir_all(root).expect("temporary root should be removable");
    }
}
