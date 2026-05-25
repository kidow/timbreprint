import React from "react";
import ReactDOM from "react-dom/client";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  Copy,
  FileAudio,
  FolderOpen,
  Loader2,
  Settings,
  Terminal,
} from "lucide-react";
import "./styles.css";

type Confidence = "낮음" | "중간" | "높음";
type JobStatus =
  | "idle"
  | "selected"
  | "preprocessing"
  | "analyzing"
  | "prompting"
  | "completed"
  | "failed"
  | "cancelled";

type ScoredValue = {
  value: string;
  confidence: number;
};

type NumericScoredValue = {
  value: number;
  confidence: number;
};

type Analysis = {
  tempo: NumericScoredValue;
  key: ScoredValue;
  energy: ScoredValue;
  mood: ScoredValue[];
  genre: ScoredValue[];
  instruments: ScoredValue[];
  texture: ScoredValue[];
  features?: {
    analysisBackend?: string;
    durationSeconds?: number;
    rms?: number;
    zeroCrossingRate?: number;
    spectralCentroidHz?: number;
    onsetDensity?: number;
  };
};

type ToolStatus = {
  ffmpeg: string | null;
  python: string | null;
  appDataDir: string;
  logsDir: string;
};

type JobResult = {
  id: string;
  status: JobStatus;
  sourcePath: string;
  jobDir: string;
  analysis: Analysis;
  prompt: string;
};

const isTauriRuntime = () => "__TAURI_INTERNALS__" in window;

const confidenceLabel = (value: number): Confidence => {
  if (value < 0.5) return "낮음";
  if (value < 0.75) return "중간";
  return "높음";
};

const pillList = (items: ScoredValue[]) =>
  items.map((item) => (
    <span className="pill" key={`${item.value}-${item.confidence}`}>
      {item.value}
      <small>{confidenceLabel(item.confidence)}</small>
    </span>
  ));

function App() {
  const [tools, setTools] = React.useState<ToolStatus | null>(null);
  const [status, setStatus] = React.useState<JobStatus>("idle");
  const [selectedPath, setSelectedPath] = React.useState<string | null>(null);
  const [job, setJob] = React.useState<JobResult | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    void refreshToolStatus();
  }, []);

  const refreshToolStatus = async () => {
    if (!isTauriRuntime()) {
      setTools({
        ffmpeg: null,
        python: null,
        appDataDir: "Tauri runtime required",
        logsDir: "Tauri runtime required",
      });
      return;
    }

    const result = await invoke<ToolStatus>("check_environment");
    setTools(result);
  };

  const selectFile = async () => {
    setError(null);
    if (!isTauriRuntime()) {
      setSelectedPath("/tmp/timbreprint-demo.mp3");
      setStatus("selected");
      setJob(null);
      return;
    }

    const file = await open({
      multiple: false,
      filters: [{ name: "Audio", extensions: ["mp3", "wav", "m4a", "flac"] }],
    });
    if (typeof file === "string") {
      setSelectedPath(file);
      setStatus("selected");
      setJob(null);
    }
  };

  const runAnalysis = async () => {
    if (!selectedPath) return;

    setError(null);
    setStatus("preprocessing");

    try {
      if (!isTauriRuntime()) {
        const result = createBrowserPreviewJob(selectedPath);
        setJob(result);
        setStatus(result.status);
        return;
      }

      const result = await invoke<JobResult>("run_analysis", {
        sourcePath: selectedPath,
      });
      setJob(result);
      setStatus(result.status);
      await refreshToolStatus();
    } catch (err) {
      setStatus("failed");
      setError(err instanceof Error ? err.message : String(err));
    }
};

