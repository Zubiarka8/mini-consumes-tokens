// MCT dashboard SPA (ADR-019). Alpine CSP build: templates only read properties
// and call methods without arguments (the element's `data-v` carries the value),
// so every derived view-model is precomputed here. No inline script, no eval.
'use strict';

const I18N = {
  es: {
    project: 'Proyecto', snapshot: 'Instantánea', updating: 'Actualizando…', offline: 'Sin conexión con el servidor',
    theme: 'Cambiar tema', themeDark: 'Oscuro', themeLight: 'Claro', langName: 'Español',
    navDup: 'Duplicados', navDead: 'Código muerto', navObs: 'Obsidian',
    filter: 'Filtrar…', all: 'Todos', pending: 'Pendiente', confirmed: 'Confirmado', false_positive: 'Falso positivo',
    ignored: 'Ignorado', needs_reevaluation: 'Reevaluar',
    pick: 'Elige un hallazgo de la lista.', occurrences: 'Ocurrencias', pickHint: 'Pulsa una ocurrencia para compararla con la primera.',
    onlyHere: 'Líneas que solo aparecen aquí', deadHint: 'Sin referencias indexadas: revisa que no se use por reflexión, macros o fuera del repositorio.',
    save: 'Guardar', findingReview: 'Decisión sobre el hallazgo', findingReason: 'Motivo de la decisión',
    findingHint: 'Esta decisión afecta al hallazgo completo y a los resultados que recibe la IA.',
    fileHint: 'Etiquetas de trabajo de cada copia. No descartan el hallazgo ni se invalidan al editar el código.',
    reviewSave: 'Guardar decisión', restore: 'Volver a pendiente', previousPage: 'Página anterior', nextPage: 'Página siguiente',
    conflict: 'El código o la revisión cambió. Tu borrador se conserva. Revisa la versión actual antes de guardarlo.',
    reviewLatest: 'Revisar versión actual y conservar borrador', retainedDraft: 'Borrador sin guardar', discardDraft: 'Descartar borrador',
    inspectOrphan: 'Ver historial y notas', orphanDetails: 'Historial y notas guardadas',
    sourcePrevious: 'Fragmento anterior', sourceNext: 'Fragmento siguiente', sourcePartial: 'Vista parcial del código: usa los controles para revisar el resto.',
    sourceUnavailable: 'No se pudo verificar o leer este fragmento. Reindexa y vuelve a cargar.',
    pDecision: 'Decisión guardada', pFingerprint: 'Huella del código', pScope: 'Alcance de esta petición',
    pResolved: 'El hallazgo ya está descartado. Conserva la decisión y no repitas la investigación, salvo que el código o su contexto hayan cambiado.',
    pNeedsReview: 'Reevalúa la decisión anterior sobre esta versión del código. Explica qué cambió antes de proponer modificaciones.',
    pFresh: 'Revisa únicamente este hallazgo y sus relaciones; no vuelvas a analizar todos los hallazgos del proyecto.',
    dupTitle: 'Código duplicado', deadTitle: 'Código muerto', lines: 'líneas', tokens: 'tokens', line: 'línea',
    emptyList: 'No hay hallazgos con este filtro.', noneDup: 'No se encontró código duplicado.', noneDead: 'No hay candidatos a código muerto.',
    obsTitle: 'Vault de Obsidian', obsSub: 'Notas generadas a partir del índice.', noVault: 'El vault no está generado (--no-obsidian).',
    graph: 'Grafo', fullGraph: 'Ver grafo completo', searchNotes: 'Buscar notas…', seeInGraph: 'Ver en el grafo', close: 'Cerrar',
    linksTo: 'Enlaza a', linkedFrom: 'Enlazada desde', nodes: 'nodos', notes: 'notas', links: 'enlaces',
    cFiles: 'Archivos', cDirs: 'Carpetas', cSymbols: 'Símbolos', cImports: 'Importaciones', cNotes: 'Notas', cLinks: 'Enlaces',
    cDup: 'Duplicados', cDead: 'Código muerto',
    errSave: 'No se pudo guardar', errLoad: 'No se pudo cargar', saved: 'Guardado', stale: 'Índice actualizado',
    files: 'Archivos', similarity: 'Líneas coincidentes con las demás', legendSame: 'Línea idéntica', legendRen: 'Misma línea con otros nombres', legendDiff: 'Línea distinta',
    fileReview: 'Revisión por fichero', file: 'Archivo', fileStatus: 'Estado del archivo', fileNote: 'Notas',
    fileNoteHint: 'Notas sobre este archivo…', dupWord: 'Duplicado', ofWord: 'de', filesWord: 'archivos',
    fPending: 'Sin revisar', fAccepted: 'Conservar esta copia', fRejected: 'Proponer retirar esta copia', fInReview: 'En revisión', fIgnored: 'Ignorarlo',
    navOrphans: 'Huérfanos', orphanHint: 'Decisiones cuyo hallazgo ya no aparece. Solo se borran al pulsar Borrar.', deleteDecision: 'Borrar decisión', noOrphans: 'No hay decisiones huérfanas.', navHome: 'Inicio', helloTitle: '¡Hola mundo!', helloSub: 'Panel de MCT: elige una sección para empezar.',
    openHint: 'Pulsa un duplicado para abrirlo en una pestaña nueva, como un editor de código.',
    openTab: 'Abrir en pestaña nueva', backToList: 'Volver a la lista', editorTitle: 'Editor de duplicado',
    show: 'Mostrar', showAll: 'Todo', showSame: 'Coincidentes', showDiff: 'Distintas', findInCode: 'Buscar en el código…',
    wrap: 'Ajustar líneas', fontUp: 'Aumentar letra', fontDown: 'Reducir letra', copyCode: 'Copiar código',
    copied: 'Copiado', prevDup: 'Duplicado anterior', nextDup: 'Duplicado siguiente', noMatch: 'Ninguna línea coincide con el filtro.',
    closeTab: 'Cerrar pestaña', closeFile: 'Cerrar archivo', notFound: 'Este duplicado ya no existe en el índice.',
    soloFile: 'Solo este archivo', openAll: 'Abrir todos', closeAll: 'Cerrar todo', noneOpen: 'No hay archivos abiertos: elige uno de la lista o pulsa «Abrir todos».', toggleFiles: 'Lista de archivos', toggleAside: 'Panel lateral',
    activeFile: 'Archivo activo',
    aiTitle: 'Plantilla para IA', aiCopy: 'Copiar plantilla', aiHint: 'Pégala en tu asistente: lleva ubicaciones, líneas y la tarea.',
    pDupIntro: 'Contexto: el análisis de MCT ha detectado código duplicado ({n} archivos, {lines} líneas, {tokens} tokens, lenguaje {lang}).',
    pDeadIntro: 'Contexto: el análisis de MCT marca `{name}` ({kind}, {lang}{parent}) como posible código muerto: no tiene referencias indexadas.',
    pWhere: 'Ubicaciones:', pNotes: 'Notas de revisión:', pTask: 'Qué hay que hacer:',
    pDupTask: [
      'Abre cada ubicación y compara los fragmentos línea a línea.',
      'Decide si es duplicación real (misma lógica) o solo un parecido (falso positivo) y explica por qué.',
      'Si es real, extrae la lógica común a una única función o módulo con un nombre claro y sustituye cada fragmento por una llamada.',
      'No cambies el comportamiento. Ejecuta los tests y el linter del proyecto antes y después.',
      'Si unificar empeora el código, déjalo como está y dime el motivo.',
    ],
    pDeadTask: [
      'Busca usos que el índice no ve: reflexión, macros, exportaciones públicas, tests, scripts y código fuera del repositorio.',
      'Si de verdad no se usa, bórralo junto con los imports, tests y documentación que queden huérfanos.',
      'Si es API pública o se usa de forma dinámica, no lo borres y explica el motivo.',
      'Ejecuta la compilación, los tests y el linter antes y después.',
    ],
  },
  en: {
    project: 'Project', snapshot: 'Snapshot', updating: 'Updating…', offline: 'Cannot reach the server',
    theme: 'Toggle theme', themeDark: 'Dark', themeLight: 'Light', langName: 'English',
    navDup: 'Duplicates', navDead: 'Dead code', navObs: 'Obsidian',
    filter: 'Filter…', all: 'All', pending: 'Pending', confirmed: 'Confirmed', false_positive: 'False positive',
    ignored: 'Ignored', needs_reevaluation: 'Re-evaluate',
    pick: 'Pick a finding from the list.', occurrences: 'Occurrences', pickHint: 'Click an occurrence to compare it with the first.',
    onlyHere: 'Lines that only appear here', deadHint: 'No indexed references: make sure it is not used through reflection, macros or outside this repository.',
    save: 'Save', findingReview: 'Finding decision', findingReason: 'Decision reason',
    findingHint: 'This decision applies to the whole finding and the results delivered to the AI.',
    fileHint: 'Working labels for each copy. They do not dismiss the finding or expire after source edits.',
    reviewSave: 'Save decision', restore: 'Restore to pending', previousPage: 'Previous page', nextPage: 'Next page',
    conflict: 'The source or review changed. Your draft is retained. Review the current version before saving it.',
    reviewLatest: 'Review current version and retain draft', retainedDraft: 'Unsaved draft', discardDraft: 'Discard draft',
    inspectOrphan: 'View history and notes', orphanDetails: 'Saved history and notes',
    sourcePrevious: 'Previous source chunk', sourceNext: 'Next source chunk', sourcePartial: 'Partial source view: use the controls to inspect the remaining code.',
    sourceUnavailable: 'This source chunk could not be verified or read. Reindex and reload.',
    pDecision: 'Saved decision', pFingerprint: 'Source fingerprint', pScope: 'Scope of this request',
    pResolved: 'This finding is already dismissed. Preserve the decision and do not repeat the investigation unless its code or context changed.',
    pNeedsReview: 'Reevaluate the earlier decision against this source version. Explain what changed before proposing edits.',
    pFresh: 'Review only this finding and its relationships; do not reanalyze every finding in the project.',
    dupTitle: 'Duplicate code', deadTitle: 'Dead code', lines: 'lines', tokens: 'tokens', line: 'line',
    emptyList: 'No findings match this filter.', noneDup: 'No duplicate code found.', noneDead: 'No dead-code candidates.',
    obsTitle: 'Obsidian vault', obsSub: 'Notes generated from the index.', noVault: 'The vault is not generated (--no-obsidian).',
    graph: 'Graph', fullGraph: 'Show full graph', searchNotes: 'Search notes…', seeInGraph: 'Show in graph', close: 'Close',
    linksTo: 'Links to', linkedFrom: 'Linked from', nodes: 'nodes', notes: 'notes', links: 'links',
    cFiles: 'Files', cDirs: 'Folders', cSymbols: 'Symbols', cImports: 'Imports', cNotes: 'Notes', cLinks: 'Links',
    cDup: 'Duplicates', cDead: 'Dead code',
    errSave: 'Could not save', errLoad: 'Could not load', saved: 'Saved', stale: 'Index updated',
    files: 'Files', similarity: 'Lines shared with the others', legendSame: 'Identical line', legendRen: 'Same line, other names', legendDiff: 'Different line',
    fileReview: 'File review', file: 'File', fileStatus: 'File status', fileNote: 'Notes',
    fileNoteHint: 'Notes about this file…', dupWord: 'Duplicate', ofWord: 'of', filesWord: 'files',
    fPending: 'Not reviewed', fAccepted: 'Keep this copy', fRejected: 'Propose removing this copy', fInReview: 'In review', fIgnored: 'Ignore it',
    navOrphans: 'Orphans', orphanHint: 'Decisions with no current finding. Only the Delete button removes them.', deleteDecision: 'Delete decision', noOrphans: 'No orphan decisions.', navHome: 'Home', helloTitle: 'Hello world!', helloSub: 'MCT dashboard: pick a section to get started.',
    openHint: 'Click a duplicate to open it in a new tab, like a code editor.',
    openTab: 'Open in a new tab', backToList: 'Back to the list', editorTitle: 'Duplicate editor',
    show: 'Show', showAll: 'All', showSame: 'Matching', showDiff: 'Different', findInCode: 'Search the code…',
    wrap: 'Wrap lines', fontUp: 'Larger text', fontDown: 'Smaller text', copyCode: 'Copy code',
    copied: 'Copied', prevDup: 'Previous duplicate', nextDup: 'Next duplicate', noMatch: 'No line matches the filter.',
    closeTab: 'Close tab', closeFile: 'Close file', notFound: 'This duplicate is no longer in the index.',
    soloFile: 'Only this file', openAll: 'Open all', closeAll: 'Close all', noneOpen: 'No files open: pick one from the list or press “Open all”.', toggleFiles: 'File list', toggleAside: 'Side panel',
    activeFile: 'Active file',
    aiTitle: 'AI prompt template', aiCopy: 'Copy template', aiHint: 'Paste it into your assistant: it carries locations, lines and the task.',
    pDupIntro: 'Context: the MCT analysis found duplicate code ({n} files, {lines} lines, {tokens} tokens, language {lang}).',
    pDeadIntro: 'Context: the MCT analysis flags `{name}` ({kind}, {lang}{parent}) as possible dead code: it has no indexed references.',
    pWhere: 'Locations:', pNotes: 'Review notes:', pTask: 'What to do:',
    pDupTask: [
      'Open each location and compare the snippets line by line.',
      'Decide whether it is real duplication (same logic) or just a resemblance (false positive), and explain why.',
      'If it is real, extract the shared logic into one function or module with a clear name and replace each snippet with a call.',
      'Do not change behaviour. Run the project tests and linter before and after.',
      'If merging makes the code worse, leave it as it is and tell me why.',
    ],
    pDeadTask: [
      'Look for uses the index cannot see: reflection, macros, public exports, tests, scripts and code outside the repository.',
      'If it is really unused, delete it together with the imports, tests and docs left orphaned.',
      'If it is public API or used dynamically, do not delete it and explain why.',
      'Run the build, tests and linter before and after.',
    ],
  },
};

