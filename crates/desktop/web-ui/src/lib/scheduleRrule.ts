import type { AutomationTriggerKind } from '../types/automation';
import { parseAutomationTriggerKind } from '../types/automation';

export type ScheduleKind =
  | 'minutely'
  | 'hourly'
  | 'daily'
  | 'weekly'
  | 'monthly'
  | 'once'
  | 'custom';

export const WEEKDAYS = ['MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU'] as const;
export const WORKDAYS = ['MO', 'TU', 'WE', 'TH', 'FR'] as const;

export interface ParsedSchedule {
  kind: ScheduleKind;
  intervalMinutes: number;
  intervalHours: number;
  intervalDays: number;
  intervalMonths: number;
  days: string[];
  hour: number;
  minute: number;
  monthDay: number;
  onceAt: string;
  customRrule: string;
  restrictWeekdays: boolean;
}

export interface ScheduleFormValues {
  scheduleKind: ScheduleKind;
  intervalMinutes: number;
  intervalHours: number;
  intervalDays: number;
  intervalMonths: number;
  days: string[];
  hour: number;
  minute: number;
  monthDay: number;
  onceAt: string;
  customRrule: string;
  restrictWeekdays: boolean;
}

function parseParts(rrule: string): Record<string, string> {
  const parts: Record<string, string> = {};
  for (const segment of rrule.toUpperCase().split(';')) {
    const [key, value] = segment.split('=');
    if (key && value) {
      parts[key.trim()] = value.trim();
    }
  }
  return parts;
}

