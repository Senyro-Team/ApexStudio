#![cfg(windows)]
#![allow(clippy::unwrap_used)]
#[allow(dead_code)]
#[path = "../src/chrome_protocol.rs"]
mod protocol;
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn windows_host_forwards_bounded_frames_without_opening_a_workspace() {
    use std::os::windows::process::CommandExt;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::windows::named_pipe::ServerOptions,
    };
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("ApexStudio").join("chrome");
    std::fs::create_dir_all(&settings).unwrap();
    let pipe = format!(r"\\.\pipe\ApexStudio-test-{}", uuid::Uuid::now_v7());
    let origin = format!("chrome-extension://{}/", "a".repeat(32));
    std::fs::write(
        settings.join("settings.json"),
        serde_json::to_vec(&protocol::ChromeConfig {
            chrome_path: String::new(),
            user_data_dir: String::new(),
            profile: "Default".into(),
            extension_id: "a".repeat(32),
            pipe: pipe.clone(),
        })
        .unwrap(),
    )
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut server = {
        let _entered = runtime.enter();
        ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(pipe)
            .unwrap()
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_apex-chrome-bridge"))
        .arg(&origin)
        .env("LOCALAPPDATA", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    protocol::write_frame(
        &mut input,
        br#"{"kind":"chunk","session":"test","offset":0}"#,
    )
    .unwrap();
    let payload = serde_json::to_vec(
        &serde_json::json!({"data":"A".repeat(256*1024),"offset":192*1024,"done":false}),
    )
    .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            server.connect().await.unwrap();
            let length = server.read_u32_le().await.unwrap();
            let mut bytes = vec![0; length as usize];
            server.read_exact(&mut bytes).await.unwrap();
            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(request["origin"], origin);
            assert_eq!(request["kind"], "chunk");
            server.write_u32_le(payload.len() as u32).await.unwrap();
            server.write_all(&payload).await.unwrap();
            server.flush().await.unwrap();
        })
        .await
        .unwrap();
    });
    let response =
        protocol::read_frame(&mut child.stdout.take().unwrap(), protocol::MAX_FRAME).unwrap();
    assert_eq!(response, payload);
    input.flush().unwrap();
    drop(input);
    assert!(child.wait().unwrap().success());
    assert!(!dir.path().join("workspace.sqlite3").exists());
}
