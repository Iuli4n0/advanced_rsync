use crate::errors::SyncError;
use crate::locations::{parse_location, FolderLocation, Location};
use std::time::{Duration, Instant};
mod errors;
mod locations;
mod sync_logic;
use crate::sync_logic::*;
use crate::watchers::*;

fn main() -> Result<(), SyncError> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <location1> <location2> ...", args[0]);
        std::process::exit(1);
    }

    let mut locations: Vec<Box<dyn Location>> = Vec::new();
    for loc_str in &args[1..] {
        let loc = parse_location(loc_str)?;
        locations.push(loc);
    }

    //Initializam SyncState
    let mut sync_state = SyncState::new();
    initial_sync_with_state(&locations, &mut sync_state)?;

    //watcher pentru foldere locale
    let folder_path = locations.iter().find_map(|loc| {
        loc.as_any()
            .downcast_ref::<FolderLocation>()
            .map(|f| f.path.clone())
    });

    let rx = if let Some(p) = folder_path {
        Some(watch_folder(&p).map_err(|e| SyncError::Parse(e.to_string()))?)
    } else {
        None
    };

    let mut last_ftp_poll = Instant::now();
    loop {
        //Tratam evenimentele din foldere
        if let Some(ref rx_channel) = rx {
            while let Ok(event_res) = rx_channel.try_recv() {
                match event_res {
                    Ok(event) => {
                        println!("Local folder event: {:?}", event);
                        handle_local_event(&event, &mut locations, &mut sync_state)?;
                    }
                    Err(e) => eprintln!("Watcher error: {:?}", e),
                }
            }
        }

        if last_ftp_poll.elapsed() > Duration::from_secs(10) {
            println!("\nPolling FTP locations...");
            poll_locations(&locations, &mut sync_state)?;
            last_ftp_poll = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(500));
    }
}
