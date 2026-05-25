import { flushSync } from "react-dom";
import ReactDOM from "react-dom/client";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle2, Copy, FileAudio, FolderOpen, Loader2, Terminal } from "lucide-react";
import { useState, type ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
    <Badge className="tag-pill" key={`${item.value}-${item.confidence}`} variant="secondary">
      {item.value}
      <small>{confidenceLabel(item.confidence)}</small>
    </Badge>
  ));

const inProgressStatuses = new Set<JobStatus>(["preprocessing", "analyzing", "prompting"]);

const statusBadgeVariant = (status: JobStatus) => {
  if (status === "completed") return "success";
  if (status === "failed") return "destructive";
  if (status === "idle") return "outline";
  return "secondary";
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

function App() {
  const [status, setStatus] = useState<JobStatus>("idle");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [job, setJob] = useState<JobResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isRunning, setIsRunning] = useState(false);

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

    flushSync(() => {
      setError(null);
      setJob(null);
      setIsRunning(true);
      setStatus("preprocessing");
    });
    await new Promise((resolve) => window.setTimeout(resolve, 0));

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
    } catch (err) {
      setStatus("failed");
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsRunning(false);
    }
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

  const fileStateLabel = selectedPath ? "File loaded" : "No file";
  const outputStateLabel = job ? "Prompt ready" : "No output";

  return (
    <main className="app-shell">
      <header className="toolbar">
        <div className="title-block">
          <p className="eyebrow">Local music analysis</p>
          <h1>Timbreprint</h1>
          <p className="page-description">
            Analyze a local track, inspect its musical shape, and extract an English
            generation prompt.
          </p>
        </div>
        <div className="toolbar-status">
          <Badge variant="outline">{fileStateLabel}</Badge>
          <StatusBadge status={status} />
          <Badge variant="secondary">{outputStateLabel}</Badge>
        </div>
      </header>

      <section className="workspace">
        <aside className="control-rail">
          <section className="panel panel--primary">
            <div className="panel-head">
              <div>
                <p className="eyebrow">Input</p>
                <h2>Source</h2>
              </div>
              <Badge variant="outline">MVP</Badge>
            </div>

            <div className="drop-zone">
              <FileAudio size={28} />
              <div>
                <p className="drop-title">Local track</p>
                <p>mp3, wav, m4a, flac. 10 min max.</p>
              </div>
              <Button onClick={() => void selectFile()} variant="secondary">
                Choose file
              </Button>
            </div>

            {selectedPath ? <p className="selected-path">{selectedPath}</p> : null}

            <div className="actions">
              <Button
                disabled={!selectedPath || isRunning}
                loading={isRunning}
                onClick={() => void runAnalysis()}
              >
                {isRunning ? null : <Terminal size={16} />}
                Analyze
              </Button>
            </div>

            {error ? <p className="error-text">{error}</p> : null}
          </section>

          <section className="panel panel--subtle">
            <div className="panel-head">
              <div>
                <p className="eyebrow">Job</p>
                <h2>Current file</h2>
              </div>
            </div>
            <dl className="meta-list">
              <div>
                <dt>State</dt>
                <dd>{status}</dd>
              </div>
              <div>
                <dt>Source</dt>
                <dd>{selectedPath ? selectedPath : "No track loaded"}</dd>
              </div>
              <div>
                <dt>Mode</dt>
                <dd>{isRunning ? "Analyzing" : "Idle"}</dd>
              </div>
              <div>
                <dt>Output</dt>
                <dd>{job ? job.jobDir : "Not generated yet"}</dd>
              </div>
            </dl>
          </section>
        </aside>

        <section className="analysis-surface">
          {job ? (
            <>
              <section className="panel panel--summary">
                <div className="result-header">
                  <div>
                    <p className="eyebrow">Analysis result</p>
                    <h2>{job.id}</h2>
                  </div>
                  <div className="result-actions">
                    <Button onClick={() => void copyPrompt()} variant="secondary">
                      <Copy size={16} />
                      Copy prompt
                    </Button>
                    <Button onClick={() => void openJobFolder()} variant="outline">
                      <FolderOpen size={16} />
                      Open output
                    </Button>
                  </div>
                </div>

                <div className="summary-strip">
                  <Metric
                    label="Tempo"
                    value={`${job.analysis.tempo.value} BPM`}
                    score={job.analysis.tempo.confidence}
                  />
                  <Metric
                    label="Key"
                    value={job.analysis.key.value}
                    score={job.analysis.key.confidence}
                  />
                  <Metric
                    label="Energy"
                    value={job.analysis.energy.value}
                    score={job.analysis.energy.confidence}
                  />
                </div>
              </section>

              <section className="details-grid">
                <section className="panel panel--dense">
                  <div className="panel-head">
                    <div>
                      <p className="eyebrow">Tags</p>
                      <h2>Musical shape</h2>
                    </div>
                  </div>

                  <div className="tag-grid">
                    <TagGroup title="Genre">{pillList(job.analysis.genre)}</TagGroup>
                    <TagGroup title="Mood">{pillList(job.analysis.mood)}</TagGroup>
                    <TagGroup title="Instruments">{pillList(job.analysis.instruments)}</TagGroup>
                    <TagGroup title="Texture">{pillList(job.analysis.texture)}</TagGroup>
                  </div>
                </section>

                <section className="panel panel--dense">
                  <div className="panel-head">
                    <div>
                      <p className="eyebrow">Prompt</p>
                      <h2>English Prompt</h2>
                    </div>
                  </div>

                  <div className="prompt-box">
                    <p>{job.prompt}</p>
                  </div>
                </section>
              </section>

              <section className="panel panel--json">
                <div className="panel-head">
                  <div>
                    <p className="eyebrow">Data</p>
                    <h2>Raw JSON</h2>
                  </div>
                </div>
                <details className="json-view" open>
                  <summary>Raw analysis JSON</summary>
                  <pre>{JSON.stringify(job.analysis, null, 2)}</pre>
                </details>
              </section>
            </>
          ) : (
            <section className="panel panel--empty">
              <p className="eyebrow">Result</p>
              <h2>No analysis yet</h2>
              <p>
                Choose a local file, then run analysis.
              </p>
            </section>
          )}
        </section>
      </section>
    </main>
  );
}

function StatusBadge({ status }: { status: JobStatus }) {
  const variant = statusBadgeVariant(status);

  return (
    <Badge variant={variant}>
      {status === "completed" ? <CheckCircle2 size={15} /> : null}
      {inProgressStatuses.has(status) ? <Loader2 className="spin" size={15} /> : null}
      {status}
    </Badge>
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

function TagGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="tag-group">
      <h3>{title}</h3>
      <div className="pill-list">{children}</div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <App />,
);
