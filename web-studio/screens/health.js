// Health screen — port of loka-studio/lib/screens/health_screen.dart.
// Reachability + DB stats (count, type distribution) + query performance
// (/health/queries) + HNSW vector index health, via
// LokaClient.{health,stats,queryMetrics,vectorsHealth}.

const esc = (s) => String(s).replace(/[&<>"]/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const short = (u) => { const m = /[#/]([^#/]+)\/?$/.exec(u); return m ? m[1] : u; };

const ms = (v) => (v == null ? '—' : v < 1 ? `${(v * 1000).toFixed(0)} µs` : `${v.toFixed(1)} ms`);
const num = (v, d = 1) => (v == null ? '—' : Number(v).toFixed(d));

// Query performance from GET /health/queries (planning/query-metrics.md):
// latency percentiles, per pattern shape, and how close the planner's row
// estimates are where they can be checked.
function renderQueryMetrics(qm) {
  if (!qm) {
    return '<h2 class="s-title">Query performance</h2>' +
      '<p class="muted">Not available from this endpoint (needs a <code>loka serve</code> with /health/queries).</p>';
  }
  if (!qm.queries) {
    return '<h2 class="s-title">Query performance</h2>' +
      '<p class="muted">No queries recorded yet.</p>';
  }
  const l = qm.latency_ms, e = qm.estimates;
  const shapes = qm.patterns.slice().sort((a, b) => (b.latency_ms.p90 ?? 0) - (a.latency_ms.p90 ?? 0));
  return '<h2 class="s-title">Query performance</h2>' +
    '<div class="cards">' +
    `<div class="card"><div class="k">Queries</div><div class="val">${qm.queries.toLocaleString()}</div></div>` +
    `<div class="card"><div class="k">Latency p50 / p90 / p99</div><div class="val">${ms(l.p50)} / ${ms(l.p90)} / ${ms(l.p99)}</div></div>` +
    `<div class="card"><div class="k">Planner estimates within 2×</div><div class="val">${e.within_2x == null ? '—' : Math.round(e.within_2x * 100) + '%'}</div></div>` +
    `<div class="card"><div class="k">Estimate q-error p50 / p90</div><div class="val">${num(e.q_error.p50)} / ${num(e.q_error.p90)}</div></div>` +
    '</div>' +
    '<table class="grid" style="margin-top:12px"><thead><tr>' +
    '<th>pattern shape</th><th>count</th><th>p50</th><th>p90</th><th>p99</th><th>rows p50</th>' +
    '</tr></thead><tbody>' +
    shapes.map(p =>
      `<tr><td><code>${esc(p.shape)}</code></td><td class="v-num">${p.count}</td>` +
      `<td class="v-num">${ms(p.latency_ms.p50)}</td><td class="v-num">${ms(p.latency_ms.p90)}</td>` +
      `<td class="v-num">${ms(p.latency_ms.p99)}</td><td class="v-num">${num(p.rows_p50, 0)}</td></tr>`).join('') +
    '</tbody></table>' +
    '<p class="muted" style="margin-top:6px">Shapes: <code>C</code> constant, <code>B</code> bound by an earlier pattern, <code>?</code> free. ' +
    `Estimates are scored only for patterns run with no variables bound (${e.scored} so far). Last 1024 samples per series.</p>`;
}

export default async function mount(host, ctx) {
  host.innerHTML = `<div class="pad">
    <h2 class="s-title">Health</h2>
    <div class="toolbar"><button class="run" id="hx-refresh">Refresh</button>
      <span class="count" id="hx-info"></span></div>
    <div id="hx-cards" class="cards"></div>
    <div id="hx-types" style="margin-top:18px"></div>
    <div id="hx-queries" style="margin-top:18px"></div>
    <div id="hx-vec" style="margin-top:18px"></div>
  </div>`;
  const cards = host.querySelector('#hx-cards');
  const typesEl = host.querySelector('#hx-types');
  const queriesEl = host.querySelector('#hx-queries');
  const vecEl = host.querySelector('#hx-vec');
  const info = host.querySelector('#hx-info');
  const refresh = host.querySelector('#hx-refresh');

  function card(k, v, color) {
    return `<div class="card"><div class="k">${esc(k)}</div>` +
      `<div class="val"${color ? ` style="color:${color}"` : ''}>${esc(v)}</div></div>`;
  }

  async function load() {
    refresh.disabled = true;
    info.textContent = 'checking…';
    cards.innerHTML = typesEl.innerHTML = queriesEl.innerHTML = vecEl.innerHTML = '';
    const up = await ctx.client.health();
    const stats = await ctx.client.stats();
    const vh = await ctx.client.vectorsHealth();
    const qm = await ctx.client.queryMetrics();
    info.textContent = ctx.endpoint;

    cards.innerHTML =
      card('Reachable', up ? 'online' : 'offline',
           up ? 'var(--green)' : 'var(--red)') +
      card('Triples', stats.totalTriples >= 0 ? stats.totalTriples.toLocaleString() : '—') +
      card('Distinct types', Object.keys(stats.types).length) +
      card('Vector predicates',
           Array.isArray(vh.predicates) ? vh.predicates.length :
           (vh.predicate_count ?? (vh.predicates ? Object.keys(vh.predicates).length : '—')));

    const types = Object.entries(stats.types).sort((a, b) => b[1] - a[1]);
    if (types.length) {
      typesEl.innerHTML =
        '<h2 class="s-title">Type distribution</h2><table class="grid"><thead><tr>' +
        '<th>type</th><th>count</th></tr></thead><tbody>' +
        types.map(([t, n]) =>
          `<tr><td class="v-uri" title="${esc(t)}">${esc(short(t))}</td>` +
          `<td class="v-num">${n}</td></tr>`).join('') +
        '</tbody></table>';
    }

    queriesEl.innerHTML = renderQueryMetrics(qm);

    const showDebug = localStorage.getItem('loka-studio-debug') === 'true';
    if (showDebug) {
      vecEl.innerHTML =
        '<h2 class="s-title">HNSW vector index (Debug)</h2>' +
        (vh && Object.keys(vh).length
          ? `<pre class="turtle">${esc(JSON.stringify(vh, null, 2))}</pre>`
          : '<p class="muted">No vector predicates declared (or /vectors/health unavailable).</p>');
    } else {
      vecEl.innerHTML = '<div style="margin-top:24px"><button id="hx-show-debug" class="muted-btn">Show engine internals (HNSW debug)</button></div>';
      const debugBtn = vecEl.querySelector('#hx-show-debug');
      if (debugBtn) {
        debugBtn.onclick = () => {
          localStorage.setItem('loka-studio-debug', 'true');
          load();
        };
      }
    }
    refresh.disabled = false;

  }
  refresh.onclick = load;
  load();
}
