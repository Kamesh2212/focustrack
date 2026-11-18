import { useEffect, useRef, useState } from "react";
import { FilePlus2, Link2, Search, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { dayLabel, clock } from "../lib/format";
import type { Note } from "../lib/types";
import { Button, Empty, ErrorNote, cx } from "./ui";

type SaveState = "saved" | "saving" | "unsaved" | "error";

/** Autosaving editor for one note. */
function Editor({ note, notebooks, fixedNotebook, onSaved, onDelete }: {
  note: Note;
  notebooks: string[];
  fixedNotebook?: string;
  onSaved: (n: Note) => void;
  onDelete: () => void;
}) {
  const [title, setTitle] = useState(note.title);
  const [body, setBody] = useState(note.body);
  const [notebook, setNotebook] = useState(note.notebook);
  const [state, setState] = useState<SaveState>("saved");
  const timer = useRef<number | null>(null);
  const latest = useRef({ title, body, notebook });
  latest.current = { title, body, notebook };

  const save = async () => {
    setState("saving");
    try {
      const n = await api.updateNote(note.id, latest.current.notebook, latest.current.title, latest.current.body);
      setState("saved");
      onSaved(n);
    } catch {
      setState("error");
    }
  };

  const schedule = () => {
    setState("unsaved");
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(save, 700);
  };

  // Save pending edits when switching notes or leaving the page.
  useEffect(
    () => () => {
      if (timer.current) {
        window.clearTimeout(timer.current);
        api.updateNote(note.id, latest.current.notebook, latest.current.title, latest.current.body).catch(() => {});
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [note.id],
  );

  return (
    <div className="flex h-full min-h-[420px] flex-col">
      <div className="mb-2 flex items-center gap-2">
        <input
          value={title}
          onChange={(e) => {
            setTitle(e.target.value);
            schedule();
          }}
          placeholder="Title"
          maxLength={200}
          className="min-w-0 flex-1 bg-transparent text-lg font-semibold outline-none"
          aria-label="Note title"
        />
        <span className={cx("shrink-0 text-xs", state === "error" ? "text-rose-600" : "text-slate-400")}>
          {state === "saving" ? "Saving…" : state === "unsaved" ? "Editing" : state === "error" ? "Couldn't save" : "Saved"}
        </span>
        <button onClick={onDelete} className="rounded p-1.5 text-slate-500 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950" aria-label="Delete note">
          <Trash2 size={15} />
        </button>
      </div>
      <div className="mb-3 flex flex-wrap items-center gap-3 text-xs text-slate-500">
        {!fixedNotebook && (
          <label className="flex items-center gap-1.5">
            Notebook
            <input
              list="ft-notebooks"
              value={notebook}
              maxLength={60}
              onChange={(e) => {
                setNotebook(e.target.value);
                schedule();
              }}
              className="w-36 rounded border border-slate-200 bg-white px-1.5 py-0.5 text-xs dark:border-slate-700 dark:bg-slate-900"
            />
            <datalist id="ft-notebooks">
              {notebooks.map((n) => (
                <option key={n} value={n} />
              ))}
            </datalist>
          </label>
        )}
        <span>Edited {dayLabel(note.updatedAt.slice(0, 10))} {clock(note.updatedAt)}</span>
        {note.sessionLabel && (
          <span className="inline-flex items-center gap-1">
            <Link2 size={12} /> Written during "{note.sessionLabel}"
          </span>
        )}
      </div>
      <textarea
        value={body}
        onChange={(e) => {
          setBody(e.target.value);
          schedule();
        }}
        placeholder="Start writing…"
        className="min-h-[340px] flex-1 resize-none rounded-lg border border-slate-200 bg-slate-50/60 p-3 font-[inherit] text-sm leading-relaxed outline-none focus:border-indigo-400 dark:border-slate-800 dark:bg-slate-950/40"
        aria-label="Note text"
      />
    </div>
  );
}

/**
 * Notes list + editor. With `notebook`, shows and creates notes in that
 * notebook only (used for a study subject).
 */
export function NotesPanel({ notebook, notebooks = [] }: { notebook?: string; notebooks?: string[] }) {
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { data, reload } = useLoad(() => api.notes(notebook ?? null, search), [notebook, search]);
  const notes = data ?? [];
  const current = notes.find((n) => n.id === selected) ?? null;

  useEffect(() => {
    if (selected === null && notes.length) setSelected(notes[0].id);
  }, [notes, selected]);

  const create = async () => {
    setError(null);
    try {
      const n = await api.createNote(notebook ?? "General", "", "");
      setSelected(n.id);
      reload();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div>
      <ErrorNote error={error} />
      <div className="grid gap-4 md:grid-cols-[240px_1fr]">
        <div className="flex min-w-0 flex-col gap-2">
          <div className="flex gap-2">
            <label className="flex min-w-0 flex-1 items-center gap-1.5 rounded-lg border border-slate-200 px-2 py-1 dark:border-slate-700">
              <Search size={14} className="text-slate-400" />
              <input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search notes" className="w-full bg-transparent text-sm outline-none" />
            </label>
            <Button onClick={create} title="New note">
              <FilePlus2 size={15} />
            </Button>
          </div>
          {notes.length === 0 ? (
            <Empty>No notes yet.</Empty>
          ) : (
            <ul className="max-h-[440px] space-y-1 overflow-y-auto">
              {notes.map((n) => (
                <li key={n.id}>
                  <button
                    onClick={() => setSelected(n.id)}
                    className={cx(
                      "w-full rounded-lg px-2.5 py-2 text-left",
                      n.id === selected ? "bg-indigo-50 dark:bg-indigo-950/60" : "hover:bg-slate-50 dark:hover:bg-slate-800/60",
                    )}
                  >
                    <div className="truncate text-sm font-medium">{n.title || n.body.split("\n")[0] || "Untitled"}</div>
                    <div className="truncate text-xs text-slate-500">
                      {!notebook && `${n.notebook} · `}
                      {dayLabel(n.updatedAt.slice(0, 10))}
                    </div>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="min-w-0">
          {current ? (
            <Editor
              key={current.id}
              note={current}
              notebooks={notebooks}
              fixedNotebook={notebook}
              onSaved={() => reload()}
              onDelete={async () => {
                await api.deleteNote(current.id);
                setSelected(null);
                reload();
              }}
            />
          ) : (
            <Empty>Select a note or create a new one.</Empty>
          )}
        </div>
      </div>
    </div>
  );
}

/** One-line quick note for the dashboard. */
export function QuickNote() {
  const [text, setText] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const save = async () => {
    const t = text.trim();
    if (!t) return;
    try {
      await api.createNote("Quick notes", t.split("\n")[0].slice(0, 60), t);
      setText("");
      setStatus("Saved to Quick notes ✓");
    } catch (e) {
      setStatus(String(e));
    }
    window.setTimeout(() => setStatus(null), 3000);
  };
  return (
    <div>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) save();
        }}
        rows={3}
        placeholder="Jot something down… (Ctrl+Enter to save)"
        className="w-full resize-none rounded-lg border border-slate-200 bg-white p-2.5 text-sm outline-none focus:border-indigo-400 dark:border-slate-700 dark:bg-slate-900"
        aria-label="Quick note"
      />
      <div className="mt-2 flex items-center gap-3">
        <Button onClick={save} disabled={!text.trim()}>
          Save note
        </Button>
        {status && <span className="text-xs text-slate-500">{status}</span>}
      </div>
    </div>
  );
}
