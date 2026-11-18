import { useState } from "react";
import { BookOpen, ExternalLink, Plus, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { fmtDuration } from "../lib/format";
import { Button, Card, Empty, ErrorNote, PageHeader, Stat } from "../components/ui";
import { ActiveSession, SessionStarter, type Preset } from "../components/Session";
import { NotesPanel } from "../components/NotesPanel";
import { SessionHistory } from "./Focus";
import { FocusMusicCard } from "../components/FocusMusic";

const STUDY_PRESETS: Preset[] = [
  { id: "pomodoro", title: "Pomodoro", desc: "4 × 25 min with short breaks", plan: { label: "Study", focusMins: 25, breakMins: 5, longBreakMins: 15, rounds: 4 } },
  { id: "50", title: "50 minutes", desc: "One focused block", plan: { label: "Study", focusMins: 50, breakMins: 0, longBreakMins: 0, rounds: 1 } },
  { id: "90", title: "90-minute session", desc: "Long, deep study", plan: { label: "Study", focusMins: 90, breakMins: 0, longBreakMins: 0, rounds: 1 } },
  { id: "custom", title: "Custom", desc: "Your own length and breaks", plan: { label: "Study", focusMins: 45, breakMins: 10, longBreakMins: 0, rounds: 2 } },
];

function Resources() {
  const { data, reload } = useLoad(() => api.resources(), []);
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const [url, setUrl] = useState("");
  const [subject, setSubject] = useState("");
  const [error, setError] = useState<string | null>(null);
  const field = "rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900";

  const add = async () => {
    setError(null);
    try {
      await api.addResource(title, url, subject || null);
      setTitle("");
      setUrl("");
      setSubject("");
      setAdding(false);
      reload();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Card
      title="Study resources"
      action={
        !adding && (
          <Button onClick={() => setAdding(true)}>
            <Plus size={15} /> Add link
          </Button>
        )
      }
    >
      <ErrorNote error={error} />
      {adding && (
        <div className="mb-3 grid gap-2 text-sm sm:grid-cols-[1fr_1.5fr_0.8fr_auto]">
          <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Name" className={field} />
          <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://…" className={field} />
          <input value={subject} onChange={(e) => setSubject(e.target.value)} placeholder="Subject (optional)" className={field} />
          <div className="flex gap-2">
            <Button variant="primary" onClick={add}>Save</Button>
            <Button onClick={() => setAdding(false)}>Cancel</Button>
          </div>
        </div>
      )}
      {!data?.length ? (
        <Empty>No saved links yet.</Empty>
      ) : (
        <ul className="grid gap-2 sm:grid-cols-2">
          {data.map((r) => (
            <li key={r.id} className="group flex items-center gap-2 rounded-lg border border-slate-200 px-3 py-2 dark:border-slate-700">
              <button onClick={() => api.openUrl(r.url).catch((e) => setError(String(e)))} className="flex min-w-0 flex-1 items-center gap-2 text-left">
                <BookOpen size={15} className="shrink-0 text-teal-500" />
                <span className="min-w-0">
                  <span className="block truncate text-sm font-medium">{r.title}</span>
                  <span className="block truncate text-xs text-slate-500">{r.subject ? `${r.subject} · ` : ""}{r.url.replace(/^https?:\/\//, "")}</span>
                </span>
                <ExternalLink size={13} className="ml-auto shrink-0 text-slate-400" />
              </button>
              <button
                onClick={async () => {
                  await api.deleteResource(r.id);
                  reload();
                }}
                className="rounded p-1 text-slate-400 opacity-0 hover:text-rose-600 group-hover:opacity-100"
                aria-label={`Remove ${r.title}`}
              >
                <Trash2 size={14} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

export default function Study() {
  const { data, error, reload } = useLoad(() => api.focus("study"), [], 5000);
  const time = useLoad(() => api.studyTime(), [], 30000);
  const notebooks = useLoad(() => api.notebooks(), []);
  const subjects = [...new Set([...(data?.history.map((h) => h.subject).filter((s): s is string => !!s) ?? []), ...(notebooks.data?.map((n) => n.name).filter((n) => n !== "Quick notes" && n !== "General") ?? [])])];
  const [subject, setSubject] = useState<string>("");
  const activeSubject = data?.active?.mode === "study" ? data.active.subject ?? "" : "";
  const notesSubject = activeSubject || subject || subjects[0] || "General";

  return (
    <>
      <PageHeader title="Study" subtitle="A distraction-free study space: timer, blocking, resources and notes per subject." />
      <ErrorNote error={error} />

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Stat label="Studied today" value={fmtDuration(data?.todaySecs ?? 0)} hint="In study sessions" />
        <Stat label="This week" value={fmtDuration(data?.weekSecs ?? 0)} hint={`${data?.weekSessions ?? 0} rounds completed`} />
        <Stat label="Study apps & sites today" value={fmtDuration(time.data?.[0] ?? 0)} hint="Time in the Study category" />
        <Stat label="Study apps & sites this week" value={fmtDuration(time.data?.[1] ?? 0)} hint="Assign apps and sites to Study to count them" />
      </div>

      <Card className="mt-4" title={data?.active ? (data.active.mode === "study" ? "Studying" : "A focus session is running") : "Start studying"}>
        {data?.active ? (
          <ActiveSession f={data.active} stopPhrase={data.stopPhrase} onChange={reload} />
        ) : (
          <SessionStarter mode="study" presets={STUDY_PRESETS} defaultProfile="Study Mode" subjects={subjects} onStarted={reload} />
        )}
      </Card>

      <div className="mt-4">
        <FocusMusicCard />
      </div>

      <Card
        className="mt-4"
        title={`Notes · ${notesSubject}`}
        action={
          !activeSubject && (
            <select
              value={notesSubject}
              onChange={(e) => setSubject(e.target.value)}
              className="rounded-md border border-slate-200 bg-white px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
              aria-label="Subject notebook"
            >
              {[...new Set([notesSubject, ...subjects, "General"])].map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
          )
        }
      >
        <NotesPanel notebook={notesSubject} />
      </Card>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Resources />
        <Card title="Study history">
          <SessionHistory items={data?.history ?? []} />
        </Card>
      </div>
    </>
  );
}
