//! Transparent Codex App Server transport used by the adaptive agent mode.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::error::AppError;

const APP_SERVER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RELAY_POLL: Duration = Duration::from_millis(8);

pub(crate) trait MessageInterceptor {
    fn negotiated(&mut self, _capabilities: &NegotiatedCapabilities) -> Result<(), AppError> {
        Ok(())
    }

    fn client_message(&mut self, message: &mut Value) -> Result<Vec<Value>, AppError>;

    fn server_message(&mut self, message: &Value) -> Result<Vec<Value>, AppError>;
}

pub(crate) struct ProcessConfig {
    pub binary: PathBuf,
    pub working_directory: PathBuf,
    pub profile: Option<String>,
    pub tui_args: Vec<OsString>,
    pub verbose: bool,
    pub codex_version: String,
}

#[derive(Clone, Debug)]
pub(crate) struct NegotiatedCapabilities {
    pub model_count: usize,
    pub collaboration_mode_count: usize,
    pub experimental_feature_count: usize,
    pub namespace_tools: bool,
    pub image_generation: bool,
    pub web_search: bool,
    pub model_catalog: crate::codex::catalog::ModelCatalog,
}

struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child }
    }

    fn wait(&mut self) -> Result<std::process::ExitStatus, AppError> {
        self.child
            .wait()
            .map_err(|error| AppError::AppServer(format!("failed waiting for Codex TUI: {error}")))
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn app_error(context: &str, error: impl std::fmt::Display) -> AppError {
    AppError::AppServer(format!("{context}: {error}"))
}

fn reserve_endpoint() -> Result<(TcpListener, String), AppError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| app_error("failed to reserve local proxy port", error))?;
    let address = listener
        .local_addr()
        .map_err(|error| app_error("failed to read local proxy address", error))?;
    Ok((listener, format!("ws://{address}")))
}

fn unused_endpoint() -> Result<String, AppError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| app_error("failed to reserve App Server port", error))?;
    let address = listener
        .local_addr()
        .map_err(|error| app_error("failed to read App Server address", error))?;
    drop(listener);
    Ok(format!("ws://{address}"))
}

fn spawn_app_server(config: &ProcessConfig, endpoint: &str) -> Result<ChildGuard, AppError> {
    let mut command = Command::new(&config.binary);
    if let Some(profile) = &config.profile {
        command.arg("--profile").arg(profile);
    }
    command
        .arg("app-server")
        .arg("--listen")
        .arg(endpoint)
        .current_dir(&config.working_directory)
        .env("CAUTO_ACTIVE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(if config.verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        });
    let child = command.spawn().map_err(|source| AppError::LaunchFailed {
        path: config.binary.clone(),
        source,
    })?;
    Ok(ChildGuard::new(child))
}

fn connect_with_retry(endpoint: &str) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, AppError> {
    let started = Instant::now();
    loop {
        match tungstenite::connect(endpoint) {
            Ok((socket, _)) => return Ok(socket),
            Err(_) if started.elapsed() < APP_SERVER_CONNECT_TIMEOUT => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(app_error("App Server did not become ready", error)),
        }
    }
}

fn send_json<S: Read + Write>(socket: &mut WebSocket<S>, value: &Value) -> Result<(), AppError> {
    let text =
        serde_json::to_string(value).map_err(|error| AppError::Serialization(error.to_string()))?;
    socket
        .send(Message::Text(text.into()))
        .map_err(|error| app_error("failed to write App Server message", error))
}

fn request<S: Read + Write>(
    socket: &mut WebSocket<S>,
    id: &str,
    method: &str,
    params: Value,
) -> Result<Value, AppError> {
    send_json(
        socket,
        &json!({
            "method": method,
            "id": id,
            "params": params,
        }),
    )?;
    loop {
        let message = socket
            .read()
            .map_err(|error| app_error("failed to read App Server response", error))?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(text.as_str())
            .map_err(|error| app_error("App Server returned invalid JSON", error))?;
        if value.get("id").and_then(Value::as_str) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(AppError::AppServer(format!(
                "{method} was rejected: {}",
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown protocol error")
            )));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| AppError::AppServer(format!("{method} returned no result")));
    }
}

fn array_len(result: &Value, method: &str) -> Result<usize, AppError> {
    result
        .get("data")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(|| AppError::AppServer(format!("{method} returned an invalid data array")))
}

fn request_all_models<S: Read + Write>(socket: &mut WebSocket<S>) -> Result<Value, AppError> {
    let mut data = Vec::new();
    let mut cursor: Option<String> = None;
    for page in 0..10 {
        let mut params = json!({ "limit": 200, "includeHidden": true });
        if let Some(cursor) = &cursor {
            params["cursor"] = Value::String(cursor.clone());
        }
        let result = request(
            socket,
            &format!("cauto:models:{page}"),
            "model/list",
            params,
        )?;
        let page_data = result
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::AppServer("model/list returned invalid data".into()))?;
        data.extend(page_data.iter().cloned());
        cursor = result
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if cursor.is_none() {
            return Ok(json!({ "data": data }));
        }
    }
    Err(AppError::AppServer(
        "model/list exceeded the 10-page safety bound".into(),
    ))
}