export function toDatetimeLocalValue(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function defaultOnceAt(): string {
  const d = new Date();
  d.setMinutes(0, 0, 0);
  d.setHours(d.getHours() + 1);
  return toDatetimeLocalValue(d);
}

export function defaultScheduleFormValues(): ScheduleFormValues {
  return {
    scheduleKind: 'daily',
    intervalMinutes: 15,
    intervalHours: 1,
    intervalDays: 1,
    intervalMonths: 1,
    days: [...WORKDAYS],
    hour: 9,
    minute: 0,
    monthDay: 1,
    onceAt: defaultOnceAt(),
    customRrule: '',
    restrictWeekdays: false,
  };
}

export function formValuesFromAutomation(item: {
  name: string;
  prompt: string;
  rrule: string;
  trigger_kind?: string;
  model?: string | null;
  mode?: string | null;
  cwds?: string[];
  allow_shell?: boolean | null;
  trust_mode?: boolean | null;
  auto_approve?: boolean | null;
  gate_preset?: string | null;
  gate?: string[];
  use_worktree?: boolean | null;
  write_briefing?: boolean | null;
}): ScheduleFormValues & {
  name: string;
  prompt: string;
  triggerKind: AutomationTriggerKind;
  mode: string;
  model: string;
  workspace: string;
  allowShell: boolean;
  trustMode: boolean;
  autoApprove: boolean;
  gatePreset: string;
  gateInline: string;
  useWorktree: boolean;
  writeBriefing: boolean;
} {
  const parsed = parseRrule(item.rrule) ?? defaultScheduleFormValues();
  return {
    name: item.name,
    prompt: item.prompt,
    triggerKind: parseAutomationTriggerKind(item.trigger_kind),
    mode: item.mode ?? 'agent',
    model: item.model ?? '',
    workspace: item.cwds?.[0] ?? '',
    allowShell: item.allow_shell ?? false,
    trustMode: item.trust_mode ?? false,
    autoApprove: item.auto_approve !== false,
    gatePreset: item.gate_preset ?? '',
    gateInline: item.gate?.[0] ?? '',
    useWorktree: item.use_worktree !== false,
    writeBriefing: item.write_briefing !== false,
    ...parsed,
  };
}

export function parseRrule(rrule: string): ScheduleFormValues | null {
  const upper = rrule.trim().toUpperCase();
  if (!upper) return null;

  const parts = parseParts(upper);
  const freq = parts.FREQ;
  const bydayRaw = parts.BYDAY;
  const days = bydayRaw ? bydayRaw.split(',').map((d) => d.trim()).filter(Boolean) : [];
  const restrictWeekdays =
    days.length > 0 &&
    WORKDAYS.every((d) => days.includes(d)) &&
    days.length === WORKDAYS.length;

  const hour = Number(parts.BYHOUR ?? '9');
  const minute = Number(parts.BYMINUTE ?? '0');
  const interval = Number(parts.INTERVAL ?? '1');

  if (freq === 'MINUTELY') {
    return {
      ...defaultScheduleFormValues(),
      scheduleKind: 'minutely',
      intervalMinutes: Number.isFinite(interval) && interval > 0 ? interval : 15,
      days,
      restrictWeekdays: days.length > 0,
    };
  }
  if (freq === 'HOURLY') {
    return {
      ...defaultScheduleFormValues(),
      scheduleKind: 'hourly',
      intervalHours: Number.isFinite(interval) && interval > 0 ? interval : 1,
      days,
      restrictWeekdays: days.length > 0,
    };
  }
  if (freq === 'DAILY') {
    return {
      ...defaultScheduleFormValues(),
      scheduleKind: 'daily',
      intervalDays: Number.isFinite(interval) && interval > 0 ? interval : 1,
      hour: Number.isFinite(hour) ? hour : 9,
      minute: Number.isFinite(minute) ? minute : 0,
    };
  }
  if (freq === 'WEEKLY') {
    return {
      ...defaultScheduleFormValues(),
      scheduleKind: 'weekly',
      days: days.length > 0 ? days : [...WORKDAYS],
      hour: Number.isFinite(hour) ? hour : 9,
      minute: Number.isFinite(minute) ? minute : 0,
    };
  }
  if (freq === 'MONTHLY') {
    const monthDay = Number(parts.BYMONTHDAY ?? '1');
    return {
      ...defaultScheduleFormValues(),
      scheduleKind: 'monthly',
      intervalMonths: Number.isFinite(interval) && interval > 0 ? interval : 1,
      monthDay: Number.isFinite(monthDay) ? monthDay : 1,
      hour: Number.isFinite(hour) ? hour : 9,
      minute: Number.isFinite(minute) ? minute : 0,
    };
  }
  if (freq === 'ONCE') {
    const dtstart = parts.DTSTART ?? '';
    let onceAt = dtstart;
    if (dtstart && !dtstart.includes('T')) {
      onceAt = dtstart;
    } else if (dtstart) {
      const d = new Date(dtstart);
      if (!Number.isNaN(d.getTime())) {
        onceAt = toDatetimeLocalValue(d);
      } else if (dtstart.length >= 16) {
        onceAt = dtstart.slice(0, 16);
      }
    }
    return {
      ...defaultScheduleFormValues(),
      scheduleKind: 'once',
      onceAt: onceAt || defaultOnceAt(),
    };
  }

  return {
    ...defaultScheduleFormValues(),
    scheduleKind: 'custom',
    customRrule: rrule.trim(),
  };
}

function bydaySuffix(restrictWeekdays: boolean, days: string[]): string {
  if (restrictWeekdays) {
    return `;BYDAY=${WORKDAYS.join(',')}`;
  }
  if (days.length > 0) {
    return `;BYDAY=${days.join(',')}`;
  }
  return '';
}

export function buildRrule(values: ScheduleFormValues): string {
  const {
    scheduleKind,
    intervalMinutes,
    intervalHours,
    intervalDays,
    intervalMonths,
    days,
    hour,
    minute,
    monthDay,
    onceAt,
    customRrule,
    restrictWeekdays,
  } = values;

  if (scheduleKind === 'custom') {
    return customRrule.trim().toUpperCase();
  }

  if (scheduleKind === 'minutely') {
    const interval = Math.max(1, intervalMinutes);
    const base = interval === 1 ? 'FREQ=MINUTELY' : `FREQ=MINUTELY;INTERVAL=${interval}`;
    return `${base}${bydaySuffix(restrictWeekdays, days)}`;
  }

  if (scheduleKind === 'hourly') {
    const interval = Math.max(1, intervalHours);
    const base = interval === 1 ? 'FREQ=HOURLY' : `FREQ=HOURLY;INTERVAL=${interval}`;
    return `${base}${bydaySuffix(restrictWeekdays, days)}`;
  }

  if (scheduleKind === 'daily') {
    const interval = Math.max(1, intervalDays);
    const base = `FREQ=DAILY;BYHOUR=${hour};BYMINUTE=${minute}`;
    return interval === 1 ? base : `${base};INTERVAL=${interval}`;
  }

  if (scheduleKind === 'weekly') {
    const byday = days.length > 0 ? days.join(',') : 'MO';
    return `FREQ=WEEKLY;BYDAY=${byday};BYHOUR=${hour};BYMINUTE=${minute}`;
  }

  if (scheduleKind === 'monthly') {
    const interval = Math.max(1, intervalMonths);
    const day = Math.min(31, Math.max(1, monthDay));
    const base = `FREQ=MONTHLY;BYMONTHDAY=${day};BYHOUR=${hour};BYMINUTE=${minute}`;
    return interval === 1 ? base : `${base};INTERVAL=${interval}`;
  }

  const dt = onceAt.trim() || defaultOnceAt();
  const withSeconds = dt.length === 16 ? `${dt}:00` : dt;
  return `FREQ=ONCE;DTSTART=${withSeconds}`;
}

export function describeRrule(
  rrule: string,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  const upper = rrule.toUpperCase();
  const hm = (h: string, m: string) => `${h.padStart(2, '0')}:${m.padStart(2, '0')}`;
  const bydaySuffixDesc = () => {
    const byday = /BYDAY=([^;]+)/.exec(upper)?.[1] ?? '';
    if (!byday) return '';
    const isWorkdays =
      WORKDAYS.every((d) => byday.includes(d)) && byday.split(',').length === WORKDAYS.length;
    return isWorkdays ? ` (${t('schedule.workdaysOnly')})` : ` (${byday})`;
  };

  if (upper.startsWith('FREQ=MINUTELY')) {
    const intervalMatch = /INTERVAL=(\d+)/.exec(upper);
    const interval = intervalMatch ? Number(intervalMatch[1]) : 1;
    const suffix = bydaySuffixDesc();
    return interval === 1
      ? `${t('schedule.rruleDescMinutely')}${suffix}`
      : `${t('schedule.rruleDescMinutelyInterval', { interval: String(interval) })}${suffix}`;
  }
  if (upper.startsWith('FREQ=HOURLY')) {
    const intervalMatch = /INTERVAL=(\d+)/.exec(upper);
    const interval = intervalMatch ? Number(intervalMatch[1]) : 1;
    const suffix = bydaySuffixDesc();
    return interval === 1
      ? `${t('schedule.rruleDescHourly')}${suffix}`
      : `${t('schedule.rruleDescHourlyInterval', { interval: String(interval) })}${suffix}`;
  }
  if (upper.startsWith('FREQ=DAILY')) {
    const intervalMatch = /INTERVAL=(\d+)/.exec(upper);
    const interval = intervalMatch ? Number(intervalMatch[1]) : 1;
    const hour = /BYHOUR=(\d+)/.exec(upper)?.[1] ?? '0';
    const minute = /BYMINUTE=(\d+)/.exec(upper)?.[1] ?? '0';
    return interval === 1
      ? t('schedule.rruleDescDaily', { time: hm(hour, minute) })
      : t('schedule.rruleDescDailyInterval', {
          interval: String(interval),
          time: hm(hour, minute),
        });
  }
  if (upper.startsWith('FREQ=WEEKLY')) {
    const byday = /BYDAY=([^;]+)/.exec(upper)?.[1] ?? '';
    const hour = /BYHOUR=(\d+)/.exec(upper)?.[1] ?? '0';
    const minute = /BYMINUTE=(\d+)/.exec(upper)?.[1] ?? '0';
    return t('schedule.rruleDescWeekly', { days: byday, time: hm(hour, minute) });
  }
  if (upper.startsWith('FREQ=MONTHLY')) {
    const intervalMatch = /INTERVAL=(\d+)/.exec(upper);
    const interval = intervalMatch ? Number(intervalMatch[1]) : 1;
    const monthDay = /BYMONTHDAY=(\d+)/.exec(upper)?.[1] ?? '1';
    const hour = /BYHOUR=(\d+)/.exec(upper)?.[1] ?? '0';
    const minute = /BYMINUTE=(\d+)/.exec(upper)?.[1] ?? '0';
    return interval === 1
      ? t('schedule.rruleDescMonthly', { day: monthDay, time: hm(hour, minute) })
      : t('schedule.rruleDescMonthlyInterval', {
          interval: String(interval),
          day: monthDay,
          time: hm(hour, minute),
        });
  }
  if (upper.startsWith('FREQ=ONCE')) {
    const dtstart = /DTSTART=([^;]+)/.exec(upper)?.[1] ?? '';
    try {
      const display = new Date(dtstart).toLocaleString();
      return t('schedule.rruleDescOnce', { at: display });
    } catch {
      return t('schedule.rruleDescOnce', { at: dtstart });
    }
  }
  return rrule;
}