// A line with every identifier masked (string literals kept), to spot renamed twins.
const shapeOf = (line) => line.replace(/("[^"]*"|'[^']*')|[A-Za-z_]\w*/g, (m, quoted) => quoted || 'ID');
const CODE_FILTERS =['all', 'same', 'diff'];
const CODE_FILTER_KEY = { all: 'showAll', same: 'showSame', diff: 'showDiff' };
const FONT_MIN = 10;
const FONT_MAX = 20;

const POLL_MS = 2000;
const PAGE = 50;
const SOURCE_PAGE = 400;

const CHIP = {
  pending: 'bg-warn/15 text-warn',
  confirmed: 'bg-bad/15 text-bad',
  false_positive: 'bg-info/15 text-info',
  ignored: 'bg-panel2 text-mute',
  needs_reevaluation: 'bg-brand/15 text-brand',
};
const FILE_STATUS = {
  pending: 'fPending', accepted: 'fAccepted', rejected: 'fRejected', in_review: 'fInReview', ignored: 'fIgnored',
};
const FILE_CHIP = {
  pending: 'bg-panel2 text-mute',
  accepted: 'bg-ok/15 text-ok',
  rejected: 'bg-bad/15 text-bad',
  in_review: 'bg-warn/15 text-warn',
  ignored: 'bg-info/15 text-info',
};
const FILTER_ON ='border-brand bg-brand/15 text-brand';
const FILTER_OFF = 'border-line text-mute hover:bg-panel2';
const NAV_ON = 'bg-brand/15 text-brand';
const NAV_OFF = 'text-mute';
const ROW_ON = 'border-brand bg-brand/10';
const ROW_OFF = 'border-line';
// Side columns of the editor depend on which panels are open.
// A..Z, then AA, AB... (spreadsheet style) so groups of any size keep letter tags
function letterTag(i) {
  let n = i + 1;
  let out = "";
  while (n > 0) {
    const r = (n - 1) % 26;
    out = String.fromCharCode(65 + r) + out;
    n = Math.floor((n - 1) / 26);
  }
  return out;
}

const EDITOR_COLS = {
  'true,true': 'xl:grid-cols-[176px_minmax(0,1fr)_300px]',
  'true,false': 'xl:grid-cols-[176px_minmax(0,1fr)]',
  'false,true': 'xl:grid-cols-[minmax(0,1fr)_300px]',
  'false,false': '',
};
const fill = (text, vars) => text.replace(/\{(\w+)\}/g, (_, key) => String(vars[key] ?? ''));

function store(key, value) {
  try {
    if (value === undefined) return localStorage.getItem(key);
    localStorage.setItem(key, value);
  } catch (_) { /* storage blocked: the page still works */ }
  return null;
}

function csrf() {
  const meta = document.querySelector('meta[name="csrf-token"]');
  return meta ? meta.content : '';
}

async function getJson(path) {
  const response = await fetch(path, { headers: { Accept: 'application/json' }, cache: 'no-store' });
  if (!response.ok) { const error = new Error(`HTTP ${response.status}`); error.status = response.status; throw error; }
  return response.json();
}

async function postJson(path, body) {
  const response = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf() },
    body: JSON.stringify(body),
  });
  if (!response.ok) { const error = new Error(`HTTP ${response.status}`); error.status = response.status; throw error; }
  return response.json();
}

