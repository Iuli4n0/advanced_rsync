use crate::errors::SyncError;
use crate::locations::{DirMetadata, FileMetadata, Location};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use ftp::FtpStream;
use sha2::{Digest, Sha256};
use std::any::Any;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct FtpLocation {
    pub user: String,
    pub pass: String,
    pub host: String,
    pub path: String, // folder remote de unde facem sync
}

fn list_dirs_recursive_ftp(
    ftp_stream: &mut FtpStream,
    relative_path: &str, // Calea relativa pentru cwd
    full_path: &str,     // Calea completa pentru inregistrare
    results: &mut Vec<DirMetadata>,
) -> Result<(), SyncError> {
    println!("Listing directories in: '{}'", relative_path);

    if relative_path != "." && !relative_path.is_empty() {
        println!("Changing directory to: '{}'", relative_path);
        ftp_stream
            .cwd(relative_path)
            .map_err(|e| SyncError::Ftp(format!("Failed cwd({}): {}", relative_path, e)))?;

        let current_dir = ftp_stream
            .pwd()
            .map_err(|e| SyncError::Ftp(e.to_string()))?;
        println!("Current directory after cwd: '{}'", current_dir);
    }

    let entries = ftp_stream
        .list(None)
        .map_err(|e| SyncError::Ftp(format!("Failed list in {}: {}", relative_path, e)))?;

    for entry in entries {
        println!("Entry: '{}'", entry);
        if let Some((entry_name, is_dir, maybe_time)) = parse_list_entry_dir(&entry) {
            println!(
                "Parsed entry - Name: '{}', Is Dir: {}, Modified: {:?}",
                entry_name, is_dir, maybe_time
            );
            if is_dir {
                // Construim calea completa pentru inregistrare
                let child_full_path = if full_path == "." || full_path.is_empty() {
                    entry_name.clone()
                } else {
                    format!("{}/{}", full_path, entry_name)
                };

                // !!!Evitam adaugarea path-urilor goale sau "." sau ".."
                if child_full_path.is_empty() || child_full_path == "." || child_full_path == ".." {
                    println!("Skipping invalid directory path: '{}'", child_full_path);
                    continue;
                }

                let dir_modified = maybe_time.unwrap_or(UNIX_EPOCH);

                results.push(DirMetadata {
                    path: child_full_path.clone(),
                    modified: dir_modified,
                });

                // calea relativă entry_name si calea completă child_full_path
                list_dirs_recursive_ftp(ftp_stream, &entry_name, &child_full_path, results)?;
            }
        }
    }

    // ne intaorcem la directrul parinte
    if relative_path != "." && !relative_path.is_empty() {
        println!("Changing directory up from: '{}'", relative_path);
        ftp_stream
            .cdup()
            .map_err(|e| SyncError::Ftp(format!("Failed cdup from {}: {}", relative_path, e)))?;
    }

    Ok(())
}

impl FtpLocation {
    pub fn new(user: &str, pass: &str, host: &str, path: &str) -> Self {
        FtpLocation {
            user: user.to_string(),
            pass: pass.to_string(),
            host: host.to_string(),
            path: path.to_string(),
        }
    }

    fn connect(&self) -> Result<FtpStream, SyncError> {
        let mut ftp_stream =
            FtpStream::connect(&self.host).map_err(|e| SyncError::Ftp(e.to_string()))?;

        ftp_stream
            .login(&self.user, &self.pass)
            .map_err(|e| SyncError::Ftp(e.to_string()))?;

        // Intram in root
        if !self.path.is_empty() && self.path != "." {
            ftp_stream
                .cwd(&self.path)
                .map_err(|e| SyncError::Ftp(e.to_string()))?;
        }
        Ok(ftp_stream)
    }

