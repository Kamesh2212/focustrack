import { useEffect, useState } from "react";
import { ShieldCheck, Smartphone } from "lucide-react";
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

/**
 * Phones: app blocking (#69) uses an Android accessibility service. Google Play
 * requires a clear explanation and the user's agreement before sending them
 * to turn it on, so the card shows both first.
 */
export function PhoneBlockingCard() {
  const p = usePlatform();
  const phone = p === "android";
  const [on, setOn] = useState<boolean | null>(null);
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    if (!phone) return;
    const check = () => api.phoneBlocking().then(setOn).catch(() => setOn(false));
    check();
    const onVisible = () => document.visibilityState === "visible" && check();
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [phone]);

  if (!phone) return null;
  return (
    <Card title="App blocking on this phone">
      <div className="text-sm">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="max-w-xl">
            <div className="font-medium">{on ? "App blocking is on" : "App blocking is off"}</div>
            <div className="mt-0.5 text-xs text-slate-500">
              Blocks apps when their daily limit is used up, and the apps your focus session blocks. The phone app,
              emergency calls and Settings are never blocked.
            </div>
          </div>
          {!asking && (
            <Button onClick={() => (on ? api.openPhoneBlocking().catch(() => {}) : setAsking(true))}>{on ? "Manage" : "Turn on"}</Button>
          )}
        </div>
        {asking && (
          <div className="mt-3 rounded-lg border border-indigo-200 bg-indigo-50 p-3 text-indigo-950 dark:border-indigo-900 dark:bg-indigo-950/50 dark:text-indigo-100">
            <div className="mb-1 flex items-center gap-1.5 font-medium">
              <ShieldCheck size={15} /> FocusTrack uses Android's Accessibility service
            </div>
            <ul className="ml-4 list-disc space-y-1 text-xs">
              <li>
                <b>What it sees:</b> only which app is open (its name). Never what's on the screen, what you type or
                anything inside apps.
              </li>
              <li>
                <b>What it does:</b> shows FocusTrack's block screen over an app that is over its daily limit or blocked by
                your focus session.
              </li>
              <li>
                <b>Where it goes:</b> nowhere. It stays on this phone; it isn't sent to FocusTrack or anyone else.
              </li>
              <li>You can turn it off any time in Settings &gt; Accessibility.</li>
            </ul>
            <p className="mt-2 text-xs">
              On the next screen, open <b>FocusTrack app blocking</b> and turn it on. Android may first ask you to allow
              restricted settings for FocusTrack (in its App info).
            </p>
            <div className="mt-3 flex flex-wrap gap-2">
              <Button
                variant="primary"
                onClick={() => {
                  setAsking(false);
                  api.openPhoneBlocking().catch(() => {});
                }}
              >
                I agree, open settings
              </Button>
              <Button onClick={() => setAsking(false)}>Not now</Button>
            </div>
          </div>
        )}
      </div>
    </Card>
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
