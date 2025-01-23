pub mod watchers;
use crate::errors::SyncError;
use crate::locations::{DirMetadata, FileMetadata, FolderLocation, Location, ZipLocation};
use notify::{
    event::{ModifyKind, RemoveKind},
    Event, EventKind,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::SystemTime;

pub struct SyncState {
    pub file_metadata: HashMap<String, FileMetadata>,
    pub dir_metadata: HashMap<String, DirMetadata>,
}

impl SyncState {
    pub fn new() -> Self {
        SyncState {
            file_metadata: HashMap::new(),
            dir_metadata: HashMap::new(),
        }
    }

    pub fn update_file(&mut self, path: String, metadata: FileMetadata) {
        self.file_metadata.insert(path, metadata);
    }

    pub fn remove_file(&mut self, path: &str) {
        self.file_metadata.remove(path);
    }

    pub fn update_dir(&mut self, path: String, metadata: DirMetadata) {
        self.dir_metadata.insert(path, metadata);
    }
    pub fn remove_dir(&mut self, path: &str) {
        self.dir_metadata.remove(path);
    }
}

pub fn handle_local_event(
    event: &Event,
    locations: &mut Vec<Box<dyn Location>>,
    sync_state: &mut SyncState,
) -> Result<(), SyncError> {
    match &event.kind {
        EventKind::Create(_) => {
            for path in &event.paths {
                let meta = match fs::metadata(path) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to read metadata for {:?}: {:?}", path, e);
                        continue; // Trecem peste
                    }
                };

                let folder_loc = locations
                    .iter()
                    .find_map(|loc| loc.as_any().downcast_ref::<FolderLocation>());

                let relative_path = if let Some(folder) = folder_loc {
                    match path.strip_prefix(&folder.path) {
                        Ok(rel) => rel.to_string_lossy().to_string(),
                        Err(_) => path.to_string_lossy().to_string(),
                    }
                } else {
                    path.to_string_lossy().to_string()
                };

                if meta.is_dir() {
                    println!("Handling create for directory: {}", relative_path);

                    for loc in locations.iter_mut() {
                        if !loc.as_any().is::<ZipLocation>() {
                            loc.create_dir(&relative_path)?;
                            println!("Directory '{}' created in location", relative_path);
                        }
                    }

                    sync_state.update_dir(
                        relative_path.clone(),
                        DirMetadata {
                            path: relative_path.clone(),
                            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        },
                    );
                } else {
                    println!("Handling create for file: {}", relative_path);
                    sync_file(locations, &relative_path, sync_state)?;
                }
            }
        }

        EventKind::Modify(ModifyKind::Data(_)) => {
            for path in &event.paths {
                let relative_path = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());

                println!("Handling modify for file: {}", relative_path);
                sync_file(locations, &relative_path, sync_state)?;
            }
        }

        EventKind::Remove(kind) => {
            for path in &event.paths {
                let is_dir = match kind {
                    RemoveKind::Folder => true,
                    RemoveKind::Any => false,
                    RemoveKind::File => false,
                    RemoveKind::Other => false,
                };

                let folder_loc = locations
                    .iter()
                    .find_map(|loc| loc.as_any().downcast_ref::<FolderLocation>());
                let relative_path = if let Some(folder) = folder_loc {
                    match path.strip_prefix(&folder.path) {
                        Ok(rel) => rel.to_string_lossy().to_string(),
                        Err(_) => path.to_string_lossy().to_string(),
                    }
                } else {
                    path.to_string_lossy().to_string()
                };

                if is_dir {
                    println!("Handling remove for directory: {}", relative_path);
                    for loc in locations.iter_mut() {
                        if !loc.as_any().is::<ZipLocation>() {
                            loc.remove_dir(&relative_path)?;
                            println!("Directory {} removed from location", relative_path);
                        }
                    }
                    sync_state.remove_dir(&relative_path);
                } else {
                    println!("Handling remove for file: {}", relative_path);
                    for loc in locations.iter_mut() {
                        if !loc.as_any().is::<ZipLocation>() {
                            loc.delete_file(&relative_path)?;
                            println!("File {} deleted from location", relative_path);
                        }
                    }
                    sync_state.remove_file(&relative_path);
                }
            }
        }

        _ => {
            println!("Unhandled event kind: {:?}", event.kind);
        }
    }

    Ok(())
}

