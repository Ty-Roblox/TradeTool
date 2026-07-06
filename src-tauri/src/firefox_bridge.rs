use crate::models::TradeListing;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(windows)]
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BRIDGE_PORT_START: u16 = 17652;
const BRIDGE_PORT_END: u16 = 17661;
const PAIRING_HEADER: &str = "x-tradetool-key";
const TELEPORT_RESULT_TIMEOUT: Duration = Duration::from_secs(15);
const ADDON_CONNECTED_WINDOW: Duration = Duration::from_secs(35);
const PAIRING_KEY_NAMESPACE: &str = "TradeTool POE2 Firefox TP Bridge machine key v1";
const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

static BRIDGE: OnceLock<Arc<BridgeRuntime>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirefoxBridgeStatus {
    pub enabled: bool,
    pub port: Option<u16>,
    pub pairing_key: String,
    pub connected: bool,
    pub pending: bool,
    pub last_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeleportToHideoutResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgePendingRequest {
    pub request_id: u64,
    pub listing_id: String,
    pub token: Option<String>,
    pub fetch_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeTeleportResult {
    pub request_id: u64,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Debug, Default)]
struct PendingState {
    next_request_id: u64,
    pending: Option<BridgePendingRequest>,
    results: HashMap<u64, BridgeTeleportResult>,
}

#[derive(Debug, Clone)]
struct ListingTeleportTarget {
    token: Option<String>,
    fetch_url: Option<String>,
}

#[derive(Debug)]
pub struct BridgeCore {
    pairing_key: String,
    listings: Mutex<HashMap<String, ListingTeleportTarget>>,
    pending: Mutex<PendingState>,
}

impl BridgeCore {
    pub fn new(pairing_key: String) -> Self {
        Self {
            pairing_key,
            listings: Mutex::new(HashMap::new()),
            pending: Mutex::new(PendingState {
                next_request_id: 1,
                pending: None,
                results: HashMap::new(),
            }),
        }
    }

    pub fn replace_listing_tokens(&self, listings: &[TradeListing], fetch_url: Option<&str>) {
        let mut stored_listings = self.listings.lock().expect("listing store poisoned");
        stored_listings.clear();

        for listing in listings {
            if !listing.can_teleport {
                continue;
            }

            let token = listing
                .hideout_token
                .as_deref()
                .filter(|token| !token.trim().is_empty())
                .map(str::to_string);
            let fetch_url = fetch_url.map(str::to_string);

            if token.is_some() || fetch_url.is_some() {
                stored_listings.insert(
                    listing.id.clone(),
                    ListingTeleportTarget { token, fetch_url },
                );
            }
        }

        let mut pending = self.pending.lock().expect("pending store poisoned");
        pending.pending = None;
        pending.results.clear();
    }

    #[cfg(test)]
    pub fn token_for_listing(&self, listing_id: &str) -> Option<String> {
        self.listings
            .lock()
            .expect("listing store poisoned")
            .get(listing_id)
            .and_then(|listing| listing.token.clone())
    }

    pub fn queue_teleport(&self, listing_id: &str) -> Result<u64, String> {
        let target = self
            .listings
            .lock()
            .expect("listing store poisoned")
            .get(listing_id)
            .cloned()
            .ok_or_else(|| "Teleport is not available for that listing.".to_string())?;

        if target.token.is_none() && target.fetch_url.is_none() {
            return Err("Teleport is not available for that listing.".to_string());
        }

        let mut pending = self.pending.lock().expect("pending store poisoned");
        let request_id = pending.next_request_id;
        pending.next_request_id += 1;
        pending.pending = Some(BridgePendingRequest {
            request_id,
            listing_id: listing_id.to_string(),
            token: target.token,
            fetch_url: target.fetch_url,
        });
        pending.results.remove(&request_id);
        Ok(request_id)
    }

    pub fn take_pending_request(&self) -> Option<BridgePendingRequest> {
        self.pending
            .lock()
            .expect("pending store poisoned")
            .pending
            .take()
    }

    pub fn complete_request(&self, result: BridgeTeleportResult) -> Result<(), String> {
        let mut pending = self.pending.lock().expect("pending store poisoned");
        pending.results.insert(result.request_id, result);
        Ok(())
    }

    pub fn take_result(&self, request_id: u64) -> Option<BridgeTeleportResult> {
        self.pending
            .lock()
            .expect("pending store poisoned")
            .results
            .remove(&request_id)
    }

    pub fn has_pending_request(&self) -> bool {
        self.pending
            .lock()
            .expect("pending store poisoned")
            .pending
            .is_some()
    }

    pub fn is_valid_pairing_key(&self, key: &str) -> bool {
        !key.is_empty() && key == self.pairing_key
    }

    pub fn pairing_key(&self) -> &str {
        &self.pairing_key
    }
}

#[derive(Debug)]
struct BridgeRuntime {
    core: BridgeCore,
    port: Option<u16>,
    last_seen: Mutex<Option<Instant>>,
    last_message: Mutex<Option<String>>,
}

impl BridgeRuntime {
    fn mark_seen(&self) {
        *self.last_seen.lock().expect("last seen poisoned") = Some(Instant::now());
    }

    fn set_message(&self, message: impl Into<String>) {
        *self.last_message.lock().expect("last message poisoned") = Some(message.into());
    }

    fn is_connected(&self) -> bool {
        self.last_seen
            .lock()
            .expect("last seen poisoned")
            .is_some_and(|seen| seen.elapsed() <= ADDON_CONNECTED_WINDOW)
    }
}

pub fn start() {
    let _ = bridge();
}

pub fn replace_listing_tokens(listings: &[TradeListing], fetch_url: Option<&str>) {
    bridge().core.replace_listing_tokens(listings, fetch_url);
}

pub fn clear_listing_tokens() {
    bridge().core.replace_listing_tokens(&[], None);
}

pub fn status() -> FirefoxBridgeStatus {
    let runtime = bridge();
    let last_message = runtime
        .last_message
        .lock()
        .expect("last message poisoned")
        .clone();

    FirefoxBridgeStatus {
        enabled: runtime.port.is_some(),
        port: runtime.port,
        pairing_key: runtime.core.pairing_key().to_string(),
        connected: runtime.is_connected(),
        pending: runtime.core.has_pending_request(),
        last_message,
    }
}

pub fn teleport_to_hideout(listing_id: String) -> Result<TeleportToHideoutResponse, String> {
    let runtime = bridge();

    if runtime.port.is_none() {
        return Err("Firefox TP bridge could not bind a local port.".to_string());
    }

    let request_id = runtime.core.queue_teleport(&listing_id)?;
    runtime.set_message(format!("Queued TP request for listing {listing_id}."));
    let deadline = Instant::now() + TELEPORT_RESULT_TIMEOUT;

    while Instant::now() < deadline {
        if let Some(result) = runtime.core.take_result(request_id) {
            if result.success {
                let message = result
                    .message
                    .unwrap_or_else(|| "Teleport request sent.".to_string());
                runtime.set_message(message.clone());
                return Ok(TeleportToHideoutResponse {
                    success: true,
                    message,
                });
            }

            let message = result
                .message
                .unwrap_or_else(|| "Firefox could not send the teleport request.".to_string());
            runtime.set_message(message.clone());
            return Err(message);
        }

        thread::sleep(Duration::from_millis(100));
    }

    let message = if runtime.is_connected() {
        "Firefox add-on did not return a teleport result in time."
    } else {
        "Firefox add-on disconnected or is not paired."
    };
    runtime.set_message(message);
    Err(message.to_string())
}

fn bridge() -> Arc<BridgeRuntime> {
    BRIDGE.get_or_init(create_bridge_runtime).clone()
}

fn create_bridge_runtime() -> Arc<BridgeRuntime> {
    let pairing_key = generate_pairing_key();
    let core = BridgeCore::new(pairing_key);
    let mut bound_listener = None;
    let mut bound_port = None;

    for port in BRIDGE_PORT_START..=BRIDGE_PORT_END {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            bound_port = Some(port);
            bound_listener = Some(listener);
            break;
        }
    }

    let runtime = Arc::new(BridgeRuntime {
        core,
        port: bound_port,
        last_seen: Mutex::new(None),
        last_message: Mutex::new(Some(bound_port.map_or_else(
            || "Firefox TP bridge could not bind a local port.".to_string(),
            |port| format!("Firefox TP bridge listening on 127.0.0.1:{port}."),
        ))),
    });

    if let Some(listener) = bound_listener {
        let runtime = runtime.clone();
        thread::spawn(move || serve_bridge(listener, runtime));
    }

    runtime
}

