use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{fs, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    process::Command,
    time::timeout,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::test]
async fn real_binary_saves_pending_vehicle_progress_on_launcher_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let executable = std::env::var_os("CHAO_TEST_SERVER_EXE")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_chao-game-server").into());
    let mut command = Command::new(executable);
    command
        .env("CHAO_DATA_DIR", directory.path())
        .env("GAME_HOST", "127.0.0.1")
        .env("GAME_PORT", "0")
        .env("CHAO_OWNER_BOOTSTRAP", "1")
        .env("CHAO_ALLOWED_ORIGINS", "")
        .env("CHAO_SHUTDOWN_STDIN", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut startup = String::new();
    assert!(
        timeout(Duration::from_secs(5), output.read_line(&mut startup))
            .await
            .unwrap()
            .unwrap()
            > 0,
        "binary exited before announcing its listener"
    );
    let address = startup
        .trim()
        .strip_prefix("Chao Rust game server: http://")
        .expect("server startup address");
    let mut connection = TcpStream::connect(address).await.unwrap();
    let body = json!({"name":"Process owner"}).to_string();
    connection.write_all(format!("POST /api/session HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    let mut response = vec![];
    timeout(
        Duration::from_secs(5),
        connection.read_to_end(&mut response),
    )
    .await
    .unwrap()
    .unwrap();
    let response = String::from_utf8(response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("HTTP/1.1 200"));
    let session: Value = serde_json::from_str(body).unwrap();
    let url = format!(
        "ws://{address}/ws?token={}",
        session["token"].as_str().unwrap()
    );
    let (mut socket, _) = connect_async(url).await.unwrap();
    socket
        .send(Message::Text(
            json!({"type":"action", "action":"mount-dark-walker"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let message: Value = serde_json::from_str(&text).unwrap();
                    if message["type"] == "snapshot"
                        && message["players"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|player| player["vehicle"] == "dark-chao-walker")
                    {
                        break;
                    }
                }
                Some(Err(error)) => panic!("server socket failed: {error}"),
                None | Some(Ok(Message::Close(_))) => panic!("server closed before mount snapshot"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    for item in ["shadow-air-shoes", "knuckles-hammer-gloves"] {
        tokio::time::sleep(Duration::from_millis(100)).await;
        socket
            .send(Message::Text(
                json!({"type":"action","action":"equip-owner-gear","item":item})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
    }
    timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let message: Value = serde_json::from_str(&text).unwrap();
                    if message["type"] == "snapshot"
                        && message["players"][0]["gear"]["footwear"] == "shadow-air-shoes"
                        && message["players"][0]["gear"]["hands"] == "knuckles-hammer-gloves"
                    {
                        break;
                    }
                }
                Some(Err(error)) => panic!("server socket failed during gear selection: {error}"),
                None | Some(Ok(Message::Close(_))) => panic!("server closed before gear snapshot"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    socket
        .send(Message::Text(
            json!({"type":"input","boost":true,"y":9999,"boostEnergy":999999})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let message: Value = serde_json::from_str(&text).unwrap();
                    if message["type"] == "snapshot"
                        && message["players"][0]["mech"]["boosting"] == true
                    {
                        assert!(
                            message["players"][0]["mech"]["boostEnergy"]
                                .as_f64()
                                .unwrap()
                                < 100.
                        );
                        assert!(message["players"][0]["jumpOffset"].as_f64().unwrap() <= 3.);
                        break;
                    }
                }
                Some(Err(error)) => panic!("server socket failed during thrust: {error}"),
                None | Some(Ok(Message::Close(_))) => {
                    panic!("server closed before thrust snapshot")
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    socket
        .send(Message::Text(
            json!({"type":"action","action":"mech-fire","damage":999999})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    timeout(Duration::from_secs(4), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) => {
                    let message: Value = serde_json::from_str(&text).unwrap();
                    if message["kind"] == "mech-shot" {
                        assert_eq!(message["hit"], false);
                        assert_eq!(message["targetId"], Value::Null);
                        break;
                    }
                }
                Some(Err(error)) => panic!("server socket failed during cannon fire: {error}"),
                None | Some(Ok(Message::Close(_))) => panic!("server closed before cannon event"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"shutdown\n")
        .await
        .unwrap();
    let status = timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(
        status.success(),
        "server failed during graceful shutdown: {status}"
    );
    let profiles: Value =
        serde_json::from_slice(&fs::read(directory.path().join("profiles.json")).unwrap()).unwrap();
    assert_eq!(profiles[0]["vehicle"], "dark-chao-walker");
    assert_eq!(profiles[0]["admin"], true);
    assert_eq!(
        profiles[0]["gear"],
        json!({"footwear":"shadow-air-shoes","hands":"knuckles-hammer-gloves"})
    );
    assert!(profiles[0].get("boostEnergy").is_none());
    assert!(!directory.path().join("profiles.json.tmp").exists());
}
