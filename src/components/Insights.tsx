import { Lightbulb, TrendingDown, TrendingUp } from "lucide-react";
import type { Insight } from "../lib/types";
import { Empty, cx } from "./ui";

export function InsightList({ insights }: { insights: Insight[] }) {
  if (!insights.length) return <Empty>Insights appear after a few days of tracking.</Empty>;
  return (
    <ul className="space-y-2.5">
      {insights.map((i) => {
        const Icon = i.tone === "good" ? TrendingUp : i.tone === "warn" ? TrendingDown : Lightbulb;
        return (
          <li key={i.text} className="flex gap-3 text-sm">
            <span
              className={cx(
                "mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full",
                i.tone === "good" && "bg-emerald-50 text-emerald-600 dark:bg-emerald-950 dark:text-emerald-400",
                i.tone === "warn" && "bg-amber-50 text-amber-600 dark:bg-amber-950 dark:text-amber-400",
                i.tone === "info" && "bg-indigo-50 text-indigo-600 dark:bg-indigo-950 dark:text-indigo-300",
              )}
            >
              <Icon size={13} />
            </span>
            <span className="text-slate-700 dark:text-slate-200">{i.text}</span>
          </li>
        );
      })}
    </ul>
  );
}
