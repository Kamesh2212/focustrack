import type { LucideIcon } from "lucide-react";
import { Card, PageHeader } from "../components/ui";

export default function ComingSoon({
  title,
  phase,
  icon: Icon,
  summary,
  features,
}: {
  title: string;
  phase: string;
  icon: LucideIcon;
  summary: string;
  features: string[];
}) {
  return (
    <>
      <PageHeader title={title} />
      <Card>
        <div className="flex flex-col items-center px-6 py-10 text-center">
          <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-indigo-50 text-indigo-600 dark:bg-indigo-950 dark:text-indigo-300">
            <Icon size={26} />
          </div>
          <span className="mb-2 rounded-full bg-slate-100 px-2.5 py-0.5 text-xs font-medium text-slate-600 dark:bg-slate-800 dark:text-slate-300">
            {phase}
          </span>
          <p className="max-w-lg text-sm text-slate-600 dark:text-slate-300">{summary}</p>
          <ul className="mt-6 grid max-w-2xl gap-2 text-left text-sm sm:grid-cols-2">
            {features.map((f) => (
              <li key={f} className="flex gap-2 text-slate-600 dark:text-slate-300">
                <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-indigo-500" />
                {f}
              </li>
            ))}
          </ul>
        </div>
      </Card>
    </>
  );
}