const createBrowserPreviewJob = (sourcePath: string): JobResult => {
  const analysis: Analysis = {
    tempo: { value: 92, confidence: 0.8 },
    key: { value: "A minor", confidence: 0.6 },
    energy: { value: "medium", confidence: 0.7 },
    mood: [
      { value: "melancholic", confidence: 0.8 },
      { value: "warm", confidence: 0.7 },
      { value: "hopeful", confidence: 0.6 },
    ],
    genre: [
      { value: "indie electronic", confidence: 0.7 },
      { value: "ambient pop", confidence: 0.6 },
    ],
    instruments: [
      { value: "analog synth", confidence: 0.6 },
      { value: "soft drums", confidence: 0.6 },
      { value: "bass pad", confidence: 0.5 },
    ],
    texture: [
      { value: "spacious", confidence: 0.7 },
      { value: "reverb-heavy", confidence: 0.7 },
      { value: "intimate", confidence: 0.6 },
    ],
  };

  return {
    id: "browser-preview",
    status: "completed",
    sourcePath,
    jobDir: "Tauri runtime required",
    analysis,
    prompt:
      "Create a 92 BPM indie electronic, ambient pop track in A minor with analog synth, soft drums, bass pad. The mood should feel melancholic, warm, hopeful, with spacious, reverb-heavy, intimate texture and medium energy.",
  };
};

  const openJobFolder = async () => {
    if (!job) return;
    if (!isTauriRuntime()) return;
    await invoke("open_path", { path: job.jobDir });
  };

  const copyPrompt = async () => {
    if (!job) return;
    await navigator.clipboard.writeText(job.prompt);
  };

  return (
    <main className="app-shell">
      <section className="toolbar">
        <div>
          <p className="eyebrow">Local music analysis</p>
          <h1>Timbreprint</h1>
        </div>
        <button className="icon-button" onClick={refreshToolStatus} title="환경 다시 확인">
          <Settings size={18} />
        </button>
      </section>

      <section className="workspace">
        <div className="input-panel">
          <div className="drop-zone">
            <FileAudio size={30} />
            <div>
              <h2>음악 파일 선택</h2>
              <p>mp3, wav, m4a, flac / 10분 이하 파일을 MVP 입력으로 다룹니다.</p>
            </div>
            <button onClick={selectFile}>파일 선택</button>
          </div>

          {selectedPath ? <p className="selected-path">{selectedPath}</p> : null}

          <div className="actions">
            <button disabled={!selectedPath || status === "preprocessing"} onClick={runAnalysis}>
              {status === "preprocessing" ? <Loader2 className="spin" size={16} /> : <Terminal size={16} />}
              분석 실행
            </button>
            <StatusBadge status={status} />
          </div>

          {error ? <p className="error-text">{error}</p> : null}
        </div>

        <aside className="settings-panel">
          <h2>환경</h2>
          <ToolRow label="ffmpeg" value={tools?.ffmpeg} />
          <ToolRow label="Python" value={tools?.python} />
          <ToolRow label="Data" value={tools?.appDataDir} />
          <ToolRow label="Logs" value={tools?.logsDir} />
        </aside>
      </section>

      {job ? (
        <section className="result-panel">
          <div className="result-header">
            <div>
              <p className="eyebrow">Analysis result</p>
              <h2>{job.id}</h2>
            </div>
            <div className="result-actions">
              <button onClick={copyPrompt}>
                <Copy size={16} />
                프롬프트 복사
              </button>
              <button onClick={openJobFolder}>
                <FolderOpen size={16} />
                결과 폴더 열기
              </button>
            </div>
          </div>

          <div className="summary-grid">
            <Metric label="Tempo" value={`${job.analysis.tempo.value} BPM`} score={job.analysis.tempo.confidence} />
            <Metric label="Key" value={job.analysis.key.value} score={job.analysis.key.confidence} />
            <Metric label="Energy" value={job.analysis.energy.value} score={job.analysis.energy.confidence} />
          </div>

          <div className="tag-grid">
            <TagGroup title="Genre">{pillList(job.analysis.genre)}</TagGroup>
            <TagGroup title="Mood">{pillList(job.analysis.mood)}</TagGroup>
            <TagGroup title="Instruments">{pillList(job.analysis.instruments)}</TagGroup>
            <TagGroup title="Texture">{pillList(job.analysis.texture)}</TagGroup>
          </div>

          <div className="prompt-box">
            <h3>English prompt</h3>
            <p>{job.prompt}</p>
          </div>

          <details className="json-view">
            <summary>JSON 보기</summary>
            <pre>{JSON.stringify(job.analysis, null, 2)}</pre>
          </details>
        </section>
      ) : null}
    </main>
  );
}

function StatusBadge({ status }: { status: JobStatus }) {
  return (
    <span className={`status-badge status-${status}`}>
      {status === "completed" ? <CheckCircle2 size={15} /> : null}
      {status}
    </span>
  );
}

function ToolRow({ label, value }: { label: string; value?: string | null }) {
  return (
    <div className="tool-row">
      <span>{label}</span>
      <code>{value ?? "not found"}</code>
    </div>
  );
}

function Metric({ label, value, score }: { label: string; value: string; score: number }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{confidenceLabel(score)}</small>
    </div>
  );
}

function TagGroup({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="tag-group">
      <h3>{title}</h3>
      <div className="pill-list">{children}</div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
