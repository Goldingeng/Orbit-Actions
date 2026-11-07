const fs = require('fs');
const path = require('path');

const docs = path.join(__dirname, '..', 'docs');
const results = JSON.parse(fs.readFileSync(path.join(docs, 'results.json'), 'utf8').replace(/^﻿/, ''));
const [orbitName, kandoName] = Object.keys(results.apps);
const orbit = results.apps[orbitName];
const kando = results.apps[kandoName];

const metrics = [
  { file: 'memory-idle', en: 'Memory at idle', ru: 'Память в простое', unit: 'mb', get: a => a.idle.workingSetMB },
  { file: 'memory-private', en: 'Private memory at idle', ru: 'Выделенная память (private) в простое', unit: 'mb', get: a => a.idle.privateMB },
  { file: 'memory-menu', en: 'Memory with the menu open', ru: 'Память с открытым меню', unit: 'mb', get: a => a.menuOpen.workingSetMB },
  { file: 'memory-after', en: 'Memory after opening the menu 20 times', ru: 'Память после 20 открытий меню', unit: 'mb', get: a => a.after20Menus.workingSetMB },
  { file: 'memory-settings', en: 'Memory with settings open', ru: 'Память с открытыми настройками', unit: 'mb', get: a => a.settingsOpen.workingSetMB },
  { file: 'start', en: 'Startup until ready', ru: 'Запуск до готовности', unit: 'ms', faster: true, get: a => a.startMs },
  { file: 'menu-latency', en: 'Opening the menu from a command', ru: 'Открытие меню по команде', unit: 'ms', faster: true, get: a => a.menuOpenMs },
  { file: 'disk', en: 'Size on disk', ru: 'Размер на диске', unit: 'mb', get: a => a.diskMB },
  { file: 'processes', en: 'Processes', ru: 'Процессы', unit: '', get: a => a.idle.processes },
  { file: 'threads', en: 'Threads at idle', ru: 'Потоки в простое', unit: '', get: a => a.idle.threads },
  { file: 'handles', en: 'Handles at idle', ru: 'Дескрипторы в простое', unit: '', get: a => a.idle.handles },
  { file: 'cpu-idle', en: 'CPU usage at idle', ru: 'Загрузка CPU в простое', unit: '%', get: a => a.idleCpuPercent },
];

const languages = {
  en: {
    dir: docs,
    locale: 'en-US',
    comma: '.',
    units: { mb: 'MB', ms: 'ms', '%': '%', '': '' },
    zeroCpu: 'Orbit Actions uses no CPU at all while idle',
    same: 'About the same',
    better: (faster, n) => `Orbit Actions is ${n} ${faster ? 'faster' : 'smaller'}`,
    worse: n => `Orbit Actions is ${n} worse`,
    times: n => `${n}x`,
  },
  ru: {
    dir: path.join(docs, 'ru'),
    locale: 'ru-RU',
    comma: ',',
    units: { mb: 'МБ', ms: 'мс', '%': '%', '': '' },
    zeroCpu: 'Orbit Actions в простое не тратит процессор вообще',
    same: 'Примерно одинаково',
    better: (faster, n) => `Orbit Actions ${faster ? 'быстрее' : 'меньше'} в ${n}`,
    worse: n => `Orbit Actions хуже в ${n}`,
    times: n => {
      if (!Number.isInteger(n)) return `${String(n).replace('.', ',')} раза`;
      const last = n % 10;
      const few = last >= 2 && last <= 4 && !(n % 100 >= 12 && n % 100 <= 14);
      return `${n.toLocaleString('ru-RU')} ${few ? 'раза' : 'раз'}`;
    },
  },
};

const style = `<style>
  .surface { fill: #fcfcfb; }
  .title { fill: #0b0b0b; font: 600 15px 'Segoe UI', system-ui, sans-serif; }
  .label { fill: #52514e; font: 13px 'Segoe UI', system-ui, sans-serif; }
  .value { fill: #0b0b0b; font: 600 13px 'Segoe UI', system-ui, sans-serif; }
  .note { fill: #52514e; font: 12px 'Segoe UI', system-ui, sans-serif; }
  .base { stroke: #d6d5d0; }
  .orbit { fill: #2a78d6; }
  .kando { fill: #eb6834; }
  @media (prefers-color-scheme: dark) {
    .surface { fill: #1a1a19; }
    .title, .value { fill: #f0efec; }
    .label, .note { fill: #c3c2b7; }
    .base { stroke: #3a3a37; }
    .orbit { fill: #3987e5; }
    .kando { fill: #d95926; }
  }
</style>`;

function format(lang, value) {
  if (value === null || value === undefined) return '-';
  if (Number.isInteger(value) || value >= 100) return Math.round(value).toLocaleString(lang.locale);
  if (value >= 1) return value.toFixed(1).replace('.', lang.comma);
  return String(+value.toFixed(3)).replace('.', lang.comma);
}

function ratio(lang, m, a, b) {
  if (a === 0 && b > 0) return lang.zeroCpu;
  if (!a || !b) return '';
  const round = r => (r >= 10 ? Math.round(r) : Math.round(r * 10) / 10);
  const r = b / a;
  if (r >= 1.05) return lang.better(m.faster, lang.times(round(r)));
  if (r <= 0.95) return lang.worse(lang.times(round(1 / r)));
  return lang.same;
}

function bar(lang, name, cls, value, max, y, unit) {
  const left = 130;
  const width = 520;
  const length = value ? Math.max(2, (value / max) * width) : 0;
  const end = left + length;
  const shown = `${format(lang, value)} ${unit}`.trim();
  const shape = length > 4
    ? `<path class="${cls}" d="M${left},${y} H${end - 4} Q${end},${y} ${end},${y + 4} V${y + 18} Q${end},${y + 22} ${end - 4},${y + 22} H${left} Z"><title>${name}: ${shown}</title></path>`
    : `<rect class="${cls}" x="${left}" y="${y}" width="${length}" height="22"><title>${name}: ${shown}</title></rect>`;
  return `<text class="label" x="${left - 10}" y="${y + 16}" text-anchor="end">${name}</text>
  ${shape}
  <text class="value" x="${end + 8}" y="${y + 16}">${shown}</text>`;
}

function chart(lang, m, title) {
  const a = m.get(orbit);
  const b = m.get(kando);
  const max = Math.max(a || 0, b || 0) || 1;
  const unit = lang.units[m.unit];
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 760 132" width="760" height="132" role="img" aria-label="${title}: ${orbitName} ${format(lang, a)} ${unit}, ${kandoName} ${format(lang, b)} ${unit}">
${style}
<rect class="surface" width="760" height="132" rx="8"/>
<text class="title" x="20" y="28">${title}</text>
${bar(lang, 'Orbit Actions', 'orbit', a, max, 44, unit)}
${bar(lang, 'Kando', 'kando', b, max, 74, unit)}
<line class="base" x1="130" y1="40" x2="130" y2="100"/>
<text class="note" x="20" y="120">${ratio(lang, m, a, b)}</text>
</svg>
`;
}

for (const [code, lang] of Object.entries(languages)) {
  fs.mkdirSync(lang.dir, { recursive: true });
  for (const m of metrics) {
    fs.writeFileSync(path.join(lang.dir, `${m.file}.svg`), chart(lang, m, m[code]));
  }
  console.log(code, metrics.map(m => `${m.file}: ${ratio(lang, m, m.get(orbit), m.get(kando))}`).join('\n   '));
}
