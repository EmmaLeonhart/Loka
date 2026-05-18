// SPARQL screen — port of loka-studio/lib/screens/sparql_screen.dart,
// reusing the result-rendering approach from pages/playground.html,
// driven by the shared JS LokaClient (ctx.client.query).

const EXAMPLES = {
  'All triples (50)':
    'SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 50',
  'Types':
    'SELECT ?type (COUNT(?s) AS ?n) WHERE {\n  ?s <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ?type\n} GROUP BY ?type',
  'Labels':
    'SELECT ?s ?label WHERE {\n  ?s <http://www.w3.org/2000/01/rdf-schema#label> ?label\n} LIMIT 50',
  'RDF-star annotations':
    'SELECT ?s ?p ?o ?ap ?av WHERE {\n  << ?s ?p ?o >> ?ap ?av\n} LIMIT 50',
};

const fmtCell = (b) => {
  if (!b) return '<td class="v-blank">—</td>';
  const v = b.value ?? '';
  if (b.type === 'uri')
    return `<td class="v-uri" title="${esc(v)}">${esc(short(v))}</td>`;
  if (b.type === 'bnode')
    return `<td class="v-blank">_:${esc(v)}</td>`;
  const num = b.datatype && /(integer|decimal|double|float|long|int)/i.test(b.datatype);
  return `<td class="${num ? 'v-num' : 'v-lit'}" title="${esc(v)}">${esc(v)}</td>`;
};
const short = (u) => {
  const m = /[#/]([^#/]+)\/?$/.exec(u);
  return m ? m[1] : u;
};
const esc = (s) => String(s).replace(/[&<>"]/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

export default async function mount(host, ctx) {
  host.innerHTML = `
    <div class="pad">
      <h2 class="s-title">SPARQL</h2>
      <div class="toolbar">
        <select id="sx-ex" class="editor" style="width:auto;padding:7px 10px;font-size:12px">
          ${Object.keys(EXAMPLES).map(k => `<option>${esc(k)}</option>`).join('')}
        </select>
        <button class="run" id="sx-run">Run &nbsp;▸</button>
        <span class="count" id="sx-info"></span>
      </div>
      <textarea id="sx-q" rows="7" spellcheck="false"></textarea>
      <div id="sx-out" style="margin-top:14px;overflow:auto"></div>
    </div>`;

  const q = host.querySelector('#sx-q');
  const out = host.querySelector('#sx-out');
  const info = host.querySelector('#sx-info');
  const sel = host.querySelector('#sx-ex');
  const run = host.querySelector('#sx-run');

  const setExample = () => { q.value = EXAMPLES[sel.value]; };
  setExample();
  sel.addEventListener('change', setExample);

  async function execute() {
    run.disabled = true;
    info.textContent = 'running…';
    out.innerHTML = '';
    const t0 = performance.now();
    try {
      const { variables, rows } = await ctx.client.query(q.value);
      const ms = Math.round(performance.now() - t0);
      info.textContent = `${rows.length} row${rows.length === 1 ? '' : 's'} · ${ms} ms`;
      if (!rows.length) { out.innerHTML = '<p class="muted">No results.</p>'; return; }
      out.innerHTML =
        '<table class="grid"><thead><tr>' +
        variables.map(v => `<th>?${esc(v)}</th>`).join('') +
        '</tr></thead><tbody>' +
        rows.map(r => '<tr>' + variables.map(v => fmtCell(r[v])).join('') + '</tr>').join('') +
        '</tbody></table>';
    } catch (e) {
      info.textContent = 'error';
      out.innerHTML = `<div class="err">${esc(e.message || String(e))}</div>`;
    } finally {
      run.disabled = false;
    }
  }

  run.addEventListener('click', execute);
  q.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); execute(); }
  });
  execute();
}
