// Ontology screen — port of loka-studio/lib/screens/ontology_screen.dart.
// The full graph exported as Turtle / N-Triples via
// LokaClient.exportGraph, with a download button (the Protégé path).

const esc = (s) => String(s).replace(/[&<>"]/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

export default async function mount(host, ctx) {
  host.innerHTML = `<div class="pad" style="display:flex;flex-direction:column;min-height:0;flex:1">
    <h2 class="s-title">Ontology / Graph export</h2>
    <div class="toolbar">
      <select id="ox-fmt" class="editor" style="width:auto;padding:7px 10px;font-size:12px">
        <option value="turtle">Turtle</option>
        <option value="ntriples">N-Triples</option>
      </select>
      <button class="run" id="ox-load">Load</button>
      <button class="run" id="ox-dl" style="background:var(--bg-card);color:var(--text)">Download</button>
      <span class="count" id="ox-info"></span>
    </div>
    <div id="ox-out" style="flex:1;min-height:0;overflow:auto"></div>
  </div>`;
  const out = host.querySelector('#ox-out');
  const info = host.querySelector('#ox-info');
  const fmt = host.querySelector('#ox-fmt');
  let last = '';

  async function load() {
    info.textContent = 'exporting…';
    out.innerHTML = '';
    try {
      last = await ctx.client.exportGraph(fmt.value);
      const lines = last.split('\n').length;
      info.textContent = `${last.length.toLocaleString()} bytes · ~${lines} lines`;
      out.innerHTML = `<pre class="turtle">${esc(last)}</pre>`;
    } catch (e) {
      info.textContent = 'error';
      out.innerHTML = `<div class="err">${esc(e.message || String(e))}</div>`;
    }
  }
  host.querySelector('#ox-load').onclick = load;
  host.querySelector('#ox-dl').onclick = () => {
    if (!last) return;
    const ext = fmt.value === 'ntriples' ? 'nt' : 'ttl';
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([last], { type: 'text/plain' }));
    a.download = `loka-graph.${ext}`;
    a.click();
    URL.revokeObjectURL(a.href);
  };
  load();
}
