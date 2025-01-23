use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

pub fn watch_folder(
    path: &std::path::Path,
) -> Result<Receiver<notify::Result<Event>>, Box<dyn std::error::Error>> {
    let (tx, rx) = channel();

    // cream un watcher nou
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            tx.send(res).expect("Failed to send event over channel");
        },
        // il facem non-blocant
        notify::Config::default(),
    )?;

    watcher.watch(path, RecursiveMode::Recursive)?;

    //il punem intr-un thread
    thread::spawn(move || {
        // "park" the thread: keep it alive
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    });

    Ok(rx)
}
