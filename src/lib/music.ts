// Focus music: background sounds generated live with the Web Audio API
// (no audio files, so nothing to license or download). Lives in the main
// window, which keeps running while FocusTrack sits in the tray; the focus
// timer pop-up controls it through Tauri events.

export type Sound = "rain" | "ocean" | "brown" | "pink" | "white";

export const SOUNDS: { id: Sound; name: string; desc: string }[] = [
  { id: "rain", name: "Rain", desc: "Steady rainfall" },
  { id: "ocean", name: "Ocean waves", desc: "Slow, rolling surf" },
  { id: "brown", name: "Brown noise", desc: "Deep, warm rumble" },
  { id: "pink", name: "Pink noise", desc: "Soft, balanced hiss" },
  { id: "white", name: "White noise", desc: "Bright, masks chatter" },
];

export interface MusicPrefs {
  sound: Sound;
  volume: number; // 0..1
  autoplay: boolean; // start with focus/study sessions
  pauseOnBreaks: boolean;
  playlistUrl: string; // the user's own music, opened in the browser
}

const KEY = "focustrack.music";
const DEFAULTS: MusicPrefs = { sound: "rain", volume: 0.5, autoplay: true, pauseOnBreaks: true, playlistUrl: "" };

export function loadPrefs(): MusicPrefs {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? "{}") };
  } catch {
    return { ...DEFAULTS };
  }
}

export function savePrefs(p: MusicPrefs) {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    /* storage unavailable: keep in memory only */
  }
}

/** Two seconds of noise in the requested colour, loopable. */
function noiseBuffer(ctx: AudioContext, colour: "white" | "pink" | "brown"): AudioBuffer {
  const len = ctx.sampleRate * 4;
  const buf = ctx.createBuffer(2, len, ctx.sampleRate);
  for (let ch = 0; ch < 2; ch++) {
    const d = buf.getChannelData(ch);
    let b0 = 0, b1 = 0, b2 = 0, b3 = 0, b4 = 0, b5 = 0, b6 = 0, last = 0;
    for (let i = 0; i < len; i++) {
      const w = Math.random() * 2 - 1;
      if (colour === "white") d[i] = w * 0.5;
      else if (colour === "pink") {
        // Paul Kellet's refined pink-noise filter
        b0 = 0.99886 * b0 + w * 0.0555179;
        b1 = 0.99332 * b1 + w * 0.0750759;
        b2 = 0.969 * b2 + w * 0.153852;
        b3 = 0.8665 * b3 + w * 0.3104856;
        b4 = 0.55 * b4 + w * 0.5329522;
        b5 = -0.7616 * b5 - w * 0.016898;
        d[i] = (b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362) * 0.11;
        b6 = w * 0.115926;
      } else {
        last = (last + 0.02 * w) / 1.02;
        d[i] = last * 3.5;
      }
    }
    // Fade the loop seam.
    for (let i = 0; i < 2000; i++) {
      const k = i / 2000;
      d[i] *= k;
      d[len - 1 - i] *= k;
    }
  }
  return buf;
}

class FocusMusic {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private stopFns: (() => void)[] = [];
  playing: Sound | null = null;
  private listeners = new Set<() => void>();

  onChange(fn: () => void) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }
  private changed() {
    this.listeners.forEach((f) => f());
  }

  private ensure() {
    if (!this.ctx) {
      this.ctx = new AudioContext();
      this.master = this.ctx.createGain();
      this.master.connect(this.ctx.destination);
    }
    return { ctx: this.ctx, master: this.master! };
  }

  setVolume(v: number) {
    if (this.master && this.ctx) this.master.gain.setTargetAtTime(Math.max(0, Math.min(1, v)), this.ctx.currentTime, 0.1);
  }

  async play(sound: Sound, volume: number) {
    this.stop(false);
    const { ctx, master } = this.ensure();
    await ctx.resume();
    master.gain.setValueAtTime(0, ctx.currentTime);
    master.gain.linearRampToValueAtTime(volume, ctx.currentTime + 1.5);

    const src = ctx.createBufferSource();
    src.loop = true;
    const nodes: AudioNode[] = [];
    if (sound === "white" || sound === "pink" || sound === "brown") {
      src.buffer = noiseBuffer(ctx, sound);
      src.connect(master);
    } else if (sound === "rain") {
      src.buffer = noiseBuffer(ctx, "pink");
      const hp = ctx.createBiquadFilter();
      hp.type = "highpass";
      hp.frequency.value = 600;
      const lp = ctx.createBiquadFilter();
      lp.type = "lowpass";
      lp.frequency.value = 7000;
      const g = ctx.createGain();
      g.gain.value = 0.9;
      src.connect(hp).connect(lp).connect(g).connect(master);
      nodes.push(hp, lp, g);
    } else {
      // Ocean: brown noise swelling and receding every ~9 seconds.
      src.buffer = noiseBuffer(ctx, "brown");
      const lp = ctx.createBiquadFilter();
      lp.type = "lowpass";
      lp.frequency.value = 900;
      const swell = ctx.createGain();
      swell.gain.value = 0.55;
      const lfo = ctx.createOscillator();
      lfo.frequency.value = 0.11;
      const depth = ctx.createGain();
      depth.gain.value = 0.45;
      lfo.connect(depth).connect(swell.gain);
      src.connect(lp).connect(swell).connect(master);
      lfo.start();
      nodes.push(lp, swell, depth);
      this.stopFns.push(() => lfo.stop());
    }
    src.start();
    this.stopFns.push(() => {
      src.stop();
      src.disconnect();
      nodes.forEach((n) => n.disconnect());
    });
    this.playing = sound;
    this.changed();
  }

  stop(notify = true) {
    if (this.ctx && this.master && this.stopFns.length) {
      const fns = this.stopFns;
      this.stopFns = [];
      this.master.gain.setTargetAtTime(0, this.ctx.currentTime, 0.3);
      window.setTimeout(() => fns.forEach((f) => f()), 1200);
    }
    this.playing = null;
    if (notify) this.changed();
  }
}

/** One player for the whole app. */
export const music = new FocusMusic();
