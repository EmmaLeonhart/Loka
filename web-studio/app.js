// Loka Studio — JS/HTML replica of the Flutter app (real DOM).
// Emma's spec: "replication of the flutter app in JS, but all of the
// existing html things are tabs in it. JS knowledge graph is best."
// So: a tabbed shell; the engine's existing HTML surfaces (/browse
// vis-network graph, the playground IDE) are tabs; the rest of the
// Flutter screens are ported to JS modules under screens/.

const DEFAULT_ENDPOINT = 'http://localhost:3030';

/* ── LokaClient: 1:1 port of loka-studio/lib/services/loka_client.dart ── */
export class LokaClient {
  constructor(endpoint) { this.endpoint = endpoint; }
  get _base() { return this.endpoint.replace(/\/+$/, ''); }

  async health() {
    try {
      const r = await fetch(this._base + '/health');
      return r.ok;
    } catch { return false; }
  }

  async query(sparql) {
    const r = await fetch(this._base + '/sparql', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/sparql-query',
        'Accept': 'application/sparql-results+json',
      },
      body: sparql,
    });
    if (!r.ok) throw new Error(`Query failed: ${r.status} ${await r.text()}`);
    const j = await r.json();
    const vars = j?.head?.vars ?? [];
    const rows = j?.results?.bindings ?? [];
    return { variables: vars, rows };
  }

  async fetchTriples(limit = 200, offset = 0) {
    const { rows } = await this.query(
      `SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT ${limit} OFFSET ${offset}`);
    return rows;
  }

  async stats() {
    const out = { totalTriples: -1, types: {} };
    try {
      const c = await this.query('SELECT (COUNT(*) AS ?count) WHERE { ?s ?p ?o }');
      out.totalTriples = parseInt(c.rows?.[0]?.count?.value ?? '-1', 10);
      const t = await this.query(
        'SELECT ?type (COUNT(?s) AS ?count) WHERE { ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?type } GROUP BY ?type');
      for (const row of t.rows) {
        const ty = row?.type?.value, n = parseInt(row?.count?.value ?? '0', 10);
        if (ty) out.types[ty] = n;
      }
    } catch { /* COUNT may be unimplemented; leave -1 */ }
    return out;
  }

  async vectorsHealth() {
    try {
      const r = await fetch(this._base + '/vectors/health');
      return r.ok ? await r.json() : {};
    } catch { return {}; }
  }

  async exportGraph(format = 'turtle') {
    const u = this._base + '/graph' + (format === 'ntriples' ? '?format=ntriples' : '');
    const r = await fetch(u);
    if (!r.ok) throw new Error(`Export failed: ${r.status}`);
    return r.text();
  }
}

/* ── Tab registry ──
   kind 'embed'  → an existing engine HTML surface in an <iframe>.
   kind 'screen' → a JS module screens/<id>.js (lazy; placeholder if
                   not built yet — lets later slices drop in cleanly). */
const TABS = [
  { id: 'graph',      label: 'Knowledge Graph', kind: 'embed', src: e => `${e}/browse` },
  { id: 'sparql',     label: 'SPARQL',          kind: 'screen' },
  { id: 'triples',    label: 'Triples',         kind: 'screen' },
  { id: 'health',     label: 'Health',          kind: 'screen' },
  { id: 'ontology',   label: 'Ontology',        kind: 'screen' },
  { id: 'playground', label: 'Playground',      kind: 'embed', src: e => `${e}/` },
];

/* ── Connection state (localStorage, mirrors connection_provider.dart) ── */
const store = {
  get endpoint() {
    try { return localStorage.getItem('loka-studio-endpoint') || DEFAULT_ENDPOINT; }
    catch { return DEFAULT_ENDPOINT; }
  },
  set endpoint(v) { try { localStorage.setItem('loka-studio-endpoint', v); } catch {} },
};

let client = new LokaClient(store.endpoint);

/* ── Boot ── */
const view = document.getElementById('view');
const tabsEl = document.getElementById('tabs');
const epInput = document.getElementById('endpoint');
const connDot = document.getElementById('conn-dot');

for (const t of TABS) {
  const b = document.createElement('button');
  b.textContent = t.label;
  b.dataset.tab = t.id;
  b.onclick = () => { location.hash = t.id; };
  tabsEl.appendChild(b);
}

epInput.value = store.endpoint;
epInput.addEventListener('change', () => {
  const v = epInput.value.trim().replace(/\/+$/, '') || DEFAULT_ENDPOINT;
  store.endpoint = v;
  client = new LokaClient(v);
  pingConn();
  render(); // re-render current tab against the new endpoint
});

async function pingConn() {
  connDot.dataset.state = 'unknown';
  connDot.dataset.state = (await client.health()) ? 'ok' : 'down';
}

document.getElementById('theme-toggle').onclick = () => {
  const cur = document.documentElement.getAttribute('data-theme') === 'light' ? 'light' : 'dark';
  const next = cur === 'light' ? 'dark' : 'light';
  document.documentElement.setAttribute('data-theme', next);
  try { localStorage.setItem('loka-studio-theme', next); } catch {}
};

const screenCache = new Map();

async function render() {
  const id = (location.hash.replace('#', '') || TABS[0].id);
  const tab = TABS.find(t => t.id === id) || TABS[0];
  for (const b of tabsEl.children) b.classList.toggle('active', b.dataset.tab === tab.id);
  view.innerHTML = '';

  if (tab.kind === 'embed') {
    const wrap = document.createElement('div');
    wrap.className = 'screen embed';
    const f = document.createElement('iframe');
    f.src = tab.src(client._base);
    f.title = tab.label;
    wrap.appendChild(f);
    view.appendChild(wrap);
    return;
  }

  // JS screen module (slice 2+). Lazy-import; placeholder until built.
  const host = document.createElement('div');
  host.className = 'screen';
  view.appendChild(host);
  try {
    let mod = screenCache.get(tab.id);
    if (!mod) { mod = await import(`./screens/${tab.id}.js`); screenCache.set(tab.id, mod); }
    await mod.default(host, { client, endpoint: client._base });
  } catch (e) {
    host.innerHTML =
      `<div class="pad"><h2 class="s-title">${tab.label}</h2>` +
      `<p class="muted">This screen is being ported from the Flutter app ` +
      `(loka-studio/lib/screens/${tab.id}_screen.dart) — coming in a later slice.</p>` +
      `<p class="err">${(e && e.message) ? '' : ''}</p></div>`;
  }
}

window.addEventListener('hashchange', render);
if (!location.hash) location.hash = TABS[0].id;
render();
pingConn();
setInterval(pingConn, 15000);