fn generate_pairing_key() -> String {
    machine_pairing_key().unwrap_or_else(generate_ephemeral_pairing_key)
}

fn machine_pairing_key() -> Option<String> {
    derive_machine_pairing_key(&machine_pairing_signals())
}

fn generate_ephemeral_pairing_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let stack_marker = &nanos as *const u128 as usize;
    format!("{nanos:x}{:x}{stack_marker:x}", std::process::id())
}

fn machine_pairing_signals() -> Vec<String> {
    let mut signals = Vec::new();

    if let Some(machine_guid) = windows_machine_guid_signal() {
        signals.push(machine_guid);
    }

    push_env_signal(&mut signals, "computer-name", "COMPUTERNAME");
    push_env_signal(&mut signals, "hostname", "HOSTNAME");

    signals
}

fn push_env_signal(signals: &mut Vec<String>, label: &str, variable: &str) {
    let Ok(value) = env::var(variable) else {
        return;
    };
    let value = value.trim();
    if !value.is_empty() {
        signals.push(format!("{label}:{value}"));
    }
}

#[cfg(windows)]
fn windows_machine_guid_signal() -> Option<String> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_windows_machine_guid(&String::from_utf8_lossy(&output.stdout))
        .map(|guid| format!("machine-guid:{guid}"))
}

