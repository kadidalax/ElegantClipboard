// 轻量级音效反馈（Web Audio 合成音，无需外部音频文件）

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { logError } from "@/lib/logger";
import { useUISettings, type SoundTiming } from "@/stores/ui-settings";

let _ctx: AudioContext | null = null;
let _audioFailureLogged = false;
let _pasteSoundUnlisten: Promise<UnlistenFn> | null = null;

function logAudioFailureOnce(message: string, error: unknown): void {
  if (_audioFailureLogged) return;
  _audioFailureLogged = true;
  logError(message, error);
}

function getCtx(): AudioContext {
  if (!_ctx) {
    _ctx = new AudioContext();
  }
  if (_ctx.state === "suspended") {
    void _ctx.resume().catch((error) => {
      logAudioFailureOnce("Failed to resume AudioContext:", error);
    });
  }
  return _ctx;
}

function warmUpCtx(ac: AudioContext) {
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  gain.gain.value = 0;
  osc.connect(gain);
  gain.connect(ac.destination);
  osc.start();
  osc.stop(ac.currentTime + 0.005);
}

if (typeof window !== "undefined") {
  warmUpCtx(getCtx());
}

function playToneAt(
  ac: AudioContext,
  freq: number,
  duration: number,
  volume: number,
  when: number,
) {
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = "sine";
  osc.frequency.value = freq;
  gain.gain.setValueAtTime(volume, when);
  gain.gain.exponentialRampToValueAtTime(0.001, when + duration);
  osc.connect(gain);
  gain.connect(ac.destination);
  osc.start(when);
  osc.stop(when + duration);
}

function playCopyTones() {
  try {
    const ac = getCtx();
    const t0 = ac.currentTime;
    playToneAt(ac, 880, 0.06, 0.15, t0);
    playToneAt(ac, 1100, 0.06, 0.15, t0 + 0.04);
  } catch (error) {
    logAudioFailureOnce("Audio unavailable, skip copy sound:", error);
  }
}

function playPasteTones() {
  try {
    const ac = getCtx();
    playToneAt(ac, 660, 0.08, 0.15, ac.currentTime);
  } catch (error) {
    logAudioFailureOnce("Audio unavailable, skip paste sound:", error);
  }
}

/** 设置页试听：不受开关限制 */
export function previewCopySound() {
  playCopyTones();
}

/** 设置页试听：不受开关限制 */
export function previewPasteSound() {
  playPasteTones();
}

export function playCopySound(timing: SoundTiming) {
  const s = useUISettings.getState();
  if (!s.copySound || s.copySoundTiming !== timing) return;
  playCopyTones();
}

export function playPasteSound(timing: SoundTiming) {
  const s = useUISettings.getState();
  if (!s.pasteSound || s.pasteSoundTiming !== timing) return;
  playPasteTones();
}

/** 监听后端粘贴事件（覆盖快捷键粘贴、合并粘贴、粘贴为路径等） */
export function setupPasteSoundListeners(): Promise<UnlistenFn> {
  if (_pasteSoundUnlisten) {
    return _pasteSoundUnlisten;
  }

  _pasteSoundUnlisten = (async () => {
    const unlistenImmediate = await listen("paste-sound-immediate", () => {
      playPasteSound("immediate");
    });
    const unlistenSuccess = await listen("paste-sound-success", () => {
      playPasteSound("after_success");
    });
    return () => {
      void unlistenImmediate();
      void unlistenSuccess();
      _pasteSoundUnlisten = null;
    };
  })();

  return _pasteSoundUnlisten;
}
