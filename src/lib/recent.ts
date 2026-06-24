// Daftar file PDF yang terakhir dibuka, dipersistensi di localStorage.
// Dipakai untuk start page ("Terakhir dibuka").
import { basename } from "./ipc";

export type RecentFile = { path: string; name: string; ts: number };

const KEY = "feather:recent";
const MAX = 12;

export function getRecent(): RecentFile[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) || "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

/** Catat sebuah file sebagai baru saja dibuka (pindah ke paling atas). */
export function pushRecent(path: string): void {
  let list = getRecent().filter((r) => r.path !== path);
  list.unshift({ path, name: basename(path), ts: Date.now() });
  list = list.slice(0, MAX);
  localStorage.setItem(KEY, JSON.stringify(list));
}

export function removeRecent(path: string): void {
  const list = getRecent().filter((r) => r.path !== path);
  localStorage.setItem(KEY, JSON.stringify(list));
}

export function clearRecent(): void {
  localStorage.removeItem(KEY);
}

/** Waktu relatif singkat dalam bahasa Indonesia ("baru saja", "5 mnt lalu"). */
export function relativeTime(ts: number): string {
  const s = Math.max(0, Math.floor((Date.now() - ts) / 1000));
  if (s < 45) return "baru saja";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} mnt lalu`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} jam lalu`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d} hr lalu`;
  const w = Math.floor(d / 7);
  if (w < 5) return `${w} mgg lalu`;
  const mo = Math.floor(d / 30);
  if (mo < 12) return `${mo} bln lalu`;
  return `${Math.floor(d / 365)} thn lalu`;
}
