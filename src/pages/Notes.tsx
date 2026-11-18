import { useState } from "react";
import { Notebook as NotebookIcon } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { Card, PageHeader, cx } from "../components/ui";
import { NotesPanel } from "../components/NotesPanel";

export default function Notes() {
  const { data } = useLoad(() => api.notebooks(), [], 15000);
  const [notebook, setNotebook] = useState<string | null>(null);
  const books = data ?? [];
  const total = books.reduce((s, b) => s + b.count, 0);

  return (
    <>
      <PageHeader title="Notes" subtitle="Notebooks for subjects, projects and quick thoughts. Changes save automatically." />
      <div className="grid gap-4 lg:grid-cols-[200px_1fr]">
        <Card className="h-fit">
          <ul className="space-y-0.5 text-sm">
            {[{ name: null as string | null, label: "All notes", count: total }, ...books.map((b) => ({ name: b.name, label: b.name, count: b.count }))].map((b) => (
              <li key={b.label}>
                <button
                  onClick={() => setNotebook(b.name)}
                  className={cx(
                    "flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left",
                    notebook === b.name ? "bg-indigo-50 font-medium text-indigo-700 dark:bg-indigo-950/60 dark:text-indigo-300" : "hover:bg-slate-50 dark:hover:bg-slate-800/60",
                  )}
                >
                  <NotebookIcon size={14} className="shrink-0" />
                  <span className="truncate">{b.label}</span>
                  <span className="ml-auto text-xs text-slate-400">{b.count}</span>
                </button>
              </li>
            ))}
          </ul>
          <p className="mt-3 text-xs text-slate-500">To start a notebook, create a note and type a new notebook name.</p>
        </Card>
        <Card>
          <NotesPanel key={notebook ?? "all"} notebook={notebook ?? undefined} notebooks={books.map((b) => b.name)} />
        </Card>
      </div>
    </>
  );
}
