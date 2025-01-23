#[derive(Debug)]
#[allow(dead_code)]
pub enum SyncError {
    Io(std::io::Error),
    Ftp(String),
    Parse(String),
}

impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        SyncError::Io(err)
    }
}
