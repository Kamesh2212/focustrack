import { useEffect, useState } from "react";
import { Smartphone } from "lucide-react";
import { api } from "../lib/api";
import { isPhone, usePlatform } from "../lib/platform";
import { Button, Card } from "./ui";

/**
 * Phones: whether FocusTrack may read screen time (Android "Usage access"),
 * re-checked and imported whenever the app comes back to the front.
 */
export function usePhoneAccess(): { phone: boolean; granted: boolean | null; open: () => void } {
  const p = usePlatform();
  const phone = isPhone(p);
  const [granted, setGranted] = useState<boolean | null>(null);

  useEffect(() => {
    if (!phone) return;
    const check = () => {
      api
        .phoneUsageAccess()
        .then((g) => {
          setGranted(g);
          if (g) api.importPhoneUsage().catch(() => {});
        })
        .catch(() => setGranted(false));
    };
    check();
    const onVisible = () => document.visibilityState === "visible" && check();
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [phone]);

  return {
    phone,
    granted,
    open: () => {
      api.openUsageAccess().catch(() => {});
    },
  };
}

const WHY =
  "Android only shares which app is open and for how long — never what you do in it. FocusTrack keeps this on your phone and, if you sign in, syncs it end-to-end encrypted with your other devices.";

/** Dashboard banner until usage access is granted. */
export function PhoneAccessBanner() {
  const { phone, granted, open } = usePhoneAccess();
  if (!phone || granted !== false) return null;
  return (
    <div className="mb-4 flex flex-wrap items-center gap-3 rounded-xl border border-indigo-200 bg-indigo-50 px-4 py-3 text-sm text-indigo-900 dark:border-indigo-900 dark:bg-indigo-950/50 dark:text-indigo-100">
      <Smartphone size={18} className="shrink-0" />
      <div className="min-w-0 flex-1">
        <div className="font-medium">Let FocusTrack see your screen time</div>
        <div className="text-xs opacity-80">
          Turn on <b>Usage access</b> for FocusTrack in the next screen, then come back.
        </div>
      </div>
      <Button variant="primary" onClick={open}>
        Allow
      </Button>
    </div>
  );
}

/** Settings card on phones. */
export function PhoneAccessCard() {
  const { phone, granted, open } = usePhoneAccess();
  if (!phone) return null;
  return (
    <Card title="Phone screen time">
      <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
        <div className="max-w-xl">
          <div className="font-medium">{granted ? "Usage access is on" : "Usage access is off"}</div>
          <div className="mt-0.5 text-xs text-slate-500">{WHY}</div>
        </div>
        <Button onClick={open}>{granted ? "Manage" : "Turn on"}</Button>
      </div>
    </Card>
  );
}
