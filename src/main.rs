//! # gh-notify-bridge
//!
//! Poll GitHub notifications and forward to UnifiedPush.

use std::{
    env, fs,
    sync::{Arc, RwLock},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Response, Server, StatusCode};

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const GITHUB_API: &str = "https://api.github.com";
const STATE_FILE: &str = "state.json";

#[derive(Debug, Deserialize)]
struct GitHubNotification {
    id: String,
    unread: bool,
    reason: String,
    updated_at: String,
    subject: Subject,
    repository: Repository,
}

#[derive(Debug, Deserialize)]
struct Subject {
    title: String,
    #[serde(rename = "type")]
    kind: String,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Repository {
    full_name: String,
}

#[derive(Debug, Serialize)]
struct PushPayload {
    title: String,
    body: String,
    reason: String,
    repo: String,
    id: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    endpoint: Option<String>,
    last_poll: Option<String>,
}

impl PersistedState {
    fn load() -> Self {
        fs::read_to_string(STATE_FILE)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let _ = fs::write(STATE_FILE, serde_json::to_string_pretty(self).unwrap());
    }
}

struct AppState {
    github_token: String,
    state: RwLock<PersistedState>,
}

impl AppState {
    fn new(github_token: String) -> Self {
        Self {
            github_token,
            state: RwLock::new(PersistedState::load()),
        }
    }

    fn get_endpoint(&self) -> Option<String> {
        self.state.read().unwrap().endpoint.clone()
    }

    fn set_endpoint(&self, endpoint: String) {
        let mut state = self.state.write().unwrap();
        state.endpoint = Some(endpoint);
        state.save();
    }

    fn get_last_poll(&self) -> Option<String> {
        self.state.read().unwrap().last_poll.clone()
    }

    fn set_last_poll(&self, timestamp: String) {
        let mut state = self.state.write().unwrap();
        state.last_poll = Some(timestamp);
        state.save();
    }
}

fn fetch_notifications(state: &AppState) -> Result<Vec<GitHubNotification>, String> {
    let mut url = format!("{}/notifications", GITHUB_API);

    if let Some(since) = state.get_last_poll() {
        url = format!("{}?since={}", url, since);
    }

    let resp = ureq::get(&url)
        .set("Authorization", &format!("Bearer {}", state.github_token))
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", "gh-notify-bridge/0.1")
        .call()
        .map_err(|e| format!("GitHub API error: {}", e))?;

    resp.into_json::<Vec<GitHubNotification>>()
        .map_err(|e| format!("JSON parse error: {}", e))
}

fn push_notification(endpoint: &str, payload: &PushPayload) -> Result<(), String> {
    let json = serde_json::to_string(payload).map_err(|e| e.to_string())?;

    ureq::post(endpoint)
        .set("Content-Type", "application/json")
        .send_string(&json)
        .map_err(|e| format!("UP push error: {}", e))?;

    Ok(())
}

fn poll_and_push(state: &Arc<AppState>) {
    use std::time::SystemTime;

    let endpoint = match state.get_endpoint() {
        Some(e) => e,
        None => {
            eprintln!("No endpoint registered, skipping poll");
            return;
        }
    };

    // On first poll (no last_poll timestamp), only push notifications from last 60 seconds
    // This prevents flooding with old notifications while still catching recent ones
    let is_first_poll = state.get_last_poll().is_none();
    let cutoff_time = if is_first_poll {
        // ISO 8601 timestamp for 60 seconds ago
        let secs_ago = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 60;
        Some(format_unix_to_iso(secs_ago))
    } else {
        None
    };

    match fetch_notifications(state) {
        Ok(notifications) => {
            let mut latest_timestamp: Option<String> = None;
            let mut skipped = 0;

            for notif in &notifications {
                if !notif.unread {
                    continue;
                }

                if latest_timestamp.is_none()
                    || notif.updated_at > *latest_timestamp.as_ref().unwrap()
                {
                    latest_timestamp = Some(notif.updated_at.clone());
                }

                // On first poll, skip notifications older than cutoff (60 seconds ago)
                if let Some(ref cutoff) = cutoff_time {
                    if &notif.updated_at < cutoff {
                        skipped += 1;
                        continue;
                    }
                }

                let payload = PushPayload {
                    title: format!("[{}] {}", notif.repository.full_name, notif.subject.title),
                    body: format!("{}: {}", notif.reason, notif.subject.kind),
                    reason: notif.reason.clone(),
                    repo: notif.repository.full_name.clone(),
                    id: notif.id.clone(),
                };

                match push_notification(&endpoint, &payload) {
                    Ok(()) => {
                        eprintln!(
                            "Pushed: {} - {}",
                            notif.repository.full_name, notif.subject.title
                        );
                    }
                    Err(e) => {
                        eprintln!("Failed to push: {}", e);
                    }
                }
            }

            if let Some(ts) = latest_timestamp {
                state.set_last_poll(ts);
                if is_first_poll && skipped > 0 {
                    eprintln!("First poll: skipped {} old notifications", skipped);
                }
            }
        }
        Err(e) => {
            eprintln!("Poll error: {}", e);
        }
    }
}

fn format_unix_to_iso(secs: u64) -> String {
    // Convert unix timestamp to ISO 8601 format: 2024-01-13T12:00:00Z
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Simple algorithm to convert days to year/month/day
    let mut days = days_since_epoch as i64;
    let mut year = 1970i32;

    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            366
        } else {
            365
        };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_months: [i64; 12] = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0;
    for (i, &dim) in days_in_months.iter().enumerate() {
        if days < dim {
            month = i + 1;
            break;
        }
        days -= dim;
    }
    let day = days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn start_poller(state: Arc<AppState>) {
    thread::spawn(move || {
        eprintln!("Poller started (interval: {:?})", POLL_INTERVAL);

        loop {
            let start = Instant::now();
            poll_and_push(&state);

            let elapsed = start.elapsed();
            if elapsed < POLL_INTERVAL {
                thread::sleep(POLL_INTERVAL - elapsed);
            }
        }
    });
}

fn json_response(status: u16, body: &impl Serialize) -> Response<std::io::Cursor<Vec<u8>>> {
    let json = serde_json::to_vec(body).unwrap();
    let len = json.len();
    Response::new(
        StatusCode(status),
        vec![Header::from_bytes("Content-Type", "application/json").unwrap()],
        std::io::Cursor::new(json),
        Some(len),
        None,
    )
}

fn read_body(req: &mut tiny_http::Request) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    req.as_reader().read_to_end(&mut body).ok()?;
    Some(body)
}

