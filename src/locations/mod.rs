mod folder;
mod ftp;
mod zip;

use crate::errors::SyncError;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub path: String,
    pub modified: SystemTime,
    //pub size: u64,
    pub hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DirMetadata {
    pub path: String,
    pub modified: SystemTime,
}

pub use folder::FolderLocation;
pub use ftp::FtpLocation;
use std::any::Any;
pub use zip::ZipLocation;
pub trait Location: Any {
    fn as_any(&self) -> &dyn Any;
    fn list_files(&self) -> Result<Vec<FileMetadata>, SyncError>;
    fn read_file(&self, path: &str) -> Result<Vec<u8>, SyncError>;
    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), SyncError>;
    fn delete_file(&self, path: &str) -> Result<(), SyncError>;

    fn list_files_recursive(&self) -> Result<Vec<FileMetadata>, SyncError> {
        self.list_files()
    }
    fn create_dir(&self, _path: &str) -> Result<(), SyncError> {
        Ok(())
    }

    fn remove_dir(&self, _path: &str) -> Result<(), SyncError> {
        Ok(())
    }

    fn list_dirs_recursive(&self) -> Result<Vec<DirMetadata>, SyncError> {
        Ok(vec![])
    }
}

pub fn parse_location(loc_str: &str) -> Result<Box<dyn Location>, SyncError> {
    let parts: Vec<&str> = loc_str.splitn(2, ':').collect();
    if parts.len() < 2 {
        return Err(SyncError::Parse("Format invalid".to_string()));
    }

    let loc_type = parts[0];
    let loc_path = parts[1];

    match loc_type {
        "folder" => Ok(Box::new(FolderLocation::new(loc_path))),
        "zip" => Ok(Box::new(ZipLocation::new(loc_path))),
        "ftp" => {
            let ftp_str = loc_path;
            if let Some((cred, host_path)) = ftp_str.split_once('@') {
                if let Some((user, pass)) = cred.split_once(':') {
                    if let Some((host, remote_path)) = host_path.split_once('/') {
                        println!("{}", { remote_path });
                        return Ok(Box::new(FtpLocation::new(user, pass, host, remote_path)));
                    }
                }
            }
            Err(SyncError::Parse("Format ftp invalid".to_string()))
        }
        _ => Err(SyncError::Parse(format!("Tip necunoscut: {}", loc_type))),
    }
}
