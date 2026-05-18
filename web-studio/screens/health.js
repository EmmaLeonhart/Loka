// Health screen — port of loka-studio/lib/screens/health_screen.dart.
// Reachability + DB stats (count, type distribution) + HNSW vector
// index health, via LokaClient.{health,stats,vectorsHealth}.

const esc = (s) => String(s).replace(/[&<>"]/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const short = (u) => { const m = /[#/]([^#/]+)\/?$/.exec(u); return m ? m[1] : u; };

export default async function mount(host, ctx) {
  host.innerHTML = `<div class="pad">
    <h2 class="s-title">Health</h2>
    <div class="toolbar"><button class="run" id="hx-refresh">Refresh</button>
      <span class="count" id="hx-info"></span></div>
    <div id="hx-cards" class="cards"></div>
    <div id="hx-types" style="margin-top:18px"></div>
    <div id="hx-vec" style="margin-top:18px"></div>
  </div>`;
  const cards = host.querySelector('#hx-cards');
  const typesEl = host.querySelector('#hx-types');
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
    cards.innerHTML = typesEl.innerHTML = vecEl.innerHTML = '';
    const up = await ctx.client.health();
    const stats = await ctx.client.stats();
    const vh = await ctx.client.vectorsHealth();
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
    vecEl.innerHTML =
      '<h2 class="s-title">HNSW vector index</h2>' +
      (vh && Object.keys(vh).length
        ? `<pre class="turtle">${esc(JSON.stringify(vh, null, 2))}</pre>`
        : '<p class="muted">No vector predicates declared (or /vectors/health unavailable).</p>');
    refresh.disabled = false;
  }
  refresh.onclick = load;
  load();
}