// One server page only. Offsets are always pinned to the returned snapshot.
async function getPage(path, params) {
  return getJson(`${path}?${new URLSearchParams(params)}`);
}

const dataOf = (event) => event.currentTarget.dataset.v;
const baseName = (path) => path.split('/').pop();
// Last two directories of a path: tells apart the many files called lib.rs.
const dirOf = (path) => path.split('/').slice(0, -1).slice(-2).join('/');
const excerptLines = (member) => (Array.isArray(member.excerpt) ? member.excerpt : String(member.excerpt || '').split('\n'));

// Small deterministic force layout; nodes are few (<= 150).
function layout(nodes, edges, width, height) {
  const count = nodes.length;
  nodes.forEach((node, at) => {
    const angle = (at / Math.max(count, 1)) * Math.PI * 2;
    const radius = Math.min(width, height) * 0.35;
    node.x = width / 2 + Math.cos(angle) * radius;
    node.y = height / 2 + Math.sin(angle) * radius;
    node.vx = 0;
    node.vy = 0;
  });
  for (let step = 0; step < 260; step += 1) {
    const cool = 1 - step / 260;
    for (let a = 0; a < count; a += 1) {
      for (let b = a + 1; b < count; b += 1) {
        let dx = nodes[a].x - nodes[b].x;
        let dy = nodes[a].y - nodes[b].y;
        const dist2 = dx * dx + dy * dy + 0.01;
        const force = 1800 / dist2;
        const dist = Math.sqrt(dist2);
        dx /= dist; dy /= dist;
        nodes[a].vx += dx * force; nodes[a].vy += dy * force;
        nodes[b].vx -= dx * force; nodes[b].vy -= dy * force;
      }
    }
    for (const [from, to] of edges) {
      const dx = nodes[to].x - nodes[from].x;
      const dy = nodes[to].y - nodes[from].y;
      const dist = Math.sqrt(dx * dx + dy * dy) + 0.01;
      const force = (dist - 70) * 0.02;
      nodes[from].vx += (dx / dist) * force; nodes[from].vy += (dy / dist) * force;
      nodes[to].vx -= (dx / dist) * force; nodes[to].vy -= (dy / dist) * force;
    }
    for (const node of nodes) {
      node.vx += (width / 2 - node.x) * 0.004;
      node.vy += (height / 2 - node.y) * 0.004;
      node.x += Math.max(-12, Math.min(12, node.vx)) * cool;
      node.y += Math.max(-12, Math.min(12, node.vy)) * cool;
      node.vx *= 0.6; node.vy *= 0.6;
      node.x = Math.max(14, Math.min(width - 14, node.x));
      node.y = Math.max(14, Math.min(height - 14, node.y));
    }
  }
}

