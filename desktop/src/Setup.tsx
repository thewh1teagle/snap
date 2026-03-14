import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export default function Setup() {
  const navigate = useNavigate();
  const [progress, setProgress] = useState(0);
  const [status, setStatus] = useState<"idle" | "downloading" | "done" | "error">("idle");
  const [error, setError] = useState("");

  useEffect(() => {
    const unlisten = listen<number>("ytdlp://progress", (e) => {
      setProgress(e.payload);
    });
    return () => { unlisten.then(f => f()); };
  }, []);

  async function startDownload() {
    setStatus("downloading");
    setProgress(0);
    try {
      await invoke("download_ytdlp");
      setStatus("done");
      setTimeout(() => navigate("/"), 800);
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  }

  return (
    <div className="page">
      <h1>snap</h1>
      <p className="subtitle">Fast video &amp; audio downloader</p>

      <div className="setup-box">
        <p>yt-dlp is required to download media. It will be saved to your app data folder.</p>

        {status === "idle" && (
          <button onClick={startDownload}>Download yt-dlp</button>
        )}

        {status === "downloading" && (
          <div className="progress-wrap">
            <div className="progress-bar">
              <div className="progress-fill" style={{ width: `${progress}%` }} />
            </div>
            <span className="progress-label">{progress}%</span>
          </div>
        )}

        {status === "done" && <p className="ok">Done! Redirecting...</p>}

        {status === "error" && (
          <>
            <p className="err">{error}</p>
            <button onClick={startDownload}>Retry</button>
          </>
        )}
      </div>
    </div>
  );
}