pub fn initial_sync_with_state(
    locations: &[Box<dyn Location>],
    sync_state: &mut SyncState,
) -> Result<(), SyncError> {
    ///////////////////////Directoare////////////////////
    let mut all_dirs = Vec::new();
    for loc in locations.iter() {
        let dirs = loc.list_dirs_recursive()?;
        all_dirs.push(dirs);
    }

    // Facem un set cu toate directoarele
    use std::collections::HashSet;
    let mut all_dirs_set: HashSet<String> = HashSet::new();
    for dir_list in &all_dirs {
        for d in dir_list {
            all_dirs_set.insert(d.path.clone());
        }
    }

    for dirname in &all_dirs_set {
        let mut newest_time = SystemTime::UNIX_EPOCH;
        let mut newest_loc_idx: Option<usize> = None;

        for (i, dir_list) in all_dirs.iter().enumerate() {
            if let Some(dmeta) = dir_list.iter().find(|x| x.path == *dirname) {
                if dmeta.modified > newest_time {
                    newest_time = dmeta.modified;
                    newest_loc_idx = Some(i);
                }
            }
        }

        if let Some(idx) = newest_loc_idx {
            for (i, loc) in locations.iter().enumerate() {
                if i != idx {
                    loc.create_dir(dirname)?;
                }
            }
            sync_state.update_dir(
                dirname.clone(),
                DirMetadata {
                    path: dirname.clone(),
                    modified: newest_time,
                },
            );
        }
    }

    //////////////////////////Fisiere//////////////////////////////

    let mut all_metadata: Vec<HashMap<String, FileMetadata>> = Vec::new();

    for loc in locations.iter() {
        let files = loc.list_files_recursive()?;
        let mut map = HashMap::new();
        for f in files {
            map.insert(f.path.clone(), f);
        }
        all_metadata.push(map);
    }

    let mut all_files_set: HashSet<String> = HashSet::new();
    for map in &all_metadata {
        all_files_set.extend(map.keys().cloned());
    }

    for filename in &all_files_set {
        let mut newest_loc_idx: Option<usize> = None;
        let mut newest_time = SystemTime::UNIX_EPOCH;

        for (i, map) in all_metadata.iter().enumerate() {
            if let Some(meta) = map.get(filename) {
                if meta.modified > newest_time {
                    newest_time = meta.modified;
                    newest_loc_idx = Some(i);
                }
            }
        }

        let newest_loc_idx = match newest_loc_idx {
            Some(idx) => idx,
            None => continue,
        };

        let newest_data = locations[newest_loc_idx].read_file(filename)?;

        for (i, loc) in locations.iter().enumerate() {
            if i == newest_loc_idx {
                continue;
            }

            let maybe_meta = all_metadata[i].get(filename);
            match maybe_meta {
                Some(meta) if meta.modified < newest_time => {
                    loc.write_file(filename, &newest_data)?;
                }
                None => {
                    loc.write_file(filename, &newest_data)?;
                }
                _ => {}
            }
        }

        let newest_metadata = all_metadata[newest_loc_idx].get(filename).cloned().unwrap();
        sync_state.update_file(filename.clone(), newest_metadata);

        println!("Updated SyncState for file: {}", filename); // Logare
    }

    Ok(())
}

pub fn sync_file(
    locations: &[Box<dyn Location>],
    filename: &str,
    sync_state: &mut SyncState,
) -> Result<(), SyncError> {
    println!("Syncing file {}", filename);

    let mut newest_loc_idx: Option<usize> = None;
    let mut newest_metadata: Option<FileMetadata> = None;
    let mut newest_data: Option<Vec<u8>> = None;

    // Determinam locatia cu fisierul cel mai recent
    for (i, loc) in locations.iter().enumerate() {
        if let Some(metadata) = loc
            .list_files_recursive()?
            .into_iter()
            .find(|f| f.path == filename)
        {
            if newest_metadata.is_none() {
                newest_loc_idx = Some(i);
                newest_metadata = Some(metadata.clone());
                newest_data = Some(loc.read_file(filename)?);
                println!(
                    "metadata: {:?} curent_newest: {:?}",
                    metadata.modified,
                    newest_metadata.as_ref().unwrap().modified
                );
            } else {
                // pe "cel mai nou" il comparam cu cel curent
                let current_newest = newest_metadata.as_ref().unwrap();

                println!(
                    "metadata: {:?} curent_newest: {:?}",
                    metadata.modified, current_newest.modified
                );

                use std::cmp::Ordering;

                match metadata.modified.cmp(&current_newest.modified) {
                    Ordering::Greater => {
                        // Timpul este mai nou
                        let this_hash = metadata.hash.clone().unwrap_or_default();
                        let current_hash = current_newest.hash.clone().unwrap_or_default();

                        if !this_hash.is_empty()
                            && !current_hash.is_empty()
                            && this_hash == current_hash
                        {
                            println!("Fisierul {} are un timp mai nou dar acelasi hash. Skipping update...", filename);
                        } else {
                            // E clar un fisier mai nou diferit
                            newest_loc_idx = Some(i);
                            newest_metadata = Some(metadata.clone());
                            newest_data = Some(loc.read_file(filename)?);
                        }
                    }
                    Ordering::Equal => {
                        // Timp egal, verificam hash-ul
                        println!("Avem un timp egal");
                        let this_hash = metadata.hash.clone().unwrap_or_default();
                        let current_hash = current_newest.hash.clone().unwrap_or_default();

                        if !this_hash.is_empty()
                            && !current_hash.is_empty()
                            && this_hash != current_hash
                        {
                            newest_loc_idx = Some(i);
                            newest_metadata = Some(metadata.clone());
                            newest_data = Some(loc.read_file(filename)?);
                        } else {
                            println!(
                                "Fisierul {} are timp egal si acelasi hash. Skipping update...",
                                filename
                            );
                        }
                    }
                    Ordering::Less => {
                        println!(
                            "Fisierul {} are un timp mai vechi. Skipping update...",
                            filename
                        );
                    }
                }
            }
        }
    }

    let newest_data = match newest_data {
        Some(data) => data,
        None => return Ok(()), // Nimic de sincronizat
    };

    // Propagam fisierul in celelalte locatii unde apare
    for (i, loc) in locations.iter().enumerate() {
        if i != newest_loc_idx.unwrap() {
            if let Some(metadata) = loc
                .list_files_recursive()?
                .into_iter()
                .find(|f| f.path == filename)
            {
                //suprascriem
                if metadata.modified < newest_metadata.as_ref().unwrap().modified {
                    loc.write_file(filename, &newest_data)?;
                    println!("File {} updated in location {}", filename, i);
                }
            } else {
                loc.write_file(filename, &newest_data)?;
                println!("File {} added to location {}", filename, i);
            }
        }
    }

    // Actualizam starea
    if let Some(metadata) = newest_metadata {
        sync_state.update_file(filename.to_string(), metadata);
        println!("Updated SyncState for file: {}\n", filename);
    }

    Ok(())
}