document.addEventListener('alpine:init', () => {
  Alpine.data('dashboard', () => ({
    lang: store('mct.lang') || ((navigator.language || 'es').slice(0, 2) === 'en' ? 'en' : 'es'),
    theme: store('mct.theme') === 'light' ? 'light' : 'dark',
    view: 'home',
    // editor tab: one duplicate, opened from the list in its own browser tab
    editor: false,
    missing: false,
    codeFilter: 'all',
    codeQuery: '',
    wrapLines: true,
    fontPx: 12,
    codeFilters: [],
    homeLinks: [],
    // server data
    summary: null,
    dups: [],
    deads: [],
    token: '',
    busy: false,
    offline: false,
    warning: '',
    toast: '',
    // list
    query: '',
    filter: 'all',
    rows: [],
    filters: [],
    nav: [],
    listCount: '',
    orphans: [],
    orphanInfoRows: [],
    pageOffset: 0,
    pageTotal: 0,
    pageSnapshot: '',
    pageCounts: {},
    pageRequest: 0,
    pageLoading: false,
    detailRequest: 0,
    sourceRequest: 0,
    sourceLoading: false,
    sourceOffset: 0,
    sourceHasNext: false,
    sourcePartial: false,
    sourceUnavailable: false,
    drafts: {},
    findingDrafts: {},
    activeDraftKey: '',
    draftSequence: 0,
    savingFiles: {},
    reviewAction: 'restore',
    reviewReason: '',
    reviewDirty: false,
    reviewSaving: false,
    reviewConflict: false,
    fileConflict: false,
    reviewFingerprint: '',
    reviewRevision: 0,
    activeReviewKey: '',
    reviewChoices: [],
    reviewStateText: '',
    empty: '',
    sel: '',
    selItem: null,
    detail: { files: [] },
    showFiles: store('mct.files') !== '0',
    showAside: store('mct.aside') !== '0',
    promptText: '',
    fileIdx: '0',
    fileStatus: 'pending',
    fileNote: '',
    fileDirty: false,
    fileRevision: 0,
    fileRows: [],
    tabs: [],
    closed: [],
    fileStatuses: [],
    dupFooter: '',
    panes: [],
    deadInfo: { name: '', where: '', meta: '' },
    activeFile: { tag: '', base: '', dir: '', tagCls: '' },
    // obsidian
    cards: [],
    vault: false,
    noteQuery: '',
    noteRows: [],
    notesCount: '',
    note: { name: '', title: '', text: '', links: [], backlinks: [] },
    hasNote: false,
    graphFocus: '',
    graphHint: '',
    hover: '',
    graphData: null,
    toastTimer: 0,

    init() {
      document.documentElement.classList.toggle('light', this.theme === 'light');
      document.documentElement.classList.toggle('dark', this.theme === 'dark');
      document.documentElement.lang = this.lang;
      const query = new URLSearchParams(window.location.search);
      const id = query.get('id') || '';
      if (query.get('view') === 'editor' && /^[\w.-]{1,128}$/.test(id)) {
        this.editor = true;
        this.view = 'dup';
        this.sel = id;
        this.pageOffset = Math.max(0, Number(query.get('offset')) || 0);
        this.query = query.get('q') || '';
        this.filter = query.get('status') || 'all';
        document.title = `${this.t('editorTitle')} · MCT`;
      }
      this.reloadAll();
      setInterval(() => this.poll(), POLL_MS);
      window.addEventListener('resize', () => this.drawGraph());
      window.addEventListener('beforeunload', (event) => {
        if (!this.hasDrafts) return;
        event.preventDefault(); event.returnValue = '';
      });
    },

    t(key) { return I18N[this.lang][key] || key; },
    get L() { return I18N[this.lang]; },
    get fileSelIdx() { return Number(this.fileIdx) || 0; },
    get noRows() { return this.rows.length === 0; },
    get noSel() { return !this.editor && (this.sel === '' || this.view === 'dup'); },
    get pickText() { return this.view === 'dup' ? this.t('openHint') : this.t('pick'); },
    get showOrphans() { return this.view === 'orphans'; },
    get noOrphans() { return this.orphans.length === 0; },
    get hasOrphanInfo() { return this.orphanInfoRows.length > 0; },
    get previousDisabled() { return this.pageLoading || this.pageOffset === 0; },
    get nextDisabled() { return this.pageLoading || this.pageOffset + PAGE >= this.pageTotal; },
    get sourcePreviousDisabled() { return this.sourceLoading || this.sourceOffset === 0; },
    get sourceNextDisabled() { return this.sourceLoading || !this.sourceHasNext; },
    get showReview() { return Boolean(this.selItem) && (this.showDead || this.showDup); },
    get reviewSaveDisabled() { return this.reviewSaving || !this.reviewFingerprint; },
    get hasConflict() { return this.reviewConflict || this.fileConflict; },
    get hasDrafts() { return this.fileDirty || this.reviewDirty || Object.keys(this.drafts).length > 0 || Object.keys(this.findingDrafts).length > 0; },
    get findingKind() { return this.view === 'dup' ? 'duplicate' : 'dead'; },
    get showHome() { return this.view === 'home'; },
    get listMode() { return !this.editor; },
    get mainCls() { return this.editor ? 'lg:grid-cols-1' : 'lg:grid-cols-[340px_minmax(0,1fr)]'; },
    get editorCols() { return EDITOR_COLS[`${this.showFiles},${this.showAside}`]; },
    get filesBtnCls() { return this.showFiles ? FILTER_ON : FILTER_OFF; },
    get asideBtnCls() { return this.showAside ? FILTER_ON : FILTER_OFF; },
    get hasClosed() { return this.closed.length > 0; },
    get canCloseAll() { return this.tabs.length > 0; },
    get noneOpen() { return this.tabs.length === 0 && this.closed.length > 0; },
    get showToolbar() { return this.editor && !this.missing; },
    get showMain() { return this.findings && !this.gone; },
    get gone() { return this.editor && this.missing; },
    applyFont() { document.documentElement.style.setProperty('--code-fs', `${this.fontPx}px`); },
    get wrapCls() { return this.wrapLines ? 'whitespace-pre-wrap break-all' : 'whitespace-pre'; },
    // unwrapped: the rows grow to the longest line so their background spans the whole scroll width
    get boxCls() { return this.wrapLines ? '' : 'w-max min-w-full'; },
    get wrapBtnCls() { return this.wrapLines ? FILTER_ON : FILTER_OFF; },
    get noCode() { return this.panes.length > 0 && this.panes.every((p) => p.rows.length === 0); },
    get noVault() { return this.vault === false; },
    get showObs() { return this.view === 'obsidian'; },
    get langName() { return this.t('langName'); },
    get themeName() { return this.theme === 'dark' ? this.t('themeDark') : this.t('themeLight'); },
    get dark() { return this.theme === 'dark'; },
    get light() { return this.theme === 'light'; },
    get project() { return this.summary ? this.summary.project : ''; },
    get snapshotAt() { return this.summary ? this.fmt(this.summary.snapshot_at) : ''; },
    get findings() { return this.view === 'dup' || this.view === 'dead'; },
    get showDup() { return Boolean(this.selItem) && this.view === 'dup' && this.editor; },
    get showDead() { return Boolean(this.selItem) && this.view === 'dead'; },
    get listTitle() { return this.view === 'dup' ? this.t('dupTitle') : this.t('deadTitle'); },
    isView(name) { return this.view === name; },

    fmt(seconds) {
      if (!seconds) return '';
      return new Date(seconds * 1000).toLocaleString(this.lang === 'en' ? 'en-GB' : 'es-ES');
    },
    stateOf(item) { return item ? item.state : { status: 'pending' }; },
    statusLabel(state) { return this.t(state.status); },
    flash(message) {
      this.toast = message;
      clearTimeout(this.toastTimer);
      this.toastTimer = setTimeout(() => { this.toast = ''; }, 3500);
    },

    // ---- loading -------------------------------------------------------
    async reloadAll() {
      this.stashFileDraft();
      this.stashReviewDraft();
      try {
        const [summary, version] = await Promise.all([getJson('api/summary'), getJson('api/version')]);
        this.summary = summary;
        this.warning = summary.warning || '';
        this.token = version.token;
        this.busy = Boolean(version.rebuilding);
        this.offline = false;
        this.buildCards();
        if (this.findings || this.showOrphans) await this.loadPage(true);
        else this.rebuildLists();
        if (this.sel) await this.loadDetail();
        if (this.view === 'obsidian') await this.loadObsidian();
      } catch (_) { this.offline = true; }
    },

    async loadPage(resetSnapshot = false) {
      const request = ++this.pageRequest;
      const view = this.view;
      const route = view === 'orphans' ? 'api/reviews' : (view === 'dup' ? 'api/duplicates' : 'api/dead');
      const params = { offset: this.pageOffset, limit: PAGE, q: this.query.trim(), status: view === 'orphans' ? 'orphan' : this.filter };
      if (!resetSnapshot && this.pageSnapshot) params.snapshot = this.pageSnapshot;
      this.pageLoading = true;
      try {
        let data;
        try { data = await getPage(route, params); }
        catch (error) {
          if (error.status !== 409) throw error;
          // Start a new page instead of appending rows from another generation.
          data = await getPage(route, { ...params, offset: 0, snapshot: '' });
        }
        if (request !== this.pageRequest || view !== this.view) return;
        if (data.total > 0 && data.offset >= data.total) {
          this.pageOffset = 0;
          return this.loadPage(true);
        }
        this.pageOffset = data.offset || 0;
        this.pageTotal = data.total;
        this.pageSnapshot = data.snapshot;
        this.pageCounts = data.counts || {};
        if (view === 'orphans') {
          this.orphans = data.items.map((item) => ({
            ...item, key: `${item.kind}:${item.identity}`,
            label: `${this.t(item.kind === 'dead' ? 'navDead' : 'navDup')} · ${item.identity.slice(0, 12)}`,
            when: this.fmt(item.at),
          }));
        } else if (view === 'dup') this.dups = data.items;
        else this.deads = data.items;
        this.rebuildLists();
      } catch (error) {
        if (request === this.pageRequest) this.flash(this.t(error.status === 409 ? 'conflict' : 'errLoad'));
      } finally { if (request === this.pageRequest) this.pageLoading = false; }
    },
    async previousPage() {
      if (this.previousDisabled) return;
      this.stashFileDraft(); this.stashReviewDraft();
      this.pageOffset = Math.max(0, this.pageOffset - PAGE);
      await this.loadPage();
    },
    async nextPage() {
      if (this.nextDisabled) return;
      this.stashFileDraft(); this.stashReviewDraft();
      this.pageOffset += PAGE;
      await this.loadPage();
    },

    async poll() {
      if (document.hidden) return;
      try {
        const version = await getJson('api/version');
        this.offline = false;
        this.busy = Boolean(version.rebuilding);
        if (version.token !== this.token && !version.rebuilding) await this.reloadAll();
      } catch (_) {
        this.offline = true;
      }
    },

    // ---- lists ---------------------------------------------------------
    items() { return this.view === 'dup' ? this.dups : this.deads; },
    rowOf(item) {
      const state = item.state;
      const chip = { chip: this.statusLabel(state), chipCls: CHIP[state.status] || CHIP.pending };
      const cls = item.identity === this.sel ? ROW_ON : ROW_OFF;
      if (this.view === 'dup') {
        const first = item.members[0];
        return {
          identity: item.identity, cls, ...chip,
          badge: `${item.members.length}×`, language: item.language,
          title: first.symbol || baseName(first.path), sub: first.path,
          meta: `${item.line_count} ${this.t('lines')} · ${item.token_count} ${this.t('tokens')}`,
        };
      }
      return {
        identity: item.identity, cls, ...chip,
        badge: item.kind, language: item.language,
        title: item.name, sub: `${item.path}:${item.line}`,
        meta: item.parent || '',
      };
    },
    haystack(item) {
      if (this.view === 'dup') {
        return [item.language, ...item.members.map((m) => `${m.path} ${m.symbol || ''}`)].join(' ').toLowerCase();
      }
      return `${item.name} ${item.path} ${item.kind} ${item.language} ${item.parent || ''}`.toLowerCase();
    },
    rebuildLists() {
      const all = this.items();
      this.filters = ['all', ...Object.keys(CHIP)].map((key) => ({
        key, label: this.pageCounts[key] === undefined ? this.t(key) : `${this.t(key)} (${this.pageCounts[key]})`,
        cls: key === this.filter ? FILTER_ON : FILTER_OFF,
      }));
      this.rows = this.editor ? [] : all.map((item) => this.rowOf(item));
      const shown = this.showOrphans ? this.orphans.length : all.length;
      this.listCount = `${this.pageTotal ? this.pageOffset + 1 : 0}–${this.pageOffset + shown} / ${this.pageTotal}`;
      this.empty = this.t('emptyList');
      const summary = this.summary || {};
      this.nav = [
        { view: 'home', icon: '#i-house', label: this.t('navHome'), count: 0 },
        { view: 'dup', icon: '#i-copy', label: this.t('navDup'), count: summary.duplicates_pending || 0 },
        { view: 'dead', icon: '#i-file-x', label: this.t('navDead'), count: summary.dead_pending || 0 },
        { view: 'orphans', icon: '#i-file-x', label: this.t('navOrphans'), count: summary.orphan_findings || 0 },
        { view: 'obsidian', icon: '#i-network', label: this.t('navObs'), count: 0 },
      ].map((entry) => ({ ...entry, cls: entry.view === this.view ? NAV_ON : NAV_OFF }));
      this.homeLinks = this.nav.filter((entry) => entry.view !== 'home').map((entry) => ({
        ...entry, hint: entry.count ? `${entry.count} ${this.t('pending').toLowerCase()}` : '',
      }));
      const selected = all.find((item) => item.identity === this.sel);
      if (selected) this.selItem = selected;
      else if (!this.sel) this.selItem = null;
      this.rebuildDetail();
    },
    async refilter(event) {
      if (event && event.target) this.query = event.target.value;
      this.pageOffset = 0; this.pageSnapshot = '';
      await this.loadPage(true);
    },
    async setFilter(event) { this.filter = dataOf(event); await this.refilter(); },
    async go(event) {
      const view = dataOf(event);
      if (view === this.view) return;
      this.stashFileDraft(); this.stashReviewDraft();
      this.view = view;
      this.sel = ''; this.selItem = null;
      this.activeDraftKey = ''; this.activeReviewKey = '';
      this.filter = 'all'; this.query = '';
      this.pageOffset = 0; this.pageSnapshot = ''; this.pageCounts = {};
      this.orphanInfoRows = [];
      this.rebuildLists();
      if (view === 'obsidian') await this.loadObsidian();
      else if (this.findings || this.showOrphans) await this.loadPage(true);
    },

    async inspectOrphan(event) {
      const key = dataOf(event);
      const item = this.orphans.find((row) => row.key === key);
      if (!item) return;
      try {
        const data = await getJson(`api/finding?kind=${item.kind}&id=${item.identity}`);
        this.orphanInfoRows = [
          ...data.history.map((row) => `${this.fmt(row.at)} · ${this.t(row.status)} · ${row.reason || ''}`),
          ...data.comments.map((row) => `${this.fmt(row.updated_at)} · ${row.body}`),
          ...data.files.map((row) => `${row.path} · ${this.t(FILE_STATUS[row.status])} · ${row.note || ''}`),
        ];
      } catch (_) { this.flash(this.t('errLoad')); }
    },
    async deleteOrphan(event) {
      const key = dataOf(event);
      const item = this.orphans.find((row) => row.key === key);
      if (!item) return;
      try {
        await postJson('api/review/delete', { kind: item.kind, id: item.identity, expected_revision: item.revision });
        this.orphanInfoRows = [];
        await this.reloadAll();
      } catch (error) { this.flash(this.t(error.status === 409 ? 'conflict' : 'errSave')); }
    },

    // ---- editor tab: filters, zoom, copy, stepping ----------------------
    setCodeFilter(event) { this.codeFilter = dataOf(event); this.rebuildDetail(); },
    searchCode(event) { this.codeQuery = event.target.value; this.rebuildDetail(); },
    toggleWrap() { this.wrapLines = !this.wrapLines; this.rebuildDetail(); },
    fontUp() { this.fontPx = Math.min(FONT_MAX, this.fontPx + 1); this.applyFont(); },
    fontDown() { this.fontPx = Math.max(FONT_MIN, this.fontPx - 1); this.applyFont(); },
    async copyCode() {
      const pane = this.panes[this.fileSelIdx];
      const item = this.selItem;
      if (!pane || !item) return;
      const text = excerptLines(item.members[pane.idx]).join('\n');
      try {
        await navigator.clipboard.writeText(text);
        this.flash(this.t('copied'));
      } catch (_) {
        this.flash(this.t('errSave'));
      }
    },
    prevDup() { this.stepDup(-1); },
    nextDup() { this.stepDup(1); },
    async stepDup(delta) {
      this.stashFileDraft(); this.stashReviewDraft();
      let at = this.dups.findIndex((d) => d.identity === this.sel);
      let next = this.dups[at + delta];
      if (!next) {
        const offset = this.pageOffset + (delta > 0 ? PAGE : -PAGE);
        if (offset < 0 || offset >= this.pageTotal) return;
        this.pageOffset = offset;
        await this.loadPage();
        next = delta > 0 ? this.dups[0] : this.dups[this.dups.length - 1];
      }
      if (!next) return;
      this.openFinding(next.identity);
      window.history.replaceState(null, '', `app?view=editor&id=${encodeURIComponent(next.identity)}&offset=${this.pageOffset}`);
      await this.loadDetail();
    },
    closeTab() {
      if (this.hasDrafts) { this.flash(this.t('retainedDraft')); return; }
      window.close();
      // window.close() only works for tabs opened by script; if the tab is still
      // here a moment later, go back to the list instead.
      setTimeout(() => { window.location.href = 'app'; }, 150);
    },
    toggleFiles() { this.showFiles = !this.showFiles; store('mct.files', this.showFiles ? '1' : '0'); },
    toggleAside() { this.showAside = !this.showAside; store('mct.aside', this.showAside ? '1' : '0'); },
    async copyPrompt() {
      if (!this.promptText) return;
      try {
        await navigator.clipboard.writeText(this.promptText);
        this.flash(this.t('copied'));
      } catch (_) {
        this.flash(this.t('errSave'));
      }
    },
    buildCards() {
      const s = this.summary;
      if (!s) return;
      this.cards = [
        [this.t('cFiles'), s.files], [this.t('cDirs'), s.directories], [this.t('cSymbols'), s.symbols],
        [this.t('cImports'), s.imports], [this.t('cNotes'), s.notes], [this.t('cLinks'), s.note_links],
        [this.t('cDup'), s.duplicate_groups], [this.t('cDead'), s.dead_candidates],
      ].map(([label, value]) => ({ label, value: Number(value).toLocaleString(this.lang === 'en' ? 'en-GB' : 'es-ES') }));
    },

    // ---- detail / review ----------------------------------------------
    select(event) {
      const id = dataOf(event);
      if (this.view === 'dup' && !this.editor) {
        const query = new URLSearchParams({ view: 'editor', id, offset: this.pageOffset, q: this.query, status: this.filter });
        const tab = window.open(`app?${query}`, '_blank');
        if (tab) tab.opener = null;
        else window.location.href = `app?${query}`;
        return;
      }
      this.openFinding(id);
      this.loadDetail();
    },
    openFinding(id) {
      this.stashFileDraft(); this.stashReviewDraft();
      this.sel = id; this.selItem = this.items().find((item) => item.identity === id) || null;
      this.fileIdx = '0'; this.closed = []; this.sourceOffset = 0;
      this.activeDraftKey = ''; this.activeReviewKey = '';
      this.fileDirty = false; this.reviewDirty = false;
      this.fileConflict = false; this.reviewConflict = false;
      this.fileStatus = 'pending'; this.fileNote = ''; this.detail = { files: [] };
      this.syncReview();
      this.rebuildLists();
    },
    async loadFullCode(id) {
      const request = ++this.sourceRequest;
      this.sourceLoading = true;
      const params = { id, full: 1, source_offset: this.sourceOffset, source_limit: SOURCE_PAGE };
      if (this.pageSnapshot) params.snapshot = this.pageSnapshot;
      try {
        const data = await getPage('api/duplicates', params);
        if (request !== this.sourceRequest || this.sel !== id) return false;
        const full = data.items[0];
        if (!full) { this.missing = true; return false; }
        const at = this.dups.findIndex((item) => item.identity === id);
        if (at >= 0) this.dups[at] = full;
        this.selItem = full;
        this.missing = false;
        this.sourceHasNext = full.members.some((m) => m.source_next_offset !== null && m.source_next_offset !== undefined);
        this.sourcePartial = this.sourceOffset > 0 || full.members.some((m) => m.excerpt_truncated);
        this.sourceUnavailable = full.members.some((m) => m.excerpt_available === false);
        this.rebuildDetail();
        return !this.sourceUnavailable;
      } catch (error) {
        if (request !== this.sourceRequest) return false;
        this.sourceUnavailable = true;
        if (error.status === 409) { this.reviewConflict = true; this.flash(this.t('conflict')); }
        else this.flash(this.t('errLoad'));
        return false;
      } finally { if (request === this.sourceRequest) this.sourceLoading = false; }
    },
    async previousSource() {
      if (this.sourcePreviousDisabled) return;
      this.sourceOffset = Math.max(0, this.sourceOffset - SOURCE_PAGE);
      await this.loadFullCode(this.sel);
    },
    async nextSource() {
      if (this.sourceNextDisabled) return;
      this.sourceOffset += SOURCE_PAGE;
      await this.loadFullCode(this.sel);
    },
    async loadDetail() {
      if (!this.sel) return;
      const request = ++this.detailRequest;
      const id = this.sel;
      const kind = this.findingKind;
      try {
        const data = await getJson(`api/finding?kind=${kind}&id=${id}`);
        if (request !== this.detailRequest || id !== this.sel || kind !== this.findingKind) return false;
        if (data.current === false) {
          this.stashFileDraft(); this.stashReviewDraft();
          this.detail = data; this.selItem = null; this.missing = this.editor;
          this.sel = ''; this.activeDraftKey = ''; this.activeReviewKey = '';
          this.fileDirty = false; this.reviewDirty = false;
          this.reviewFingerprint = null;
          this.rebuildLists();
          return false;
        }
        this.detail = data;
        if (this.selItem) {
          this.selItem.state = data.state;
          this.selItem.revision = data.revision;
          this.selItem.fingerprint = data.fingerprint;
        }
        if (kind === 'duplicate' && !await this.loadFullCode(id)) return false;
        if (request !== this.detailRequest || id !== this.sel) return false;
        this.syncFile();
        this.syncReview();
        this.rebuildDetail();
        return true;
      } catch (error) { this.flash(this.t(error.status === 409 ? 'conflict' : 'errLoad')); return false; }
    },
    fileKey(id, path) { return `duplicate:${id}:${path}`; },
    stashFileDraft() {
      if (!this.activeDraftKey || !this.fileDirty) return;
      const existing = this.drafts[this.activeDraftKey] || {};
      this.drafts[this.activeDraftKey] = {
        ...existing, note: this.fileNote, status: this.fileStatus,
        expected_revision: existing.expected_revision === undefined ? this.fileRevision : existing.expected_revision,
        generation: existing.generation || ++this.draftSequence,
      };
    },
    syncFile() {
      this.stashFileDraft();
      const item = this.selItem;
      const member = item && item.members ? item.members[this.fileSelIdx] : null;
      const found = member ? (this.detail.files || []).find((row) => row.path === member.path) : null;
      this.activeDraftKey = member ? this.fileKey(this.sel, member.path) : '';
      const draft = this.drafts[this.activeDraftKey];
      if (draft && this.detail.identity === this.sel && draft.expected_revision !== (found ? found.revision : 0)) draft.conflict = true;
      this.fileStatus = draft ? draft.status : (found ? found.status : 'pending');
      this.fileNote = draft ? draft.note : (found ? found.note : '');
      this.fileRevision = draft ? draft.expected_revision : (found ? found.revision : 0);
      this.fileDirty = Boolean(draft);
      this.fileConflict = Boolean(draft && draft.conflict);
    },
    changeFile(event) {
      this.stashFileDraft();
      this.fileIdx = String(event.target.value);
      this.closed = this.closed.filter((idx) => idx !== this.fileSelIdx);
      this.syncFile(); this.rebuildDetail();
    },
    pickFile(event) {
      this.stashFileDraft();
      const idx = Number(dataOf(event));
      this.closed = this.closed.filter((closed) => closed !== idx);
      this.fileIdx = String(idx); this.syncFile(); this.rebuildDetail();
    },
    closeFile(event) {
      this.stashFileDraft();
      const idx = Number(dataOf(event));
      const total = this.selItem && this.selItem.members ? this.selItem.members.length : 0;
      if (this.closed.length + 1 >= total) return;
      this.closed = [...this.closed, idx];
      if (this.fileSelIdx === idx) {
        this.fileIdx = String([...Array(total).keys()].find((n) => !this.closed.includes(n)));
        this.syncFile();
      }
      this.rebuildDetail();
    },
    soloFile(event) {
      this.stashFileDraft();
      const keep = Number(dataOf(event));
      const total = this.selItem && this.selItem.members ? this.selItem.members.length : 0;
      this.closed = [...Array(total).keys()].filter((n) => n !== keep);
      this.fileIdx = String(keep); this.syncFile(); this.rebuildDetail();
    },
    openAll() { this.closed = []; this.rebuildDetail(); },
    closeAll() {
      const total = this.selItem && this.selItem.members ? this.selItem.members.length : 0;
      this.closed = [...Array(total).keys()]; this.rebuildDetail();
    },
    setFileStatus(event) { this.fileStatus = dataOf(event); this.markDirty(); this.rebuildDetail(); },
    markDirty(event) {
      if (event && event.target) this.fileNote = event.target.value;
      const item = this.selItem;
      const member = item && item.members ? item.members[this.fileSelIdx] : null;
      if (!member) return;
      const key = this.fileKey(this.sel, member.path);
      const prior = this.drafts[key];
      const found = (this.detail.files || []).find((row) => row.path === member.path);
      this.activeDraftKey = key; this.fileDirty = true;
      this.drafts[key] = {
        note: this.fileNote, status: this.fileStatus, generation: ++this.draftSequence,
        expected_revision: prior ? prior.expected_revision : (found ? found.revision : 0),
        conflict: prior ? prior.conflict : false,
      };
    },
    discardFileDraft() {
      delete this.drafts[this.activeDraftKey]; this.fileDirty = false; this.fileConflict = false;
      this.syncFile(); this.rebuildDetail();
    },
    async saveFile() {
      const member = this.selItem && this.selItem.members ? this.selItem.members[this.fileSelIdx] : null;
      if (!member) return;
      const id = this.sel;
      const key = this.fileKey(id, member.path);
      this.stashFileDraft();
      if (!this.drafts[key]) this.markDirty();
      if (this.savingFiles[key]) return;
      const sent = { ...this.drafts[key] };
      this.savingFiles[key] = true;
      try {
        const response = await postJson('api/file-review', {
          id, path: member.path, status: sent.status, note: sent.note.trim(), expected_revision: sent.expected_revision,
        });
        this.pageSnapshot = '';
        const current = this.drafts[key];
        if (current && current.generation === sent.generation) delete this.drafts[key];
        else if (current) current.expected_revision = response.revision;
        if (id === this.sel && this.findingKind === 'duplicate') {
          // Stop the old form from recreating the just-saved draft during sync.
          if (this.activeDraftKey === key && !this.drafts[key]) this.fileDirty = false;
          await this.loadDetail();
        }
        this.flash(this.t('saved'));
      } catch (error) {
        if (error.status === 409 && this.drafts[key]) {
          this.drafts[key].conflict = true;
          if (this.activeDraftKey === key) this.fileConflict = true;
        }
        this.flash(this.t(error.status === 409 ? 'conflict' : 'errSave'));
      } finally { delete this.savingFiles[key]; }
    },
    stashReviewDraft() {
      if (!this.activeReviewKey || !this.reviewDirty) return;
      const draft = this.findingDrafts[this.activeReviewKey] || {};
      this.findingDrafts[this.activeReviewKey] = {
        ...draft, action: this.reviewAction, reason: this.reviewReason,
        expected_revision: draft.expected_revision === undefined ? this.reviewRevision : draft.expected_revision,
        fingerprint: draft.fingerprint === undefined ? this.reviewFingerprint : draft.fingerprint,
        generation: draft.generation || ++this.draftSequence,
      };
    },
    syncReview() {
      this.stashReviewDraft();
      this.activeReviewKey = `${this.findingKind}:${this.sel}`;
      const base = this.detail.identity === this.sel ? this.detail : (this.selItem || {});
      const state = base.state || { status: 'pending' };
      const draft = this.findingDrafts[this.activeReviewKey];
      if (draft && (draft.fingerprint !== base.fingerprint || draft.expected_revision !== base.revision)) draft.conflict = true;
      this.reviewAction = draft ? draft.action : (['confirmed', 'false_positive', 'ignored'].includes(state.status) ? state.status : 'restore');
      this.reviewReason = draft ? draft.reason : (state.reason || '');
      this.reviewRevision = draft ? draft.expected_revision : (base.revision || 0);
      this.reviewFingerprint = draft ? draft.fingerprint : base.fingerprint;
      this.reviewDirty = Boolean(draft);
      this.reviewConflict = Boolean(draft && draft.conflict);
      this.reviewStateText = this.t(state.status);
      this.buildReviewChoices();
    },
    buildReviewChoices() {
      this.reviewChoices = ['confirmed', 'false_positive', 'ignored', 'restore'].map((key) => ({
        key, label: this.t(key), on: key === this.reviewAction, cls: key === this.reviewAction ? FILTER_ON : FILTER_OFF,
      }));
    },
    setReviewAction(event) { this.reviewAction = dataOf(event); this.markReviewDirty(); this.buildReviewChoices(); },
    markReviewDirty(event) {
      if (event && event.target) this.reviewReason = event.target.value;
      this.reviewDirty = true;
      this.stashReviewDraft();
      this.findingDrafts[this.activeReviewKey].generation = ++this.draftSequence;
    },
    discardReviewDraft() {
      delete this.findingDrafts[this.activeReviewKey]; this.reviewDirty = false; this.reviewConflict = false;
      this.syncReview(); this.rebuildDetail();
    },
    async saveReview() {
      if (!this.sel || !this.reviewFingerprint || this.reviewSaving) return;
      this.stashReviewDraft();
      if (!this.reviewDirty) this.markReviewDirty();
      const key = this.activeReviewKey;
      const id = this.sel, kind = this.findingKind;
      const sent = { ...this.findingDrafts[key] };
      this.reviewSaving = true;
      try {
        const response = await postJson('api/review', {
          kind, id, action: sent.action, reason: sent.reason, fingerprint: sent.fingerprint, expected_revision: sent.expected_revision,
        });
        this.pageSnapshot = '';
        const current = this.findingDrafts[key];
        if (current && current.generation === sent.generation) delete this.findingDrafts[key];
        else if (current) current.expected_revision = response.revision;
        if (id === this.sel && kind === this.findingKind) {
          if (!this.findingDrafts[key]) this.reviewDirty = false;
          await this.loadDetail();
        }
        this.flash(this.t('saved'));
      } catch (error) {
        if (error.status === 409 && this.findingDrafts[key]) {
          this.findingDrafts[key].conflict = true; this.reviewConflict = true;
        }
        this.flash(this.t(error.status === 409 ? 'conflict' : 'errSave'));
      } finally { this.reviewSaving = false; }
    },
    async reviewLatest() {
      // Explicit user acknowledgement: fetch and display the current source,
      // then attach retained drafts to that version. Never rebase on a poll.
      this.pageSnapshot = '';
      await this.loadPage(true);
      if (!await this.loadDetail()) return;
      const file = this.drafts[this.activeDraftKey];
      if (file) {
        const member = this.selItem && this.selItem.members ? this.selItem.members[this.fileSelIdx] : null;
        const found = member && this.detail.files.find((row) => row.path === member.path);
        file.expected_revision = found ? found.revision : 0; file.conflict = false;
      }
      const decision = this.findingDrafts[this.activeReviewKey];
      if (decision) {
        decision.expected_revision = this.detail.revision; decision.fingerprint = this.detail.fingerprint;
        decision.conflict = false;
        this.reviewRevision = decision.expected_revision; this.reviewFingerprint = decision.fingerprint;
      }
      this.reviewConflict = false; this.fileConflict = false;
    },
    rebuildDetail() {
      const item = this.selItem;
      if (!item) {
        this.fileRows = [];
        this.panes = [];
        this.promptText = '';
        return;
      }
      if (this.view === 'dup') {
        const tags = item.members.map((_, i) => letterTag(i));
        const picked = this.fileSelIdx;
        const lists = item.members.map((m) => excerptLines(m));
        const sets = lists.map((list) => new Set(list.map((line) => line.trim()).filter(Boolean)));
        const shapes = lists.map((list) => new Set(list.map((line) => shapeOf(line.trim())).filter(Boolean)));
        const states = new Map((this.detail.files || []).map((f) => [f.path, f.status]));
        const closed = this.closed;
        const canClose = item.members.length - closed.length > 1;
        this.fileRows = item.members.map((m, idx) => {
          const status = states.get(m.path) || 'pending';
          const active = idx === picked;
          const shut = closed.includes(idx);
          return {
            idx, path: m.path, base: baseName(m.path), dir: dirOf(m.path), tag: tags[idx],
            label: `${tags[idx]} · ${baseName(m.path)} (${dirOf(m.path)})`,
            chip: this.t(FILE_STATUS[status]), chipCls: FILE_CHIP[status], open: !shut, closable: canClose && !shut,
            cls: active ? 'border-brand bg-brand/25 ring-1 ring-brand' : (shut ? 'border-line opacity-50' : ROW_OFF),
            tabCls: active ? 'border-brand border-t-2 bg-panel text-ink' : 'border-line bg-panel2 text-mute',
            tagCls: active ? 'bg-brand text-white' : 'bg-line text-mute',
          };
        });
        this.tabs = this.fileRows.filter((f) => f.open);
        this.activeFile = this.fileRows[picked] || this.activeFile;
        this.panes = item.members.map((m, idx) => {
          let total = 0;
          let same = 0;
          const mode = this.codeFilter;
          const needle = this.codeQuery.trim().toLowerCase();
          const rows = [];
          lists[idx].forEach((s, n) => {
            const key = s.trim();
            let cls = '';
            let shared = false;
            if (key) {
              total += 1;
              // Other files are judged against the active one, so a single odd file
              // (e.g. `pub(super) fn`) shows red only there; the active file itself
              // is judged by the majority of the other open files.
              const judge = (pool, probe) => {
                if (idx !== picked && !closed.includes(picked)) return pool[picked].has(probe);
                const others = pool.filter((_, at) => at !== idx && !closed.includes(at));
                return others.length === 0 || others.filter((set) => set.has(probe)).length * 2 >= others.length;
              };
              const exact = judge(sets, key);
              // Same line with other identifiers (a renamed twin) is not "different".
              const renamed = !exact && judge(shapes, shapeOf(key));
              shared = exact || renamed;
              if (shared) same += 1;
              cls = exact ? 'row-same' : (renamed ? 'row-ren' : 'row-diff');
            }
            if (mode === 'same' && !shared) return;
            if (mode === 'diff' && (shared || !key)) return;
            if (needle && !s.toLowerCase().includes(needle)) return;
            rows.push({ n: (m.excerpt_first || m.start_line) + n, s, cls });
          });
          return {
            idx, tag: tags[idx], path: m.path, rows, empty: rows.length === 0,
            lines: `${m.start_line}–${m.end_line} · ${m.line_count} ${this.t('lines')}`,
            similarity: `${total ? Math.round((same * 100) / total) : 0}%`,
            cls: idx === picked ? 'border-brand ring-1 ring-brand' : 'border-line',
            tagCls: idx === picked ? 'bg-brand text-white' : 'bg-line text-mute',
            multi: item.members.length - closed.length > 1,
          };
        }).filter((p) => !closed.includes(p.idx));
        this.promptText = this.dupPrompt(item, tags);
        this.codeFilters = CODE_FILTERS.map((key) => ({
          key, label: this.t(CODE_FILTER_KEY[key]), cls: key === this.codeFilter ? FILTER_ON : FILTER_OFF,
        }));
        this.dupFooter = `${this.t('dupWord')} #${this.pageOffset + Math.max(0, this.dups.findIndex((d) => d.identity === item.identity)) + 1} ${this.t('ofWord')} ${this.pageTotal} ·${item.line_count} ${this.t('lines')} · ${item.members.length} ${this.t('filesWord')}`;
        this.fileStatuses = Object.keys(FILE_STATUS).map((key) => ({
          key, label: this.t(FILE_STATUS[key]), on: key === this.fileStatus,
          cls: key === this.fileStatus ? FILTER_ON : FILTER_OFF,
          dot: key === this.fileStatus ? 'bg-current' : '',
        }));
      } else {
        this.deadInfo = {
          name: item.name,
          where: `${item.path}:${item.line}${item.end_line ? `–${item.end_line}` : ''}`,
          meta: [item.kind, item.language, item.parent].filter(Boolean).join(' · '),
        };
        this.promptText = this.deadPrompt(item);
      }
    },
    // Copy-paste brief for an AI assistant: where the code is and what to do with it.
    dupPrompt(item, tags) {
      const states = new Map((this.detail.files || []).map((f) => [f.path, f]));
      const where = item.members.map((m, idx) => `- ${tags[idx]}: ${m.path} (${this.t('lines')} ${m.start_line}–${m.end_line})`);
      const notes = item.members.map((m, idx) => ({ tag: tags[idx], f: states.get(m.path) }))
        .filter(({ f }) => f && (f.status !== 'pending' || f.note))
        .map(({ tag, f }) => `- ${tag}: ${this.t(FILE_STATUS[f.status])}${f.note ? ` — ${f.note}` : ''}`);
      const intro = fill(this.t('pDupIntro'), {
        n: item.members.length, lines: item.line_count, tokens: item.token_count, lang: item.language,
      });
      return this.composePrompt(intro, where, [...this.findingContext(item), ...notes], this.findingTasks(item, this.t('pDupTask')));
    },
    deadPrompt(item) {
      const end = item.end_line ? `–${item.end_line}` : '';
      const intro = fill(this.t('pDeadIntro'), {
        name: item.name, kind: item.kind, lang: item.language, parent: item.parent ? `, ${item.parent}` : '',
      });
      return this.composePrompt(intro, [`- ${item.path}:${item.line}${end}`], this.findingContext(item), this.findingTasks(item, this.t('pDeadTask')));
    },
    findingContext(item) {
      const state = item.state || { status: 'pending' };
      return [
        `${this.t('pDecision')}: ${this.t(state.status)}${state.previous ? ` (${this.t(state.previous)})` : ''}${state.reason ? ` — ${state.reason}` : ''}`,
        `${this.t('pFingerprint')}: ${item.fingerprint || 'unavailable'}`,
        `${this.t('pScope')}: ${this.t(['false_positive', 'ignored'].includes(state.status) ? 'pResolved' : (state.status === 'needs_reevaluation' ? 'pNeedsReview' : 'pFresh'))}`,
      ];
    },
    findingTasks(item, tasks) {
      return ['false_positive', 'ignored'].includes((item.state || {}).status) ? [this.t('pResolved')] : tasks;
    },
    composePrompt(intro, where, notes, tasks) {
      const parts = [intro, '', this.t('pWhere'), ...where];
      if (notes.length) parts.push('', this.t('pNotes'), ...notes);
      parts.push('', this.t('pTask'), ...tasks.map((task, n) => `${n + 1}. ${task}`));
      return parts.join('\n');
    },
    // ---- obsidian ------------------------------------------------------
    async loadObsidian() {
      this.buildCards();
      await this.searchNotes();
      if (this.vault) await this.loadGraph();
    },
    async searchNotes() {
      try {
        const data = await getJson(`api/notes?q=${encodeURIComponent(this.noteQuery.trim())}&limit=${PAGE}`);
        this.vault = data.enabled;
        this.notesCount = `${data.items.length} / ${data.total} ${this.t('notes')}`;
        this.noteRows = data.items.map((n) => ({
          name: n.name, kind: n.kind, title: n.title, links: `${n.links}→ ${n.backlinks}←`,
          cls: n.name === this.note.name && this.hasNote ? ROW_ON : ROW_OFF,
        }));
      } catch (_) {
        this.flash(this.t('errLoad'));
      }
    },
    async openNote(event) {
      const name = dataOf(event);
      try {
        this.note = await getJson(`api/note?n=${encodeURIComponent(name)}`);
        this.hasNote = true;
        this.noteRows = this.noteRows.map((n) => ({ ...n, cls: n.name === name ? ROW_ON : ROW_OFF }));
      } catch (_) {
        this.flash(this.t('errLoad'));
      }
    },
    closeNote() { this.hasNote = false; },
    async loadGraph() {
      try {
        const data = await getJson(`api/graph?n=${encodeURIComponent(this.graphFocus)}`);
        this.graphData = data;
        this.graphHint = `${data.nodes.length} / ${data.total} ${this.t('nodes')}`;
        await this.$nextTick();
        this.drawGraph(true);
      } catch (_) {
        this.flash(this.t('errLoad'));
      }
    },
    focusCurrent() {
      this.graphFocus = this.note.name;
      this.loadGraph();
    },
    fullGraph() {
      this.graphFocus = '';
      this.loadGraph();
    },
    drawGraph(relayout) {
      const canvas = this.$refs.graph;
      const data = this.graphData && Alpine.raw(this.graphData);
      if (!canvas || !data || this.view !== 'obsidian') return;
      const box = canvas.getBoundingClientRect();
      if (!box.width || !box.height) return;
      const ratio = window.devicePixelRatio || 1;
      canvas.width = Math.round(box.width * ratio);
      canvas.height = Math.round(box.height * ratio);
      const ctx = canvas.getContext('2d');
      ctx.scale(ratio, ratio);
      if (relayout || !this.laidOut) {
        layout(data.nodes, data.edges, box.width, box.height);
        this.laidOut = true;
        this.bindCanvas(canvas);
      }
      const color = (name, alpha) => `rgb(${getComputedStyle(document.documentElement).getPropertyValue(name)} / ${alpha})`;
      ctx.clearRect(0, 0, box.width, box.height);
      ctx.strokeStyle = color('--line', 1);
      ctx.lineWidth = 1;
      for (const [from, to] of data.edges) {
        ctx.beginPath();
        ctx.moveTo(data.nodes[from].x, data.nodes[from].y);
        ctx.lineTo(data.nodes[to].x, data.nodes[to].y);
        ctx.stroke();
      }
      for (const node of data.nodes) {
        ctx.beginPath();
        ctx.arc(node.x, node.y, 3 + Math.sqrt(node.degree) * 1.4, 0, Math.PI * 2);
        ctx.fillStyle = node.focus ? color('--warn', 1) : color('--brand', node.name === this.hover ? 1 : 0.75);
        ctx.fill();
      }
    },
    bindCanvas(canvas) {
      if (this.bound) return;
      this.bound = true;
      const at = (event) => {
        const box = canvas.getBoundingClientRect();
        const x = event.clientX - box.left;
        const y = event.clientY - box.top;
        let best = null;
        let bestDist = Infinity;
        for (const node of (this.graphData ? Alpine.raw(this.graphData).nodes : [])) {
          const dist = Math.hypot(node.x - x, node.y - y);
          const radius = 6 + Math.sqrt(node.degree) * 1.4;
          if (dist < radius && dist < bestDist) { best = node; bestDist = dist; }
        }
        return best;
      };
      canvas.addEventListener('mousemove', (event) => {
        const node = at(event);
        const name = node ? node.name : '';
        canvas.style.cursor = node ? 'pointer' : 'default';
        if (name !== this.hover) { this.hover = name; this.drawGraph(); }
      });
      canvas.addEventListener('mouseleave', () => { this.hover = ''; this.drawGraph(); });
      canvas.addEventListener('click', (event) => {
        const node = at(event);
        if (node) this.openNote({ currentTarget: { dataset: { v: node.name } } });
      });
    },

    // ---- preferences ---------------------------------------------------
    toggleLang() {
      this.lang = this.lang === 'es' ? 'en' : 'es';
      store('mct.lang', this.lang);
      document.documentElement.lang = this.lang;
      this.buildReviewChoices();
      this.rebuildLists();
      if (this.view === 'obsidian') this.loadObsidian();
    },
    toggleTheme() {
      this.theme = this.theme === 'dark' ? 'light' : 'dark';
      store('mct.theme', this.theme);
      document.documentElement.classList.toggle('light', this.theme === 'light');
      document.documentElement.classList.toggle('dark', this.theme === 'dark');
      this.drawGraph();
    },
  }));
});