#[derive(Deserialize)]
struct RegisterRequest {
    endpoint: String,
}

fn main() {
    let github_token =
        env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN required (with 'notifications' scope)");
    let port = env::var("PORT").unwrap_or_else(|_| "8080".into());

    let state = Arc::new(AppState::new(github_token));

    start_poller(Arc::clone(&state));

    let server = Server::http(format!("[::]:{}", port)).expect("Failed to start server");
    eprintln!("Bridge server listening on :{}", port);

    if let Some(endpoint) = state.get_endpoint() {
        eprintln!("Registered endpoint: {}", endpoint);
    } else {
        eprintln!("No endpoint registered yet. Waiting for app to register...");
    }

    for mut req in server.incoming_requests() {
        let state = Arc::clone(&state);

        thread::spawn(move || {
            let resp = match (req.method(), req.url()) {
                (Method::Get, "/health") => {
                    let endpoint = state.get_endpoint();
                    json_response(
                        200,
                        &serde_json::json!({
                            "status": "ok",
                            "endpoint": endpoint,
                            "registered": endpoint.is_some()
                        }),
                    )
                }

                (Method::Post, "/register") => {
                    let body = read_body(&mut req).unwrap_or_default();
                    match serde_json::from_slice::<RegisterRequest>(&body) {
                        Ok(reg) => {
                            eprintln!("Registered endpoint: {}", reg.endpoint);
                            state.set_endpoint(reg.endpoint.clone());
                            json_response(
                                200,
                                &serde_json::json!({
                                    "success": true,
                                    "endpoint": reg.endpoint
                                }),
                            )
                        }
                        Err(e) => json_response(
                            400,
                            &serde_json::json!({"error": format!("Invalid JSON: {}", e)}),
                        ),
                    }
                }

                (Method::Post, "/poll") => {
                    poll_and_push(&state);
                    json_response(200, &serde_json::json!({"triggered": true}))
                }

                _ => json_response(404, &serde_json::json!({"error": "not found"})),
            };
            let _ = req.respond(resp);
        });
    }
}