pub fn poll_locations(
    locations: &[Box<dyn Location>],
    sync_state: &mut SyncState,
) -> Result<(), SyncError> {
    for (loc_index, loc) in locations.iter().enumerate() {
        //////////////////Directoare///////////////////////////

        let dirs = loc.list_dirs_recursive()?;
        let current_dirs: HashSet<String> = dirs.iter().map(|d| d.path.clone()).collect();
        let known_dirs: HashSet<String> = sync_state.dir_metadata.keys().cloned().collect();

        let removed_dirs: Vec<String> = known_dirs.difference(&current_dirs).cloned().collect();

        if loc.as_any().is::<ZipLocation>() {
            println!(
                "Location #{} is ZIP -> skip removing files not found in ZIP.",
                loc_index
            );
        } else {
            for rd in removed_dirs {
                println!(
                    "Detected removed folder `{}` in location #{}",
                    rd, loc_index
                );

                if rd.is_empty() || rd == "." {
                    println!("Skipping remove for empty or '.' path: {}", rd);
                    continue;
                } else {
                    sync_state.remove_dir(&rd);

                    for (i2, other_loc) in locations.iter().enumerate() {
                        if !other_loc.as_any().is::<ZipLocation>() {
                            println!("Removing dir `{}` in location #{}", rd, i2);
                            let _ = other_loc.remove_dir(&rd);
                        }
                    }
                }
            }
        }

        // Directoare noi
        for d in &dirs {
            if !sync_state.dir_metadata.contains_key(&d.path) {
                println!(
                    "Detected new folder `{}` in location #{}",
                    d.path, loc_index
                );
                for (i2, other_loc) in locations.iter().enumerate() {
                    if i2 != loc_index && !other_loc.as_any().is::<ZipLocation>() {
                        println!("Creating dir `{}` in location #{}", d.path, i2);
                        let _ = other_loc.create_dir(&d.path);
                    }
                }
                sync_state.update_dir(d.path.clone(), d.clone());
            }
        }

        /////////////////////////Fisiere//////////////////////////
        let files = loc.list_files_recursive()?;

        let current_files: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
        let known_files: HashSet<String> = sync_state.file_metadata.keys().cloned().collect();

        let removed_files: Vec<String> = known_files.difference(&current_files).cloned().collect();

        if loc.as_any().is::<ZipLocation>() {
            println!(
                "Location #{} is ZIP -> skip removing files not found in ZIP.",
                loc_index
            );
        } else {
            for removed_file in removed_files {
                println!(
                    "Detected file removal: {} in location #{}",
                    removed_file, loc_index
                );
                for other_loc in locations {
                    if !other_loc.as_any().is::<ZipLocation>() {
                        match other_loc.delete_file(&removed_file) {
                            Ok(_) => println!("File {} deleted from location", removed_file),
                            Err(e) => println!("Failed to delete file {}: {:?}", removed_file, e),
                        }
                    }
                }
                sync_state.remove_file(&removed_file);
            }
        }

        //fisierele noi sau modificate
        for file in &files {
            if let Some(state_meta) = sync_state.file_metadata.get(&file.path) {
                if state_meta.hash != file.hash {
                    println!(
                        "Detected modification for file: {} in location #{}",
                        file.path, loc_index
                    );
                    sync_file(locations, &file.path, sync_state)?;
                }
            } else {
                println!(
                    "Detected new file: {} in location #{}",
                    file.path, loc_index
                );
                sync_file(locations, &file.path, sync_state)?;
            }
        }
    }

    Ok(())
}
