const fs = require('fs');
const path = require('path');

const docs = path.join(__dirname, '..', 'docs');
const results = JSON.parse(fs.readFileSync(path.join(docs, 'results.json'), 'utf8').replace(/^﻿/, ''));
const [orbitName, kandoName] = Object.keys(results.apps);
const orbit = results.apps[orbitName];
const kando = results.apps[kandoName];

const metrics = [
  { file: 'memory-idle', title: 'Память в простое', unit: 'МБ', get: a => a.idle.workingSetMB },
  { file: 'memory-private', title: 'Выделенная память (private) в простое', unit: 'МБ', get: a => a.idle.privateMB },
  { file: 'memory-menu', title: 'Память с открытым меню', unit: 'МБ', get: a => a.menuOpen.workingSetMB },
  { file: 'memory-after', title: 'Память после 20 открытий меню', unit: 'МБ', get: a => a.after20Menus.workingSetMB },
  { file: 'memory-settings', title: 'Память с открытыми настройками', unit: 'МБ', get: a => a.settingsOpen.workingSetMB },
  { file: 'start', title: 'Запуск до готовности', unit: 'мс', word: 'быстрее', get: a => a.startMs },
  { file: 'menu-latency', title: 'Открытие меню по команде', unit: 'мс', word: 'быстрее', get: a => a.menuOpenMs },
  { file: 'disk', title: 'Размер на диске', unit: 'МБ', get: a => a.diskMB },
  { file: 'processes', title: 'Процессы', unit: '', get: a => a.idle.processes },
  { file: 'threads', title: 'Потоки в простое', unit: '', get: a => a.idle.threads },
  { file: 'handles', title: 'Дескрипторы в простое', unit: '', get: a => a.idle.handles },
  { file: 'cpu-idle', title: 'Загрузка CPU в простое', unit: '%', get: a => a.idleCpuPercent },
];

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

function format(value) {
  if (value === null || value === undefined) return '-';
  if (Number.isInteger(value) || value >= 100) return Math.round(value).toLocaleString('ru-RU');
  if (value >= 1) return value.toFixed(1).replace('.', ',');
  return String(+value.toFixed(3)).replace('.', ',');
}

function times(value) {
  const rounded = value >= 10 ? Math.round(value) : Math.round(value * 10) / 10;
  if (!Number.isInteger(rounded)) return `${String(rounded).replace('.', ',')} раза`;
  const last = rounded % 10;
  const few = last >= 2 && last <= 4 && !(rounded % 100 >= 12 && rounded % 100 <= 14);
  return `${rounded.toLocaleString('ru-RU')} ${few ? 'раза' : 'раз'}`;
}

function ratio(a, b, word = 'меньше') {
  if (a === 0 && b > 0) return 'Orbit Actions в простое не тратит процессор вообще';
  if (!a || !b) return '';
  const r = b / a;
  if (r >= 1.05) return `Orbit Actions ${word} в ${times(r)}`;
  if (r <= 0.95) return `Orbit Actions хуже в ${times(1 / r)}`;
  return 'Примерно одинаково';
}

function bar(name, cls, value, max, y, unit) {
  const left = 130;
  const width = 520;
  const length = value ? Math.max(2, (value / max) * width) : 0;
  const end = left + length;
  const shape = length > 4
    ? `<path class="${cls}" d="M${left},${y} H${end - 4} Q${end},${y} ${end},${y + 4} V${y + 18} Q${end},${y + 22} ${end - 4},${y + 22} H${left} Z"><title>${name}: ${format(value)} ${unit}</title></path>`
    : `<rect class="${cls}" x="${left}" y="${y}" width="${length}" height="22"><title>${name}: ${format(value)} ${unit}</title></rect>`;
  return `<text class="label" x="${left - 10}" y="${y + 16}" text-anchor="end">${name}</text>
  ${shape}
  <text class="value" x="${end + 8}" y="${y + 16}">${format(value)} ${unit}</text>`;
}

function chart(m) {
  const a = m.get(orbit);
  const b = m.get(kando);
  const max = Math.max(a || 0, b || 0) * 1.0 || 1;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 760 132" width="760" height="132" role="img" aria-label="${m.title}: ${orbitName} ${format(a)} ${m.unit}, ${kandoName} ${format(b)} ${m.unit}">
${style}
<rect class="surface" width="760" height="132" rx="8"/>
<text class="title" x="20" y="28">${m.title}</text>
${bar('Orbit Actions', 'orbit', a, max, 44, m.unit)}
${bar('Kando', 'kando', b, max, 74, m.unit)}
<line class="base" x1="130" y1="40" x2="130" y2="100"/>
<text class="note" x="20" y="120">${ratio(a, b, m.word)}</text>
</svg>
`;
}

for (const m of metrics) {
  fs.writeFileSync(path.join(docs, `${m.file}.svg`), chart(m));
}

const rows = metrics.map(m => `| ${m.title} | ${format(m.get(orbit))} ${m.unit} | ${format(m.get(kando))} ${m.unit} | ${ratio(m.get(orbit), m.get(kando), m.word).replace('Orbit Actions ', '')} |`);
fs.writeFileSync(path.join(docs, 'table.md'), `| Показатель | Orbit Actions | Kando 3.0.1 | Разница |\n|---|---|---|---|\n${rows.join('\n')}\n`);
console.log(rows.join('\n'));