#[cfg(not(windows))]
fn windows_machine_guid_signal() -> Option<String> {
    None
}

#[cfg(windows)]
fn parse_windows_machine_guid(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        if !line.to_ascii_lowercase().contains("machineguid") {
            return None;
        }

        line.split_whitespace()
            .last()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
    })
}

fn derive_machine_pairing_key(signals: &[String]) -> Option<String> {
    let mut normalized = signals
        .iter()
        .map(|signal| signal.trim())
        .filter(|signal| !signal.is_empty())
        .map(|signal| signal.to_ascii_lowercase())
        .collect::<Vec<_>>();

    if normalized.is_empty() {
        return None;
    }

    normalized.sort();
    normalized.dedup();

    let input = format!("{PAIRING_KEY_NAMESPACE}\n{}", normalized.join("\n"));
    let first = fnv1a64(FNV_OFFSET, input.as_bytes());
    let reversed = input.bytes().rev().collect::<Vec<_>>();
    let second = fnv1a64(FNV_OFFSET ^ 0xa5a5_a5a5_a5a5_a5a5, &reversed);

    Some(format!("{first:016x}{second:016x}"))
}

fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        let hash = hash ^ u64::from(*byte);
        hash.wrapping_mul(FNV_PRIME)
    })
}

fn serve_bridge(listener: TcpListener, runtime: Arc<BridgeRuntime>) {
    for stream in listener.incoming().flatten() {
        let runtime = runtime.clone();
        thread::spawn(move || {
            let _ = handle_stream(stream, runtime);
        });
    }
}

fn handle_stream(mut stream: TcpStream, runtime: Arc<BridgeRuntime>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let Some(request) = read_http_request(&mut stream)? else {
        return Ok(());
    };

    let response = handle_bridge_request(request, &runtime);
    write_http_response(&mut stream, response)
}

#[derive(Debug)]
struct BridgeHttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

