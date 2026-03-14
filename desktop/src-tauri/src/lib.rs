use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter, Manager};

fn ytdlp_path(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("no app data dir");
    std::fs::create_dir_all(&dir).ok();
    #[cfg(windows)]
    return dir.join("yt-dlp.exe");
    #[cfg(not(windows))]
    return dir.join("yt-dlp");
}

fn ytdlp_url() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos";
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64";
    #[cfg(target_os = "windows")]
    return "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
}

async fn do_download_ytdlp(app: &AppHandle) -> Result<(), String> {
    let path = ytdlp_path(app);
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(ytdlp_url())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut bytes = Vec::new();

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
        let pct = if total > 0 {
            (downloaded * 100 / total) as u32
        } else {
            0
        };
        app.emit("ytdlp://progress", pct).ok();
    }

    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    }

    app.emit("ytdlp://progress", 100u32).ok();
    Ok(())
}

#[tauri::command]
async fn download_ytdlp(app: AppHandle) -> Result<(), String> {
    do_download_ytdlp(&app).await
}

#[derive(Serialize)]
pub struct YtdlpInfo {
    installed: bool,
    version: Option<String>,
    latest: Option<String>,
    has_update: bool,
}

#[tauri::command]
async fn get_ytdlp_info(app: AppHandle) -> Result<YtdlpInfo, String> {
    let path = ytdlp_path(&app);
    if !path.exists() {
        return Ok(YtdlpInfo {
            installed: false,
            version: None,
            latest: None,
            has_update: false,
        });
    }

    let version_out = tokio::task::spawn_blocking(move || {
        Command::new(&path).arg("--version").output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let version = String::from_utf8_lossy(&version_out.stdout)
        .trim()
        .to_string();
    let version = if version.is_empty() {
        None
    } else {
        Some(version)
    };

    let latest = fetch_latest_tag().await.ok();

    let has_update = match (&version, &latest) {
        (Some(v), Some(l)) => v != l,
        _ => false,
    };

    Ok(YtdlpInfo {
        installed: true,
        version,
        latest,
        has_update,
    })
}

async fn fetch_latest_tag() -> Result<String, String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    let client = Client::builder()
        .user_agent("snap-app")
        .build()
        .map_err(|e| e.to_string())?;
    let release: Release = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Release>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(release.tag_name)
}

#[tauri::command]
async fn update_ytdlp(app: AppHandle) -> Result<(), String> {
    let path = ytdlp_path(&app);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    do_download_ytdlp(&app).await
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    line: String,
}

#[derive(Clone, Serialize)]
struct MediaDone {
    path: String,
}

#[derive(Clone, Serialize)]
struct MediaError {
    error: String,
}

#[tauri::command]
async fn download_media(app: AppHandle, url: String, mode: String) -> Result<(), String> {
    let ytdlp = ytdlp_path(&app);
    if !ytdlp.exists() {
        return Err("yt-dlp not found. Please run setup first.".into());
    }

    let downloads_dir = app.path().download_dir().map_err(|e| e.to_string())?;

    let mut args: Vec<String> = vec![
        "--no-simulate".into(),
        "--print".into(),
        "after_move:filepath".into(),
        "--paths".into(),
        downloads_dir.to_string_lossy().to_string(),
        "-o".into(),
        "%(title)s.%(ext)s".into(),
        "--newline".into(),
    ];

    if mode == "audio" {
        args.extend([
            "-x".into(),
            "--audio-format".into(),
            "mp3".into(),
            "--audio-quality".into(),
            "0".into(),
        ]);
    } else {
        args.extend([
            "-f".into(),
            "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best".into(),
            "--merge-output-format".into(),
            "mp4".into(),
        ]);
    }

    args.push(url);

    // Return immediately — work runs in a blocking thread, events stream freely
    tokio::task::spawn_blocking(move || {
        let result = run_ytdlp(&app, &ytdlp, &args);
        match result {
            Ok(path) => { app.emit("media://done", MediaDone { path }).ok(); }
            Err(e) => { app.emit("media://error", MediaError { error: e }).ok(); }
        }
    });

    Ok(())
}

fn run_ytdlp(app: &AppHandle, ytdlp: &std::path::Path, args: &[String]) -> Result<String, String> {
    let mut child = Command::new(ytdlp)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let stderr = child.stderr.take().unwrap();
    let app_stderr = app.clone();

    // Drain stderr in a thread so FFmpeg never blocks on the pipe buffer
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            if let Ok(l) = line {
                let trimmed = l.trim().to_string();
                if !trimmed.is_empty() {
                    app_stderr.emit("media://progress", DownloadProgress { line: trimmed }).ok();
                }
            }
        }
    });

    let mut final_path = String::new();

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let line = line.map_err(|e| e.to_string())?;
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                if std::path::Path::new(&trimmed).exists() {
                    final_path = trimmed.clone();
                }
                app.emit("media://progress", DownloadProgress { line: trimmed }).ok();
            }
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("yt-dlp exited with error. Check the log above.".into());
    }

    Ok(final_path)
}

#[tauri::command]
async fn reveal_file(path: String) -> Result<(), String> {
    showfile::show_path_in_file_manager(path);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            download_ytdlp,
            get_ytdlp_info,
            update_ytdlp,
            download_media,
            reveal_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
