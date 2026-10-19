import type { DriveKind } from "../lib/types";

export type IconName =
  | "hdd" | "usb" | "ssd" | "nvme" | "sd"
  | "ok" | "warn" | "error" | "info" | "question" | "play" | "pending" | "skipped" | "shield";

/**
 * Hand-drawn XP-style icons (32x32 grid, Luna gradients). One hidden sprite is mounted once by
 * <XpIconSprite/>; every <XpIcon/> references a symbol from it, so sizes 16 and 32 share one drawing.
 */
export function XpIconSprite() {
  return (
    <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id="g-metal" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#f4f6fa" /><stop offset="0.5" stopColor="#c3cad6" /><stop offset="1" stopColor="#8993a5" />
        </linearGradient>
        <linearGradient id="g-metal-dark" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#8d99b0" /><stop offset="1" stopColor="#4d5870" />
        </linearGradient>
        <linearGradient id="g-blue" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#7fb4ff" /><stop offset="0.5" stopColor="#2f6fe0" /><stop offset="1" stopColor="#1546a8" />
        </linearGradient>
        <linearGradient id="g-green" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#8fe07a" /><stop offset="0.55" stopColor="#2fa42a" /><stop offset="1" stopColor="#17701a" />
        </linearGradient>
        <linearGradient id="g-yellow" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#fff39a" /><stop offset="0.55" stopColor="#f9c80e" /><stop offset="1" stopColor="#c98c00" />
        </linearGradient>
        <linearGradient id="g-red" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ff9c86" /><stop offset="0.5" stopColor="#e03a1e" /><stop offset="1" stopColor="#9b1408" />
        </linearGradient>
        <linearGradient id="g-pcb" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#4fb36a" /><stop offset="1" stopColor="#1f7a3c" />
        </linearGradient>
        <linearGradient id="g-gold" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ffe88a" /><stop offset="1" stopColor="#c8961a" />
        </linearGradient>

        <symbol id="i-hdd" viewBox="0 0 32 32">
          <path d="M3 13 7 6h18l4 7z" fill="#e9edf5" stroke="#5a6478" />
          <rect x="3" y="13" width="26" height="12" rx="1.5" fill="url(#g-metal)" stroke="#4d5870" />
          <rect x="5" y="15" width="22" height="1.6" fill="#fff" opacity=".7" />
          <rect x="5" y="20" width="12" height="2" fill="#5a6478" />
          <circle cx="23" cy="21" r="1.6" fill="#38d64a" stroke="#14701f" strokeWidth=".6" />
          <rect x="3" y="25" width="26" height="2" fill="#39425a" />
        </symbol>
        <symbol id="i-ssd" viewBox="0 0 32 32">
          <rect x="3" y="9" width="26" height="15" rx="2" fill="url(#g-blue)" stroke="#0d2f7a" />
          <rect x="5" y="11" width="22" height="2" fill="#fff" opacity=".45" />
          <rect x="6" y="16" width="9" height="5" fill="#0d2f7a" opacity=".55" />
          <rect x="18" y="16" width="8" height="1.6" fill="#dbe8ff" /><rect x="18" y="19.4" width="6" height="1.6" fill="#dbe8ff" />
          <rect x="4" y="24" width="24" height="2.5" fill="url(#g-gold)" stroke="#8a6410" strokeWidth=".6" />
        </symbol>
        <symbol id="i-nvme" viewBox="0 0 32 32">
          <rect x="2" y="10" width="28" height="13" rx="1.5" fill="url(#g-pcb)" stroke="#0e4a22" />
          <rect x="6" y="12.5" width="8" height="8" fill="#20262e" stroke="#000" strokeWidth=".6" />
          <rect x="16" y="12.5" width="6" height="8" fill="#20262e" stroke="#000" strokeWidth=".6" />
          <rect x="24" y="14" width="3" height="5" fill="#cfd5dd" stroke="#6a7384" strokeWidth=".6" />
          <path d="M2 23h3v3H2z" fill="url(#g-gold)" /><rect x="6" y="23" width="22" height="2.4" fill="url(#g-gold)" stroke="#8a6410" strokeWidth=".5" />
        </symbol>
        <symbol id="i-usb" viewBox="0 0 32 32">
          <rect x="2" y="12" width="9" height="8" fill="url(#g-metal)" stroke="#4d5870" />
          <rect x="4" y="14" width="2" height="1.8" fill="#39425a" /><rect x="7" y="14" width="2" height="1.8" fill="#39425a" />
          <rect x="11" y="9" width="19" height="14" rx="2.5" fill="url(#g-blue)" stroke="#0d2f7a" />
          <rect x="13" y="11" width="15" height="2.2" fill="#fff" opacity=".45" />
          <circle cx="25" cy="19" r="1.6" fill="#38d64a" stroke="#14701f" strokeWidth=".6" />
        </symbol>
        <symbol id="i-sd" viewBox="0 0 32 32">
          <path d="M8 3h13l5 5v21H8z" fill="url(#g-blue)" stroke="#0d2f7a" />
          <path d="M21 3v5h5" fill="none" stroke="#0d2f7a" strokeWidth=".8" />
          <rect x="11" y="4.5" width="1.8" height="5" fill="url(#g-gold)" /><rect x="14.2" y="4.5" width="1.8" height="5" fill="url(#g-gold)" /><rect x="17.4" y="4.5" width="1.8" height="5" fill="url(#g-gold)" />
          <rect x="11" y="15" width="12" height="9" fill="#fff" opacity=".85" /><rect x="13" y="18" width="8" height="1.6" fill="#1546a8" />
        </symbol>
        <symbol id="i-ok" viewBox="0 0 32 32">
          <circle cx="16" cy="16" r="13" fill="url(#g-green)" stroke="#115a14" />
          <ellipse cx="16" cy="9" rx="9" ry="5" fill="#fff" opacity=".35" />
          <path d="M8.5 16.5l5 5 10-11" fill="none" stroke="#fff" strokeWidth="4" strokeLinecap="square" />
        </symbol>
        <symbol id="i-warn" viewBox="0 0 32 32">
          <path d="M16 3 30 28H2z" fill="url(#g-yellow)" stroke="#8a6000" strokeLinejoin="round" strokeWidth="1.2" />
          <rect x="14.3" y="11" width="3.4" height="9" fill="#2a2100" />
          <rect x="14.3" y="22" width="3.4" height="3.2" fill="#2a2100" />
        </symbol>
        <symbol id="i-error" viewBox="0 0 32 32">
          <circle cx="16" cy="16" r="13" fill="url(#g-red)" stroke="#7a0f06" />
          <ellipse cx="16" cy="9" rx="9" ry="5" fill="#fff" opacity=".35" />
          <path d="M10 10l12 12M22 10 10 22" stroke="#fff" strokeWidth="4" />
        </symbol>
        <symbol id="i-info" viewBox="0 0 32 32">
          <circle cx="16" cy="16" r="13" fill="url(#g-blue)" stroke="#0d2f7a" />
          <ellipse cx="16" cy="9" rx="9" ry="5" fill="#fff" opacity=".35" />
          <rect x="14" y="14" width="4" height="10" fill="#fff" /><rect x="14" y="8" width="4" height="4" fill="#fff" />
        </symbol>
        <symbol id="i-question" viewBox="0 0 32 32">
          <circle cx="16" cy="16" r="13" fill="url(#g-blue)" stroke="#0d2f7a" />
          <ellipse cx="16" cy="9" rx="9" ry="5" fill="#fff" opacity=".35" />
          <path d="M11.5 12.5a4.5 4.5 0 1 1 6.6 4c-1.4.8-2.1 1.6-2.1 3.2" fill="none" stroke="#fff" strokeWidth="3.4" />
          <rect x="14.2" y="22" width="3.6" height="3.6" fill="#fff" />
        </symbol>
        <symbol id="i-play" viewBox="0 0 32 32">
          <path d="M6 5 26 16 6 27z" fill="url(#g-blue)" stroke="#0d2f7a" strokeLinejoin="round" />
        </symbol>
        <symbol id="i-pending" viewBox="0 0 32 32">
          <circle cx="16" cy="16" r="9" fill="#f4f2e8" stroke="#9c9a8c" strokeWidth="2" />
        </symbol>
        <symbol id="i-skipped" viewBox="0 0 32 32">
          <circle cx="16" cy="16" r="12" fill="url(#g-metal)" stroke="#59627a" />
          <rect x="8" y="14" width="16" height="4" fill="#fff" />
        </symbol>
        <symbol id="i-shield" viewBox="0 0 32 32">
          <path d="M16 2 28 6v10c0 7-5 12-12 15C9 28 4 23 4 16V6z" fill="url(#g-blue)" stroke="#0d2f7a" strokeLinejoin="round" />
          <path d="M16 2 28 6v10c0 7-5 12-12 15z" fill="#fff" opacity=".18" />
          <path d="M10 16l4.5 4.5L22 12" fill="none" stroke="#fff" strokeWidth="3.4" />
        </symbol>
      </defs>
    </svg>
  );
}

export function XpIcon({ name, size = 16, className = "" }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg className={`xp-icon ${className}`} width={size} height={size} viewBox="0 0 32 32" aria-hidden="true" focusable="false">
      <use href={`#i-${name}`} />
    </svg>
  );
}

export function driveIconName(kind: DriveKind): IconName {
  return kind === "nvme" ? "nvme" : kind === "ssd" ? "ssd" : kind === "usb" ? "usb" : kind === "sd" ? "sd" : "hdd";
}

export function DriveIcon({ kind, size = 16 }: { kind: DriveKind; size?: number }) {
  return <XpIcon name={driveIconName(kind)} size={size} />;
}