    fn list_files_recursive_ftp(
        &self,
        ftp_stream: &mut FtpStream,
        relative_path: &str, // Calea relativa pentru cwd
        full_path: &str,     // Calea completa pentru inregistrare
        results: &mut Vec<FileMetadata>,
    ) -> Result<(), SyncError> {
        println!("Listing files in: '{}'", relative_path);

        if relative_path != "." && !relative_path.is_empty() {
            println!("Changing directory to: '{}'", relative_path);
            ftp_stream
                .cwd(relative_path)
                .map_err(|e| SyncError::Ftp(format!("Failed cwd({}): {}", relative_path, e)))?;

            let current_dir = ftp_stream
                .pwd()
                .map_err(|e| SyncError::Ftp(e.to_string()))?;
            println!("Current directory after cwd: '{}'", current_dir);
        }

        let entries = ftp_stream
            .list(None)
            .map_err(|e| SyncError::Ftp(format!("Failed list in {}: {}", relative_path, e)))?;

        for entry_line in entries {
            println!("Entry: '{}'", entry_line);
            if let Some((name, is_dir, maybe_time)) = parse_list_entry(&entry_line) {
                println!(
                    "Parsed entry - Name: '{}', Is Dir: {}, Modified: {:?}",
                    name, is_dir, maybe_time
                );
                if is_dir {
                    // Este subdirector -> recursie
                    let sub_dir = if full_path == "." || full_path.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{}", full_path, name)
                    };

                    if sub_dir.is_empty() || sub_dir == "." || sub_dir == ".." {
                        println!("Skipping invalid subdirectory path: '{}'", sub_dir);
                        continue;
                    }

                    // calea relativă name și calea completă sub_dir
                    self.list_files_recursive_ftp(ftp_stream, &name, &sub_dir, results)?;
                } else {
                    let full_file_path = if full_path == "." || full_path.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{}", full_path, name)
                    };

                    if full_file_path.is_empty() || full_file_path == "." || full_file_path == ".."
                    {
                        println!("Skipping invalid file path: '{}'", full_file_path);
                        continue;
                    }

                    let file_modified = maybe_time.unwrap_or(UNIX_EPOCH);
                    let mut file_hash = None;

                    if let Ok(data) = self.read_file(&full_file_path) {
                        file_hash = Some(calculate_hash(&data));
                    }

                    results.push(FileMetadata {
                        path: full_file_path.clone(),
                        modified: file_modified,
                        hash: file_hash,
                    });

                    println!("Added file: '{}'", full_file_path);
                }
            }
        }

        // Revenim la directorul parinte
        if relative_path != "." && !relative_path.is_empty() {
            println!("Changing directory up from: '{}'", relative_path);
            ftp_stream.cdup().map_err(|e| {
                SyncError::Ftp(format!("Failed cdup from {}: {}", relative_path, e))
            })?;
        }

        Ok(())
    }
}

//   //////////////////////////////////////IMPL Location pentru FtpLocation//////////////////////////////////////////
impl Location for FtpLocation {
    fn as_any(&self) -> &dyn Any {
        self
    }

    // nu il mai folosim
    fn list_files(&self) -> Result<Vec<FileMetadata>, SyncError> {
        let mut ftp_stream = self.connect()?;
        let entries = ftp_stream
            .list(None)
            .map_err(|e| SyncError::Ftp(format!("Failed list in root: {}", e)))?;

        let mut results = Vec::new();
        for entry_line in entries {
            if let Some((name, is_dir, maybe_time)) = parse_list_entry(&entry_line) {
                if !is_dir {
                    let file_modified = maybe_time.unwrap_or(UNIX_EPOCH);
                    let mut file_hash = None;
                    if let Ok(data) = self.read_file(&name) {
                        file_hash = Some(calculate_hash(&data));
                    }
                    results.push(FileMetadata {
                        path: name,
                        modified: file_modified,
                        hash: file_hash,
                    });
                }
            }
        }
        Ok(results)
    }

    // folosim metoda recursiva
    fn list_files_recursive(&self) -> Result<Vec<FileMetadata>, SyncError> {
        println!("Starting recursive file listing");
        let mut ftp_stream = self.connect()?;
        let mut results = Vec::new();

        self.list_files_recursive_ftp(&mut ftp_stream, ".", ".", &mut results)?;
        println!("Completed recursive file listing");
        Ok(results)
    }

    fn list_dirs_recursive(&self) -> Result<Vec<DirMetadata>, SyncError> {
        println!("Starting recursive directory listing");
        let mut ftp_stream = self.connect()?;
        let mut results = Vec::new();

        list_dirs_recursive_ftp(&mut ftp_stream, ".", ".", &mut results)?;
        println!("Completed recursive directory listing");
        Ok(results)
    }

    ////////////////////////////////////////// READ FILE //////////////////////////////////////////

    fn read_file(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let mut ftp_stream = self.connect()?;
        let (dir, filename) = split_path_dir_file(path);

        if !dir.is_empty() && dir != "." {
            ftp_stream
                .cwd(&dir)
                .map_err(|e| SyncError::Ftp(format!("Failed cwd({}): {}", dir, e)))?;
        }

        let data = ftp_stream
            .retr(filename, |reader| {
                let mut buffer = Vec::new();
                std::io::copy(reader, &mut buffer).map_err(ftp::FtpError::ConnectionError)?;
                Ok(buffer)
            })
            .map_err(|e| SyncError::Ftp(format!("Failed retr {}: {}", filename, e)))?;

        if !dir.is_empty() && dir != "." {
            let _ = ftp_stream.cdup();
        }
        Ok(data)
    }

