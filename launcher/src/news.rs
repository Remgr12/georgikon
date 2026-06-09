use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// Shape of each item returned by the news feed URL.
/// Serve a JSON array of these at your news URL.
#[derive(Deserialize, Clone, Debug)]
pub struct NewsItem {
    pub title: String,
    pub body: String,
    pub date: String,
    /// Optional link shown as "Read more →"
    #[serde(default)]
    pub url: Option<String>,
}

pub enum NewsState {
    Idle,
    Loading,
    Loaded(Vec<NewsItem>),
    Error(String),
}

/// Spawns a background thread to fetch news and updates `state` when done.
/// Does nothing if `url` is empty.
pub fn start_fetch(url: String, state: Arc<Mutex<NewsState>>) {
    if url.is_empty() {
        *state.lock().unwrap() = NewsState::Idle;
        return;
    }
    *state.lock().unwrap() = NewsState::Loading;
    std::thread::spawn(move || {
        let result = reqwest::blocking::get(&url)
            .and_then(|r| r.json::<Vec<NewsItem>>())
            .map_err(|e| e.to_string());
        *state.lock().unwrap() = match result {
            Ok(items) => NewsState::Loaded(items),
            Err(e) => NewsState::Error(e),
        };
    });
}
