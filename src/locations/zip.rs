use super::{DirMetadata, FileMetadata, Location};
use crate::errors::SyncError;
use sha2::{Digest, Sha256};
use std::any::Any;
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

pub struct ZipLocation {
    pub path: PathBuf,
}

impl ZipLocation {
    pub fn new(path: &str) -> Self {
        ZipLocation {
            path: PathBuf::from(path),
        }
    }

    fn extract_directories<P: AsRef<Path>>(path: P) -> Vec<String> {
        let mut dirs = Vec::new();
        let mut current = path.as_ref();

        while let Some(parent) = current.parent() {
            if let Some(parent_str) = parent.to_str() {
                dirs.push(parent_str.to_string());
            }
            current = parent;
        }

        dirs
    }
}

fn calculate_hash_zip(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

impl Location for ZipLocation {
    fn as_any(&self) -> &dyn Any {
        self
    }

    //nnu mai folosim
    fn list_files(&self) -> Result<Vec<FileMetadata>, SyncError> {
        let file = File::open(&self.path)?;
        let mut archive = ZipArchive::new(file).map_err(|e| SyncError::Parse(e.to_string()))?;

        let mut results = Vec::new();
        for i in 0..archive.len() {
            if let Ok(mut file_) = archive.by_index(i) {
                let mut buffer = Vec::new();
                file_.read_to_end(&mut buffer)?;
                results.push(FileMetadata {
                    path: file_.name().to_string(),
                    modified: SystemTime::UNIX_EPOCH,
                    //size: file_.size(),
                    hash: Some(calculate_hash_zip(&buffer)),
                });
            }
        }
        Ok(results)
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let file = File::open(&self.path).map_err(SyncError::Io)?;
        let mut archive = ZipArchive::new(file).map_err(|e| SyncError::Parse(e.to_string()))?;

        let mut file_ = archive.by_name(path).map_err(|_| {
            SyncError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File '{}' not found in ZIP", path),
            ))
        })?;

        if file_.is_dir() {
            return Err(SyncError::Parse(format!("'{}' is a directory", path)));
        }

        let mut buf = Vec::new();
        file_.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn write_file(&self, path: &str, _data: &[u8]) -> Result<(), SyncError> {
        println!(
            "Attempted to write '{}' to ZIP (read-only). Ignoring.",
            path
        );

        Ok(())
    }

    fn delete_file(&self, path: &str) -> Result<(), SyncError> {
        println!(
            "Attempted to delete '{}' from ZIP (read-only). Ignoring.",
            path
        );

        Ok(())
    }

    fn list_files_recursive(&self) -> Result<Vec<FileMetadata>, SyncError> {
        let file = File::open(&self.path).map_err(SyncError::Io)?;
        let mut archive = ZipArchive::new(file).map_err(|e| SyncError::Parse(e.to_string()))?;

        let mut results = Vec::new();

        for i in 0..archive.len() {
            if let Ok(mut file_) = archive.by_index(i) {
                // Sarim peste directoare
                if file_.is_dir() {
                    continue;
                }

                let mut buffer = Vec::new();
                file_.read_to_end(&mut buffer)?;
                results.push(FileMetadata {
                    path: file_.name().to_string(),
                    modified: SystemTime::UNIX_EPOCH,
                    //size: file_.size(),
                    hash: Some(calculate_hash_zip(&buffer)),
                });
            }
        }
        Ok(results)
    }

    fn list_dirs_recursive(&self) -> Result<Vec<DirMetadata>, SyncError> {
        let file = File::open(&self.path).map_err(SyncError::Io)?;
        let mut archive = ZipArchive::new(file).map_err(|e| SyncError::Parse(e.to_string()))?;

        let mut dir_set: HashSet<String> = HashSet::new();

        for i in 0..archive.len() {
            if let Ok(file_) = archive.by_index(i) {
                let file_path = Path::new(file_.name());

                if file_.is_dir() {
                    let dir_str = file_.name().to_string();
                    if !dir_str.is_empty() {
                        dir_set.insert(dir_str);
                    }
                }

                // Extrage directoarele din calea fisierului
                for dir in Self::extract_directories(file_path) {
                    dir_set.insert(dir);
                }
            }
        }

        // Convertim set in vector de DirMetadata
        let dirs = dir_set
            .into_iter()
            .filter(|dir| !dir.is_empty())
            .map(|dir_path| DirMetadata {
                path: dir_path,
                modified: UNIX_EPOCH,
            })
            .collect();

        Ok(dirs)
    }
}