#[derive(Debug)]
struct BridgeHttpResponse {
    status: &'static str,
    content_type: &'static str,
    body: String,
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<Option<BridgeHttpRequest>> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 4096];

    loop {
        let bytes_read = stream.read(&mut temp)?;
        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..bytes_read]);

        if let Some(header_end) = find_header_end(&buffer) {
            let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
            let content_length = parse_content_length(&header_text);
            let body_start = header_end + 4;
            let expected_len = body_start + content_length;

            while buffer.len() < expected_len {
                let bytes_read = stream.read(&mut temp)?;
                if bytes_read == 0 {
                    break;
                }
                buffer.extend_from_slice(&temp[..bytes_read]);
            }

            return Ok(parse_http_request(
                &buffer[..buffer.len().min(expected_len)],
            ));
        }

        if buffer.len() > 64 * 1024 {
            return Ok(None);
        }
    }

    Ok(None)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(header_text: &str) -> usize {
    header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or_default()
}

fn parse_http_request(bytes: &[u8]) -> Option<BridgeHttpRequest> {
    let text = String::from_utf8_lossy(bytes);
    let (head, body) = text.split_once("\r\n\r\n")?;
    let mut lines = head.lines();
    let request_line = lines.next()?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next()?.to_string();
    let path = request_parts.next()?.to_string();
    let mut headers = HashMap::new();

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    Some(BridgeHttpRequest {
        method,
        path,
        headers,
        body: body.to_string(),
    })
}

fn handle_bridge_request(
    request: BridgeHttpRequest,
    runtime: &BridgeRuntime,
) -> BridgeHttpResponse {
    if request.method == "OPTIONS" {
        return empty_response("204 No Content");
    }

    let key = request
        .headers
        .get(PAIRING_HEADER)
        .map(String::as_str)
        .unwrap_or_default();

    if !runtime.core.is_valid_pairing_key(key) {
        return json_response(
            "401 Unauthorized",
            serde_json::json!({ "error": "Invalid TradeTool pairing key." }),
        );
    }

    runtime.mark_seen();

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => json_response(
            "200 OK",
            serde_json::json!({
                "ready": true,
                "port": runtime.port,
            }),
        ),
        ("GET", "/next") => match runtime.core.take_pending_request() {
            Some(request) => json_response("200 OK", request),
            None => empty_response("204 No Content"),
        },
        ("POST", "/result") => match serde_json::from_str::<BridgeTeleportResult>(&request.body) {
            Ok(result) => match runtime.core.complete_request(result) {
                Ok(()) => json_response("200 OK", serde_json::json!({ "ok": true })),
                Err(error) => {
                    json_response("400 Bad Request", serde_json::json!({ "error": error }))
                }
            },
            Err(error) => json_response(
                "400 Bad Request",
                serde_json::json!({ "error": format!("Invalid TP result payload: {error}") }),
            ),
        },
        _ => json_response(
            "404 Not Found",
            serde_json::json!({ "error": "Unknown TradeTool bridge route." }),
        ),
    }
}

fn empty_response(status: &'static str) -> BridgeHttpResponse {
    BridgeHttpResponse {
        status,
        content_type: "text/plain; charset=utf-8",
        body: String::new(),
    }
}

fn json_response(status: &'static str, body: impl Serialize) -> BridgeHttpResponse {
    BridgeHttpResponse {
        status,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string()),
    }
}

