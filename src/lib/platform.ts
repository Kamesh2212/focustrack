import { useEffect, useState } from "react";
import { api } from "./api";

let cached: Promise<string> | null = null;

/** "windows", "android", … (asked once). */
export function platform(): Promise<string> {
  cached ??= api.platform().catch(() => "windows");
  return cached;
}

/** The platform, or null while it's being looked up. */
export function usePlatform(): string | null {
  const [p, setP] = useState<string | null>(null);
  useEffect(() => {
    platform().then(setP);
  }, []);
  return p;
}

export const isPhone = (p: string | null) => p === "android" || p === "ios";
