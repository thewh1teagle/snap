import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface YtdlpInfo {
  installed: boolean;
  version: string | null;
  latest: string | null;
  has_update: boolean;
}

interface ProgressEvent {
  line: string;
}

export default function App() {
  const navigate = useNavigate();
  const [url, setUrl] = useState("");
  const [mode, setMode] = useState<"video" | "audio">("video");
  const [status, setStatus] = useState<"idle" | "downloading" | "done" | "error">("idle");
  const [log, setLog] = useState<string[]>([]);
  const [savedPath, setSavedPath] = useState("");
  const [error, setError] = useState("");
  const [info, setInfo] = useState<YtdlpInfo | null>(null);
  const [updating, setUpdating] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const downloadingRef = useRef(false);

  useEffect(() => {
    invoke<YtdlpInfo>("get_ytdlp_info").then((i) => {
      if (!i.installed) {
        navigate("/setup");
      } else {
        setInfo(i);
      }
    });
  }, []);

  useEffect(() => {
    const unlistenProgress = listen<ProgressEvent>("media://progress", (e) => {
      setLog((prev) => [...prev, e.payload.line]);
      setTimeout(() => {
        logRef.current?.scrollTo(0, logRef.current.scrollHeight);
      }, 0);
    });
    const unlistenDone = listen<{ path: string }>("media://done", (e) => {
      setSavedPath(e.payload.path);
      setStatus("done");
      downloadingRef.current = false;
    });
    const unlistenError = listen<{ error: string }>("media://error", (e) => {
      setError(e.payload.error);
      setStatus("error");
      downloadingRef.current = false;
    });
    return () => {
      unlistenProgress.then((f) => f());
      unlistenDone.then((f) => f());
      unlistenError.then((f) => f());
    };
  }, []);

  async function handleDownload() {
    if (!url.trim() || downloadingRef.current) return;
    downloadingRef.current = true;
    setStatus("downloading");
    setLog([]);
    setSavedPath("");
    setError("");
    try {
      await invoke("download_media", { url: url.trim(), mode });
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }

  async function handleUpdate() {
    setUpdating(true);
    try {
      await invoke("update_ytdlp");
      const i = await invoke<YtdlpInfo>("get_ytdlp_info");
      setInfo(i);
    } finally {
      setUpdating(false);
    }
  }

  const mediaSrc = savedPath ? convertFileSrc(savedPath) : null;

  return (
    <div className="page">
      <div className="header">
        <h1>snap</h1>
        {info?.has_update && (
          <button className="update-badge" onClick={handleUpdate} disabled={updating}>
            {updating ? "Updating..." : `Update yt-dlp → ${info.latest}`}
          </button>
        )}
      </div>

      <div className="main-box">
        <input
          className="url-input"
          type="url"
          placeholder="Paste video or audio URL..."
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleDownload()}
        />

        <div className="mode-toggle">
          <button className={mode === "video" ? "active" : ""} onClick={() => setMode("video")}>
            Video
          </button>
          <button className={mode === "audio" ? "active" : ""} onClick={() => setMode("audio")}>
            Audio
          </button>
        </div>

        <button
          className="download-btn"
          onClick={handleDownload}
          disabled={status === "downloading" || !url.trim()}
        >
          {status === "downloading" ? "Downloading..." : "Download"}
        </button>
      </div>

      {log.length > 0 && (
        <div className="log" ref={logRef}>
          {log.map((l, i) => (
            <div key={i} className="log-line">
              {l}
            </div>
          ))}
        </div>
      )}

      {status === "done" && savedPath && (
        <div className="result">
          <button className="path-btn" onClick={() => invoke("reveal_file", { path: savedPath })} title="Reveal in Finder">
            📂 {savedPath}
          </button>
          {mediaSrc && mode === "audio" && <audio controls src={mediaSrc} className="preview" />}
          {mediaSrc && mode === "video" && (
            <>
              <video ref={videoRef} controls src={mediaSrc} className="preview" />
              <button className="fullscreen-btn" onClick={() => (videoRef.current as any)?.webkitRequestFullscreen()}>
                ⛶ Fullscreen
              </button>
            </>
          )}
        </div>
      )}

      {status === "error" && <p className="err">{error}</p>}
    </div>
  );
}