pub(crate) fn negotiate(
    endpoint: &str,
    codex_version: &str,
) -> Result<NegotiatedCapabilities, AppError> {
    let mut socket = connect_with_retry(endpoint)?;
    let _ = request(
        &mut socket,
        "cauto:initialize",
        "initialize",
        json!({
            "clientInfo": {
                "name": "cauto",
                "title": "cauto adaptive router",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "experimentalApi": true,
            },
        }),
    )?;
    send_json(
        &mut socket,
        &json!({ "method": "initialized", "params": {} }),
    )?;
    let models = request_all_models(&mut socket)?;
    let collaboration_modes = request(
        &mut socket,
        "cauto:collaboration-modes",
        "collaborationMode/list",
        json!({}),
    )?;
    let provider = request(
        &mut socket,
        "cauto:provider-capabilities",
        "modelProvider/capabilities/read",
        json!({}),
    )?;
    let features = request(
        &mut socket,
        "cauto:experimental-features",
        "experimentalFeature/list",
        json!({ "limit": 200 }),
    )?;
    let _ = socket.close(None);
    let model_catalog =
        crate::codex::catalog::parse_app_server_models(models.clone(), codex_version)
            .map_err(|error| AppError::CatalogParse(error.to_string()))?;
    Ok(NegotiatedCapabilities {
        model_count: array_len(&models, "model/list")?,
        collaboration_mode_count: array_len(&collaboration_modes, "collaborationMode/list")?,
        experimental_feature_count: array_len(&features, "experimentalFeature/list")?,
        namespace_tools: provider
            .get("namespaceTools")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        image_generation: provider
            .get("imageGeneration")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        web_search: provider
            .get("webSearch")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        model_catalog,
    })
}

fn spawn_tui(config: &ProcessConfig, endpoint: &str) -> Result<ChildGuard, AppError> {
    let mut command = Command::new(&config.binary);
    if let Some(profile) = &config.profile {
        command.arg("--profile").arg(profile);
    }
    command
        .arg("--remote")
        .arg(endpoint)
        .args(&config.tui_args)
        .current_dir(&config.working_directory)
        .env("CAUTO_ACTIVE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let child = command.spawn().map_err(|source| AppError::LaunchFailed {
        path: config.binary.clone(),
        source,
    })?;
    Ok(ChildGuard::new(child))
}

fn accept_tui(
    listener: &TcpListener,
    tui: &mut ChildGuard,
) -> Result<WebSocket<TcpStream>, AppError> {
    listener
        .set_nonblocking(true)
        .map_err(|error| app_error("failed to configure proxy listener", error))?;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                configure_accepted_tui_stream(&stream)?;
                let mut socket = tungstenite::accept(stream)
                    .map_err(|error| app_error("Codex TUI WebSocket handshake failed", error))?;
                socket
                    .get_mut()
                    .set_read_timeout(Some(RELAY_POLL))
                    .map_err(|error| app_error("failed to configure TUI socket", error))?;
                return Ok(socket);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(status) = tui
                    .child
                    .try_wait()
                    .map_err(|error| app_error("failed to inspect Codex TUI", error))?
                {
                    return Err(AppError::AppServer(format!(
                        "Codex TUI exited before connecting to the proxy ({status})"
                    )));
                }
                // Codex can present update, authentication, or other preflight
                // prompts before it opens the remote connection. Keep waiting
                // while the visible TUI child is alive so the user controls
                // how long those interactive steps take.
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(app_error("failed accepting Codex TUI connection", error)),
        }
    }
}

fn configure_accepted_tui_stream(stream: &TcpStream) -> Result<(), AppError> {
    // Darwin can propagate O_NONBLOCK from the listener to accepted sockets.
    // Relay reads use a timeout for cooperative polling, but writes must retain
    // normal blocking backpressure instead of surfacing a transient EAGAIN as
    // a fatal App Server forwarding error.
    stream
        .set_nonblocking(false)
        .map_err(|error| app_error("failed to configure accepted TUI socket", error))
}

fn idle_error(error: &tungstenite::Error) -> bool {
    matches!(
        error,
        tungstenite::Error::Io(source)
            if matches!(
                source.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
    )
}

fn transform_client<I: MessageInterceptor>(
    message: Message,
    interceptor: &mut I,
) -> Result<(Message, Vec<Value>), AppError> {
    let Message::Text(text) = message else {
        return Ok((message, Vec::new()));
    };
    let Ok(mut value) = serde_json::from_str::<Value>(text.as_str()) else {
        return Ok((Message::Text(text), Vec::new()));
    };
    let synthetic = interceptor.client_message(&mut value)?;
    let encoded = serde_json::to_string(&value)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok((Message::Text(encoded.into()), synthetic))
}

fn inspect_server<I: MessageInterceptor>(
    message: &Message,
    interceptor: &mut I,
) -> Result<Vec<Value>, AppError> {
    let Message::Text(text) = message else {
        return Ok(Vec::new());
    };
    let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
        return Ok(Vec::new());
    };
    interceptor.server_message(&value)
}

