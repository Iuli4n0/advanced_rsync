use super::{DirMetadata, FileMetadata, Location};
use crate::errors::SyncError;
use sha2::{Digest, Sha256};
use std::any::Any;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

pub struct FolderLocation {
    pub path: PathBuf,
}

impl FolderLocation {
    pub fn new(path: &str) -> Self {
        FolderLocation {
            path: PathBuf::from(path),
        }
    }
}

fn calculate_file_hash(path: &std::path::Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    hasher.update(buffer);
    Some(hex::encode(hasher.finalize()))
}

impl Location for FolderLocation {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn list_files(&self) -> Result<Vec<FileMetadata>, SyncError> {
        let mut results = Vec::new();

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let relative_path = entry
                .path()
                .strip_prefix(&self.path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| entry.path().to_string_lossy().to_string());

            let hash = calculate_file_hash(&entry.path());

            let modified_time = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let adjusted_time = modified_time
                .checked_add(Duration::from_secs(2 * 3600))
                .unwrap_or(modified_time);

            results.push(FileMetadata {
                path: relative_path,
                modified: adjusted_time,
                //size: metadata.len(),
                hash,
            });
        }

        Ok(results)
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let full_path = self.path.join(path);
        let data = std::fs::read(&full_path)?;
        Ok(data)
    }

    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), SyncError> {
        let full_path = self.path.join(path);

        // Cream directorul parinte daca nu exista
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(full_path, data)?;
        Ok(())
    }

    fn delete_file(&self, path: &str) -> Result<(), SyncError> {
        let full_path = self.path.join(path);
        if full_path.exists() {
            std::fs::remove_file(full_path)?;
        }
        Ok(())
    }
    fn list_files_recursive(&self) -> Result<Vec<FileMetadata>, SyncError> {
        let mut results = Vec::new();

        for entry in WalkDir::new(&self.path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let full_path = entry.path();
                let metadata = std::fs::metadata(full_path)?;
                let relative_path = full_path
                    .strip_prefix(&self.path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| full_path.to_string_lossy().to_string());

                let hash = calculate_file_hash(full_path);

                let modified_time = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let adjusted_time = modified_time
                    .checked_add(Duration::from_secs(2 * 3600))
                    .unwrap_or(modified_time);

                results.push(FileMetadata {
                    path: relative_path,
                    modified: adjusted_time,
                    hash,
                });
            }
        }

        Ok(results)
    }

    fn list_dirs_recursive(&self) -> Result<Vec<DirMetadata>, SyncError> {
        let mut results = Vec::new();
        for entry in WalkDir::new(&self.path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_dir() {
                let full_path = entry.path();
                // Calea relativa
                let relative_path = full_path
                    .strip_prefix(&self.path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| full_path.to_string_lossy().to_string());

                let metadata = std::fs::metadata(full_path)?;
                let modified_time = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                results.push(DirMetadata {
                    path: relative_path,
                    modified: modified_time,
                });
            }
        }
        Ok(results)
    }

    fn create_dir(&self, path: &str) -> Result<(), SyncError> {
        let dir_path = self.path.join(path);
        std::fs::create_dir_all(dir_path)?;
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> Result<(), SyncError> {
        let dir_path = self.path.join(path);
        if dir_path.is_dir() {
            std::fs::remove_dir_all(&dir_path)?;
        }
        Ok(())
    }
}
