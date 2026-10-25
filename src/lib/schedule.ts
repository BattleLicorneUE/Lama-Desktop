/** Tâches planifiées : calcul des échéances (heure locale) et textes affichés. */

export type Repeat = "once" | "hourly" | "daily" | "weekdays" | "weekly";

export type ScheduledTask = {
  id: string;
  name: string;
  prompt: string;
  kind: "chat" | "code";
  workspace: string | null;
  project_id: string | null;
  repeat: Repeat;
  /** « HH:MM » ; pour « hourly », seule la minute compte. */
  time: string;
  /** 0 = lundi … 6 = dimanche (weekly). */
  weekday: number;
  /** « AAAA-MM-JJ » (once). */
  date: string | null;
  enabled: boolean;
  next_at: number | null;
  last_at: number | null;
  last_conv: string | null;
};

export const REPEATS: { id: Repeat; label: string }[] = [
  { id: "once", label: "Une seule fois" },
  { id: "hourly", label: "Chaque heure" },
  { id: "daily", label: "Chaque jour" },
  { id: "weekdays", label: "Du lundi au vendredi" },
  { id: "weekly", label: "Chaque semaine" },
];

export const DAYS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];

const pad = (n: number) => String(n).padStart(2, "0");
export const dateInput = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

function parseTime(time: string): [number, number] {
  const m = /^(\d{1,2}):(\d{2})$/.exec(time.trim());
  const h = m ? Math.min(23, Number(m[1])) : 8;
  const min = m ? Math.min(59, Number(m[2])) : 0;
  return [h, min];
}

/** Prochaine échéance strictement après `from` (secondes Unix), ou `null` si la tâche ne se répète pas et que l'heure est passée. */
export function nextRun(t: Pick<ScheduledTask, "repeat" | "time" | "weekday" | "date">, from: Date = new Date()): number | null {
  const [h, min] = parseTime(t.time);
  const at = (d: Date) => Math.floor(d.getTime() / 1000);
  if (t.repeat === "once") {
    const m = t.date ? /^(\d{4})-(\d{2})-(\d{2})$/.exec(t.date) : null;
    if (!m) return null;
    const when = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), h, min, 0, 0);
    return when > from ? at(when) : null;
  }
  if (t.repeat === "hourly") {
    const when = new Date(from);
    when.setMinutes(min, 0, 0);
    if (when <= from) when.setHours(when.getHours() + 1);
    return at(when);
  }
  for (let i = 0; i < 8; i++) {
    const when = new Date(from.getFullYear(), from.getMonth(), from.getDate() + i, h, min, 0, 0);
    if (when <= from) continue;
    const weekday = (when.getDay() + 6) % 7;
    if (t.repeat === "weekdays" && weekday > 4) continue;
    if (t.repeat === "weekly" && weekday !== t.weekday) continue;
    return at(when);
  }
  return null;
}

/** « Chaque jour à 08:00 », « Le lundi à 09:30 », « Le 24/12/2026 à 18:00 »… */
export function describeRepeat(t: Pick<ScheduledTask, "repeat" | "time" | "weekday" | "date">): string {
  const [h, min] = parseTime(t.time);
  const hm = `${pad(h)}:${pad(min)}`;
  switch (t.repeat) {
    case "once": {
      const m = t.date ? /^(\d{4})-(\d{2})-(\d{2})$/.exec(t.date) : null;
      return m ? `Le ${m[3]}/${m[2]}/${m[1]} à ${hm}` : "Date à choisir";
    }
    case "hourly":
      return `Chaque heure, à la minute ${pad(min)}`;
    case "daily":
      return `Chaque jour à ${hm}`;
    case "weekdays":
      return `Du lundi au vendredi à ${hm}`;
    default:
      return `Chaque ${DAYS[t.weekday] ?? "lundi"} à ${hm}`;
  }
}

/** « demain à 08:00 », « dans 12 min », « aujourd'hui à 18:00 »… */
export function describeNext(at: number | null, now: Date = new Date()): string {
  if (!at) return "Aucune échéance";
  const d = new Date(at * 1000);
  const mins = Math.round((d.getTime() - now.getTime()) / 60000);
  if (mins < 1) return "dans un instant";
  if (mins < 60) return `dans ${mins} min`;
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(d) - day(now)) / 86_400_000);
  if (diff === 0) return `aujourd'hui à ${hm}`;
  if (diff === 1) return `demain à ${hm}`;
  if (diff < 7) return `${DAYS[(d.getDay() + 6) % 7]} à ${hm}`;
  return `le ${pad(d.getDate())}/${pad(d.getMonth() + 1)} à ${hm}`;
}