fn configure_target_timeout(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> Result<(), AppError> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream
            .set_read_timeout(Some(RELAY_POLL))
            .map_err(|error| app_error("failed to configure App Server socket", error)),
        _ => Err(AppError::AppServer(
            "unexpected TLS transport for the local App Server".into(),
        )),
    }
}

fn relay<I: MessageInterceptor>(
    mut client: WebSocket<TcpStream>,
    mut target: WebSocket<MaybeTlsStream<TcpStream>>,
    interceptor: &mut I,
) -> Result<(), AppError> {
    configure_target_timeout(&mut target)?;
    loop {
        let mut progressed = false;
        match client.read() {
            Ok(message) => {
                progressed = true;
                let closing = matches!(message, Message::Close(_));
                let (message, synthetic) = transform_client(message, interceptor)?;
                target
                    .send(message)
                    .map_err(|error| app_error("failed forwarding TUI request", error))?;
                for value in synthetic {
                    send_json(&mut client, &value)?;
                }
                if closing {
                    return Ok(());
                }
            }
            Err(error) if idle_error(&error) => {}
            Err(tungstenite::Error::ConnectionClosed) => return Ok(()),
            Err(error) => return Err(app_error("failed reading Codex TUI message", error)),
        }
        match target.read() {
            Ok(message) => {
                progressed = true;
                let closing = matches!(message, Message::Close(_));
                let inspectable = message.clone();
                client
                    .send(message)
                    .map_err(|error| app_error("failed forwarding App Server event", error))?;
                let synthetic = inspect_server(&inspectable, interceptor)?;
                for value in synthetic {
                    send_json(&mut client, &value)?;
                }
                if closing {
                    return Ok(());
                }
            }
            Err(error) if idle_error(&error) => {}
            Err(tungstenite::Error::ConnectionClosed) => return Ok(()),
            Err(error) => return Err(app_error("failed reading App Server event", error)),
        }
        if !progressed {
            thread::sleep(Duration::from_millis(2));
        }
    }
}

pub(crate) fn run<I: MessageInterceptor>(
    config: ProcessConfig,
    interceptor: &mut I,
) -> Result<(ExitCode, NegotiatedCapabilities), AppError> {
    let (proxy_listener, proxy_endpoint) = reserve_endpoint()?;
    let target_endpoint = unused_endpoint()?;
    let _server = spawn_app_server(&config, &target_endpoint)?;
    let capabilities = negotiate(&target_endpoint, &config.codex_version)?;
    interceptor.negotiated(&capabilities)?;
    let mut tui = spawn_tui(&config, &proxy_endpoint)?;
    let client = accept_tui(&proxy_listener, &mut tui)?;
    let target = connect_with_retry(&target_endpoint)?;
    relay(client, target, interceptor)?;
    let status = tui.wait()?;
    let code = status
        .code()
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(1);
    Ok((ExitCode::from(code), capabilities))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorder;

    impl MessageInterceptor for Recorder {
        fn client_message(&mut self, message: &mut Value) -> Result<Vec<Value>, AppError> {
            message["params"]["model"] = Value::String("gpt-test".into());
            Ok(vec![json!({
                "method": "warning",
                "params": { "threadId": null, "message": "routed" }
            })])
        }

        fn server_message(&mut self, _message: &Value) -> Result<Vec<Value>, AppError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn text_requests_can_be_rewritten_without_touching_binary_frames() {
        let mut recorder = Recorder;
        let input = Message::Text(
            serde_json::to_string(&json!({
                "method": "turn/start",
                "params": { "model": "old" }
            }))
            .unwrap()
            .into(),
        );
        let (output, synthetic) = transform_client(input, &mut recorder).unwrap();
        let Message::Text(output) = output else {
            panic!("expected text");
        };
        let value: Value = serde_json::from_str(output.as_str()).unwrap();
        assert_eq!(value["params"]["model"], "gpt-test");
        assert_eq!(synthetic.len(), 1);

        let binary = Message::Binary(vec![1, 2, 3].into());
        let (output, synthetic) = transform_client(binary.clone(), &mut recorder).unwrap();
        assert_eq!(output, binary);
        assert!(synthetic.is_empty());
    }

    #[test]
    fn accepted_tui_stream_is_restored_to_blocking_mode() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || TcpStream::connect(address).unwrap());
        let (mut stream, _) = listener.accept().unwrap();
        let _client = client.join().unwrap();

        stream.set_nonblocking(true).unwrap();
        configure_accepted_tui_stream(&stream).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(40)))
            .unwrap();

        let started = Instant::now();
        let error = stream.read(&mut [0_u8; 1]).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));
        assert!(
            started.elapsed() >= Duration::from_millis(10),
            "configured stream returned immediately instead of waiting for its read timeout"
        );
    }
}
