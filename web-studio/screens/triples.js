// Triples screen — port of loka-studio/lib/screens/triples_screen.dart.
// Paged SELECT ?s ?p ?o (LIMIT/OFFSET) via LokaClient.fetchTriples.

const esc = (s) => String(s).replace(/[&<>"]/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const short = (u) => { const m = /[#/]([^#/]+)\/?$/.exec(u); return m ? m[1] : u; };
const cell = (b) => {
  if (!b) return '<td class="v-blank">—</td>';
  const v = b.value ?? '';
  if (b.type === 'uri') return `<td class="v-uri" title="${esc(v)}">${esc(short(v))}</td>`;
  if (b.type === 'bnode') return `<td class="v-blank">_:${esc(v)}</td>`;
  const num = b.datatype && /(integer|decimal|double|float|long|int)/i.test(b.datatype);
  return `<td class="${num ? 'v-num' : 'v-lit'}" title="${esc(v)}">${esc(v)}</td>`;
};

export default async function mount(host, ctx) {
  let limit = 100, offset = 0;
  host.innerHTML = `
    <div class="pad" style="display:flex;flex-direction:column;min-height:0;flex:1">
      <h2 class="s-title">Triples</h2>
      <div class="toolbar">
        <button class="run" id="tx-prev" style="background:var(--bg-card);color:var(--text)">◂ Prev</button>
        <button class="run" id="tx-next" style="background:var(--bg-card);color:var(--text)">Next ▸</button>
        <select id="tx-lim" class="editor" style="width:auto;padding:7px 10px;font-size:12px">
          <option>50</option><option selected>100</option><option>250</option><option>500</option>
        </select>
        <span class="count" id="tx-info"></span>
      </div>
      <div id="tx-out" style="flex:1;min-height:0;overflow:auto"></div>
    </div>`;
  const out = host.querySelector('#tx-out');
  const info = host.querySelector('#tx-info');
  const prev = host.querySelector('#tx-prev');
  const next = host.querySelector('#tx-next');
  const lim = host.querySelector('#tx-lim');

  async function load() {
    info.textContent = 'loading…';
    prev.disabled = next.disabled = true;
    try {
      const rows = await ctx.client.fetchTriples(limit, offset);
      info.textContent =
        `rows ${offset + 1}–${offset + rows.length}` +
        (rows.length < limit ? ' (end)' : '');
      out.innerHTML = rows.length
        ? '<table class="grid"><thead><tr><th>?s</th><th>?p</th><th>?o</th></tr></thead><tbody>' +
          rows.map(r => `<tr>${cell(r.s)}${cell(r.p)}${cell(r.o)}</tr>`).join('') +
          '</tbody></table>'
        : '<p class="muted">No triples.</p>';
      next.dataset.end = rows.length < limit ? '1' : '';
    } catch (e) {
      out.innerHTML = `<div class="err">${esc(e.message || String(e))}</div>`;
    } finally {
      prev.disabled = offset === 0;
      next.disabled = next.dataset.end === '1';
    }
  }
  prev.onclick = () => { offset = Math.max(0, offset - limit); load(); };
  next.onclick = () => { offset += limit; load(); };
  lim.onchange = () => { limit = parseInt(lim.value, 10); offset = 0; load(); };
  load();
}