    ////////////////////////////////////////// WRITE FILE (recursiv) //////////////////////////////////////////
    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), SyncError> {
        let mut ftp_stream = self.connect()?;

        // Desfacem path-ul recursiv
        let (dir, filename) = split_path_dir_file(path);

        // facem tot lantul de subdirectoare
        if !dir.is_empty() && dir != "." {
            let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
            for part in &parts {
                // ignoram daca deja exista
                match ftp_stream.mkdir(part) {
                    Ok(_) => println!("Created subdir: {}", part),
                    Err(ftp::FtpError::InvalidResponse(msg)) if msg.contains("550") => {}
                    Err(e) => {
                        return Err(SyncError::Ftp(format!(
                            "Failed to make_dir({}): {}",
                            part, e
                        )));
                    }
                }
                //trecem mai departe
                ftp_stream
                    .cwd(part)
                    .map_err(|e| SyncError::Ftp(format!("Failed cwd({}): {}", part, e)))?;
            }
        }

        // Upload
        let mut cursor = Cursor::new(data.to_vec());
        ftp_stream
            .put(filename, &mut cursor)
            .map_err(|e| SyncError::Ftp(format!("Failed put {}: {}", filename, e)))?;

        // revenim la root
        if !dir.is_empty() && dir != "." {
            let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
            for _ in 0..parts.len() {
                let _ = ftp_stream.cdup();
            }
        }

        Ok(())
    }

    ////////////////////////////////////////// DELETE FILE //////////////////////////////////////////
    fn delete_file(&self, path: &str) -> Result<(), SyncError> {
        let mut ftp_stream = self.connect()?;
        let (dir, filename) = split_path_dir_file(path);

        if !dir.is_empty() && dir != "." {
            let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
            // coboram
            for part in &parts {
                ftp_stream
                    .cwd(part)
                    .map_err(|e| SyncError::Ftp(format!("Failed cwd({}): {}", part, e)))?;
            }
        }

        match ftp_stream.rm(filename) {
            Ok(_) => println!("File {} deleted successfully.", path),
            Err(ftp::FtpError::InvalidResponse(msg)) if msg.contains("550") => {
                println!("File {} does not exist or already deleted: {}", path, msg);
            }
            Err(e) => {
                return Err(SyncError::Ftp(format!(
                    "Failed to delete file {}, error: {}",
                    path, e
                )));
            }
        }

        // revenim la root
        if !dir.is_empty() && dir != "." {
            let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
            for _ in 0..parts.len() {
                let _ = ftp_stream.cdup();
            }
        }
        Ok(())
    }

    ///////////////////////////////////// CREATE DIR (recursiv) //////////////////////////////////////////
    fn create_dir(&self, path: &str) -> Result<(), SyncError> {
        let mut ftp_stream = self.connect()?;
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        println!("Creating directory: '{}'", path);
        let mut current_path = String::new();

        for part in &parts {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(part);

            println!("Ensuring directory exists: '{}'", current_path);
            match ftp_stream.mkdir(part) {
                Ok(_) => {
                    println!("Created directory '{}'", current_path);
                }
                Err(ftp::FtpError::InvalidResponse(msg)) if msg.contains("550") => {
                    println!("Directory '{}' may already exist: {}", current_path, msg);
                }
                Err(e) => {
                    return Err(SyncError::Ftp(format!(
                        "Failed to create directory '{}': {}",
                        current_path, e
                    )));
                }
            }

            println!("Changing directory to '{}'", current_path);
            ftp_stream.cwd(part).map_err(|e| {
                SyncError::Ftp(format!(
                    "Failed to change directory to '{}': {}",
                    current_path, e
                ))
            })?;
        }

        for _ in 0..parts.len() {
            println!("Changing directory up");
            ftp_stream
                .cdup()
                .map_err(|e| SyncError::Ftp(format!("Failed to cdup: {}", e)))?;
        }

        Ok(())
    }

    //////////////////////////////////// REMOVE DIR (recursiv) ////////////////////////////////////////
    fn remove_dir(&self, path: &str) -> Result<(), SyncError> {
        let mut ftp_stream = self.connect()?;
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        println!("Removing directory: '{}'", path);

        // Coboram
        for part in &parts[..parts.len().saturating_sub(1)] {
            println!("Changing directory to '{}'", part);
            ftp_stream
                .cwd(part)
                .map_err(|e| SyncError::Ftp(format!("Failed to cwd to '{}': {}", part, e)))?;
        }

        // Directorul care trebuie sters
        let target_dir = parts.last().unwrap_or(&"");
        if target_dir.is_empty() {
            println!("Invalid target directory to remove: '{}'", path);
            return Ok(());
        }

        println!("Listing contents of directory '{}'", target_dir);
        let entries = ftp_stream
            .list(Some(target_dir))
            .map_err(|e| SyncError::Ftp(format!("Failed to list '{}': {}", target_dir, e)))?;

        for entry_line in entries {
            println!("Entry in '{}': '{}'", target_dir, entry_line);
            if let Some((name, is_dir, _maybe_time)) = parse_list_entry_dir(&entry_line) {
                if is_dir {
                    // Recursiv: remove_dir a/b
                    let sub_path = format!("{}/{}", path, name);
                    println!("Recursively removing subdirectory: '{}'", sub_path);
                    self.remove_dir(&sub_path)?;
                } else {
                    println!("Removing file '{}'", name);
                    match ftp_stream.rm(&format!("{}/{}", target_dir, name)) {
                        Ok(_) => println!("Removed file '{}'", name),
                        Err(e) => println!("Failed to remove file '{}': {:?}", name, e),
                    }
                }
            }
        }

        println!("Removing directory '{}'", path);
        match ftp_stream.rmdir(target_dir) {
            Ok(_) => println!("Removed directory '{}'", path),
            Err(e) => {
                println!("Cannot remove directory '{}': {:?}", path, e);

                return Err(SyncError::Ftp(format!(
                    "Failed to remove directory '{}': {}",
                    path, e
                )));
            }
        }

        Ok(())
    }
}