fn write_http_response(
    stream: &mut TcpStream,
    response: BridgeHttpResponse,
) -> std::io::Result<()> {
    let headers = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: {}, Content-Type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len(),
        PAIRING_HEADER,
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(response.body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TradeListing, TradeListingItem};

    fn listing(id: &str, token: Option<&str>) -> TradeListing {
        TradeListing {
            id: id.to_string(),
            indexed: None,
            price: Some(crate::models::TradePrice {
                price_type: Some("~b/o".to_string()),
                amount: 1.0,
                currency: "exalted".to_string(),
            }),
            account_name: None,
            can_teleport: true,
            hideout_token: token.map(str::to_string),
            item: TradeListingItem {
                icon: None,
                name: None,
                type_line: None,
                base_type: None,
                rarity: None,
                item_level: None,
                explicit_mods: Vec::new(),
                pseudo_mods: Vec::new(),
                explicit_mod_segments: Vec::new(),
                pseudo_mod_segments: Vec::new(),
            },
        }
    }

    #[test]
    fn token_store_replaces_previous_search_tokens() {
        let core = BridgeCore::new("test-key".to_string());
        core.replace_listing_tokens(&[listing("old", Some("old-token"))], Some("old-fetch"));
        assert_eq!(core.token_for_listing("old").as_deref(), Some("old-token"));

        core.replace_listing_tokens(
            &[listing("new", Some("new-token")), listing("empty", None)],
            Some("new-fetch"),
        );

        assert!(core.token_for_listing("old").is_none());
        assert_eq!(core.token_for_listing("new").as_deref(), Some("new-token"));
        assert!(core.token_for_listing("empty").is_none());
    }

    #[test]
    fn queue_teleport_can_use_fetch_url_when_token_is_missing() {
        let core = BridgeCore::new("test-key".to_string());
        core.replace_listing_tokens(&[listing("needs-firefox", None)], Some("fetch-url"));

        let request_id = core
            .queue_teleport("needs-firefox")
            .expect("request should queue");
        let pending = core.take_pending_request().expect("pending request");

        assert_eq!(pending.request_id, request_id);
        assert_eq!(pending.listing_id, "needs-firefox");
        assert_eq!(pending.token, None);
        assert_eq!(pending.fetch_url.as_deref(), Some("fetch-url"));
    }

    #[test]
    fn queue_teleport_rejects_unknown_and_missing_token_listings() {
        let core = BridgeCore::new("test-key".to_string());
        core.replace_listing_tokens(&[listing("has-token", Some("secret-token"))], Some("fetch"));

        let unknown = core.queue_teleport("missing");
        assert!(unknown
            .expect_err("unknown listing should fail")
            .contains("not available"));

        core.replace_listing_tokens(&[listing("no-token", None)], None);
        let missing_token = core.queue_teleport("no-token");
        assert!(missing_token
            .expect_err("listing without token should fail")
            .contains("not available"));
    }

    #[test]
    fn queued_teleport_can_be_taken_and_completed() {
        let core = BridgeCore::new("test-key".to_string());
        core.replace_listing_tokens(&[listing("listing-1", Some("secret-token"))], Some("fetch"));

        let request_id = core
            .queue_teleport("listing-1")
            .expect("request should queue");
        let pending = core.take_pending_request().expect("pending request");

        assert_eq!(pending.request_id, request_id);
        assert_eq!(pending.listing_id, "listing-1");
        assert_eq!(pending.token.as_deref(), Some("secret-token"));

        core.complete_request(BridgeTeleportResult {
            request_id,
            success: true,
            message: Some("sent".to_string()),
        })
        .expect("result should complete");

        let result = core
            .take_result(request_id)
            .expect("completed result should be stored");
        assert!(result.success);
        assert_eq!(result.message.as_deref(), Some("sent"));
    }

    #[test]
    fn pairing_key_is_required() {
        let core = BridgeCore::new("test-key".to_string());

        assert!(core.is_valid_pairing_key("test-key"));
        assert!(!core.is_valid_pairing_key(""));
        assert!(!core.is_valid_pairing_key("wrong-key"));
    }

    #[test]
    fn machine_pairing_key_is_deterministic_and_hex() {
        let signals = vec![
            "machine-guid:abc123".to_string(),
            "computer-name:tradebox".to_string(),
        ];

        let first = derive_machine_pairing_key(&signals).expect("key should derive");
        let second = derive_machine_pairing_key(&signals).expect("key should derive again");

        assert_eq!(first, second);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert!(!first.contains("abc123"));
        assert!(!first.contains("tradebox"));
    }

    #[test]
    fn machine_pairing_key_changes_with_machine_signals() {
        let first = derive_machine_pairing_key(&["machine-guid:first".to_string()])
            .expect("first key should derive");
        let second = derive_machine_pairing_key(&["machine-guid:second".to_string()])
            .expect("second key should derive");

        assert_ne!(first, second);
    }

    #[test]
    fn machine_pairing_key_requires_non_empty_signals() {
        let empty = derive_machine_pairing_key(&["".to_string(), "   ".to_string()]);
        assert!(empty.is_none());
    }
}
