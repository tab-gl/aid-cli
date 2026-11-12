//! Shared helpers for the integration tests: an isolated environment, a mock
//! HTTP server and a CLI runner.
//!
//! Tests never touch the real Ace ID service and never read or write the user's
//! real `~/.config/aidc` directory.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Path of the binary under test.
pub const BIN: &str = env!("CARGO_BIN_EXE_aid");

#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }

    /// Parsed form body (`application/x-www-form-urlencoded`).
    pub fn form(&self) -> Vec<(String, String)> {
        self.body
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| match pair.split_once('=') {
                Some((key, value)) => (decode(key), decode(value)),
                None => (decode(pair), String::new()),
            })
            .collect()
    }

    pub fn form_value(&self, key: &str) -> Option<String> {
        self.form()
            .into_iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
}

fn decode(value: &str) -> String {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                match u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

#[derive(Clone, Debug)]
pub struct Route {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub content_type: String,
    pub body: String,
    pub headers: Vec<(String, String)>,
    pub delay: Duration,
}

impl Route {
    pub fn json(method: &str, path: &str, status: u16, body: &str) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            status,
            content_type: "application/json".to_string(),
            body: body.to_string(),
            headers: Vec::new(),
            delay: Duration::ZERO,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

pub struct MockServer {
    pub base: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockServer {
    pub fn start(routes: Vec<Route>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let routes = Arc::new(routes);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_routes = Arc::clone(&routes);
        let thread_requests = Arc::clone(&requests);
        let thread_stop = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        handle_connection(stream, &thread_routes, &thread_requests);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base: format!("http://127.0.0.1:{port}"),
            requests,
            stop,
            handle: Some(handle),
        }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }

    pub fn requests_to(&self, method: &str, path: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|request| {
                request.method == method
                    && request
                        .target
                        .split('?')
                        .next()
                        .map(|target| target == path)
                        .unwrap_or(false)
            })
            .collect()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    routes: &Arc<Vec<Route>>,
    requests: &Arc<Mutex<Vec<Recorded>>>,
) {
    stream.set_nonblocking(false).ok();
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let Some(request) = read_request(&mut stream) else {
        return;
    };

    if let Ok(mut log) = requests.lock() {
        log.push(request.clone());
    }

    let path = request.target.split('?').next().unwrap_or("").to_string();
    let route = routes.iter().find(|route| {
        (route.method.eq_ignore_ascii_case(&request.method) || route.method == "*")
            && route.path == path
    });

    let Some(route) = route else {
        let body = format!("no route for {} {}", request.method, path);
        respond(&mut stream, 404, "text/plain", &body, &[]);
        return;
    };

    if !route.delay.is_zero() {
        std::thread::sleep(route.delay);
    }
    respond(
        &mut stream,
        route.status,
        &route.content_type,
        &route.body,
        &route.headers,
    );
}

fn read_request(stream: &mut TcpStream) -> Option<Recorded> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if buffer.len() > 64 * 1024 {
            break;
        }
    }
    let head_end = buffer.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }

    let mut body_bytes = buffer[head_end..].to_vec();
    while body_bytes.len() < content_length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        body_bytes.extend_from_slice(&chunk[..read]);
    }

    Some(Recorded {
        method,
        target,
        headers,
        body: String::from_utf8_lossy(&body_bytes).to_string(),
    })
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
    headers: &[(String, String)],
) {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(body);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// An isolated project directory plus an isolated config directory.
pub struct Env {
    pub dir: PathBuf,
    pub config_home: PathBuf,
}

impl Env {
    pub fn new(tag: &str) -> Self {
        let mut random = [0u8; 6];
        getrandom::getrandom(&mut random).expect("random");
        let dir = std::env::temp_dir().join(format!(
            "ace-id-it-{tag}-{}-{}",
            std::process::id(),
            hex(&random)
        ));
        std::fs::create_dir_all(&dir).expect("create env dir");
        let config_home = dir.join(".config");
        std::fs::create_dir_all(&config_home).expect("create config dir");
        Self { dir, config_home }
    }

    pub fn write(&self, relative: &str, content: &str) {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, content).expect("write fixture");
    }

    pub fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.dir.join(relative)).expect("read fixture")
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.dir.join(relative).exists()
    }

    pub fn tokens_path(&self) -> PathBuf {
        self.config_home.join("aidc").join("tokens.json")
    }

    /// Seed a stored session so commands start signed in.
    pub fn seed_tokens(&self, access_token: &str, expires_in_ms: i64, refresh_token: Option<&str>) {
        let dir = self.config_home.join("aidc");
        std::fs::create_dir_all(&dir).expect("create token dir");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        let mut document = serde_json::json!({
            "access_token": access_token,
            "token_type": "Bearer",
            "expires_at": now + expires_in_ms,
        });
        if let Some(refresh_token) = refresh_token {
            document["refresh_token"] = serde_json::Value::String(refresh_token.to_string());
        }
        std::fs::write(
            dir.join("tokens.json"),
            serde_json::to_string_pretty(&document).expect("serialise tokens"),
        )
        .expect("seed tokens");
    }

    pub fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        run_in(&self.dir, &self.config_home, args, extra_env)
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Build a `Command` for the CLI with an isolated, deterministic environment.
pub fn command(project_dir: &Path, config_home: &Path, extra_env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(BIN);
    command
        .current_dir(project_dir)
        .env("XDG_CONFIG_HOME", config_home)
        .env("HOME", project_dir)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("AIDC_ISSUER", "")
        .env("AIDC_CLIENT_ID", "")
        .env("AIDC_PORT", "")
        .env("AIDC_SCOPES", "")
        .env("AIDC_REQUEST_TIMEOUT_MS", "")
        .env("AIDC_REFRESH_LEEWAY_MS", "")
        .env("AIDC_CALLBACK_TIMEOUT_MS", "")
        .env("BROWSER", "true")
        .env_remove("APPDATA");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command
}

pub fn run_in(
    project_dir: &Path,
    config_home: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Output {
    command(project_dir, config_home, extra_env)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run aid")
}

pub fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Pick a free loopback port for the OAuth callback listener.
pub fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    port
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