fn split_path_dir_file(path: &str) -> (String, &str) {
    match path.rsplit_once('/') {
        Some((dir, file)) => (dir.to_string(), file),
        None => ("".to_string(), path),
    }
}

fn parse_list_entry_dir(entry_line: &str) -> Option<(String, bool, Option<SystemTime>)> {
    let parts: Vec<&str> = entry_line.split_whitespace().collect();
    if parts.len() < 9 {
        return None;
    }
    let file_type = parts[0].chars().next()?;
    let is_dir = file_type == 'd';
    let name = parts[8..].join(" ");
    let maybe_time = None;

    Some((name, is_dir, maybe_time))
}

fn parse_list_entry(line: &str) -> Option<(String, bool, Option<SystemTime>)> {
    println!("Parsing entry: {}", line);

    let parts: Vec<&str> = line.split_whitespace().collect();
    println!("Split parts: {:?}", parts);

    if parts.len() < 9 {
        println!("Entry has insufficient parts: {:?}", parts);
        return None;
    }

    let file_type = parts[0].chars().next()?;
    let is_dir = file_type == 'd';
    let name = parts[8..].join(" ");
    println!("File name: {}", name);

    let modified = match parse_ftp_date(&parts[5..8]) {
        Some(m) => m,
        None => SystemTime::UNIX_EPOCH,
    };

    Some((name, is_dir, Some(modified)))
}

fn parse_ftp_date(date_parts: &[&str]) -> Option<SystemTime> {
    if date_parts.len() != 3 {
        println!("Unexpected number of date parts: {:?}", date_parts);
        return None;
    }

    // parsam luna
    let month = match date_parts[0] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        other => {
            println!("Invalid month: {}", other);
            return None;
        }
    };

    // Parsam ziua
    let day = match date_parts[1].parse::<u32>() {
        Ok(d) => d,
        Err(e) => {
            println!("Failed to parse day: {}, error: {}", date_parts[1], e);
            return None;
        }
    };

    // Parsam ora (HH:MM)
    let naive_time = match NaiveTime::parse_from_str(date_parts[2], "%H:%M") {
        Ok(t) => t,
        Err(e) => {
            println!(
                "Failed to parse time (HH:MM): {}, error: {}",
                date_parts[2], e
            );
            return None;
        }
    };

    // luam anul curent din Utc
    let current_date = Utc::now();
    let current_year = current_date.year();

    // NaiveDateTime cu anul curent
    let naive_date = NaiveDate::from_ymd_opt(current_year, month, day)?;
    let naive_datetime = NaiveDateTime::new(naive_date, naive_time);

    let datetime = Utc.from_utc_datetime(&naive_datetime);

    Some(datetime.into())
}

fn calculate_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}
