"""Loka world-model inference sidecar.

A long-lived HTTP server that loads the pinned checkpoint ONCE and stays
resident, so an interactive gesture (double-clicking a node in the
`/browse` graph) can ask the world model to generate triples for that node
without paying the ~10 s checkpoint-load cost per click.

There is deliberately no on-demand inference inside the Rust engine — the
engine stays lean (CLAUDE.md core philosophy #4). This sidecar is the bridge.
It does NOT fork the inference logic: it imports the same
`load_model` / `build_inference_state` / `generate_for_subject` that
`training/infer_with_citations.py` uses.

Endpoints (CORS `*`, so the browser at :8091 / the `/browse` iframe on
:3030 can call it cross-origin):

    GET  /health             -> {"ok":true,"model":...,"device":...}
    POST /generate           -> body JSON:
        { "subject":  "<IRI>",                  (required)
          "endpoint": "http://localhost:3030",  (Loka to read facts from / write to)
          "confidence": 0.15,                   (optional, mean-token-prob floor)
          "max_candidates": 6,                  (optional)
          "post": true }                        (optional, write back to Loka)
      ->  { "subject", "generated":[{s,p,o,confidence}], "nt",
            "inserted", "errors", "labeled_subject" }

Run:  python tools/infer_server.py --port 8092 --device cpu
First start downloads the pinned checkpoint+vocab+tokenizer (~180 MB) from
the public `EmmaLeonhart/loka` HF dataset (no login needed).
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import requests
import torch

# stdout/stderr tolerant of Japanese demo labels on Windows cp1252 consoles.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "training"))

from infer_with_citations import (  # noqa: E402
    load_model,
    build_inference_state,
    generate_for_subject,
)
from preprocess import fetch_all_triples  # noqa: E402

PROPERTY_CACHE = str(REPO_ROOT / "training" / "property_label_cache.json")
PROV_CONFIDENCE = "http://loka.dev/provenance/propositionConfidence"

# One model, one lock. Forward passes are serialised; /health stays responsive
# because ThreadingHTTPServer answers it on another thread without the lock.
_STATE: dict = {}
_GEN_LOCK = threading.Lock()


# ── Label fallback ──────────────────────────────────────────────────────────
# The canonical label map keys off rdfs:label. The Shinto demo
# (playground_server) labels via http://example.org/name and has no
# rdfs:label, so without a fallback `generate_for_subject` would see zero
# labelled subjects and emit nothing. This enrichment is sidecar glue only —
# it does not change the canonical training/inference pipeline.
_NAME_PREDS = (
    "http://example.org/name",
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://schema.org/name",
)
_CAMEL = re.compile(r"(?<=[a-z0-9])(?=[A-Z])")


def _humanize_iri(iri: str) -> str:
    """`http://example.org/FushimiInari` -> `Fushimi Inari`; `foundedYear` ->
    `founded year`. Last best-effort label for an otherwise-unlabelled IRI."""
    local = re.split(r"[/#]", iri.rstrip("/#"))[-1] or iri
    local = local.replace("_", " ").replace("-", " ")
    local = _CAMEL.sub(" ", local)
    return " ".join(local.split())


def _enrich_labels(triples, labels):
    """Give every subject/predicate IRI in the corpus a label if it lacks one:
    prefer a name-literal already in the triples, else humanize the IRI."""
    name_lit: dict[str, str] = {}
    for t in triples:
        if t["s"]["type"] != "uri":
            continue
        if t["p"]["value"] in _NAME_PREDS and t["o"]["type"] == "literal":
            name_lit.setdefault(t["s"]["value"], t["o"]["value"])
    n_added = 0
    seen_iris = set()
    for t in triples:
        for term, is_pred in ((t["s"], False), (t["p"], True), (t["o"], False)):
            if term["type"] != "uri":
                continue
            iri = term["value"]
            if iri in seen_iris:
                continue
            seen_iris.add(iri)
            if iri in labels:
                continue
            labels[iri] = name_lit.get(iri) or _humanize_iri(iri)
            n_added += 1
    return n_added


# ── N-Triples-star parsing (for the browser payload) ────────────────────────
_BASE_RE = re.compile(r'^<([^>]+)>\s+<([^>]+)>\s+"((?:[^"\\]|\\.)*)"\s*\.\s*$')
_CONF_RE = re.compile(
    r'^<<\s*<([^>]+)>\s+<([^>]+)>\s+"((?:[^"\\]|\\.)*)"\s*>>\s+'
    r'<' + re.escape(PROV_CONFIDENCE) + r'>\s+"([0-9.]+)"'
)


def _unescape(s: str) -> str:
    return (
        s.replace('\\"', '"').replace("\\n", "\n").replace("\\r", "\r")
        .replace("\\t", "\t").replace("\\\\", "\\")
    )


def _parse_generated(nt_lines: list[str]) -> list[dict]:
    """Turn the emitter's N-Triples-star back into [{s,p,o,confidence}] for the
    graph, pairing each base fact with its propositionConfidence annotation."""
    conf: dict[tuple, float] = {}
    for ln in nt_lines:
        m = _CONF_RE.match(ln.strip())
        if m:
            conf[(m.group(1), m.group(2), _unescape(m.group(3)))] = float(m.group(4))
    out = []
    for ln in nt_lines:
        ln = ln.strip()
        if ln.startswith("<<"):
            continue
        m = _BASE_RE.match(ln)
        if not m:
            continue
        s, p, o = m.group(1), m.group(2), _unescape(m.group(3))
        out.append({"s": s, "p": p, "o": o, "confidence": conf.get((s, p, o))})
    return out


# ── HTTP handler ────────────────────────────────────────────────────────────
class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):  # quieter, single-line
        sys.stderr.write("  [infer] " + (fmt % args) + "\n")

    def _send(self, code: int, payload: dict):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        if self.path.split("?")[0] != "/health":
            self._send(404, {"error": "not found"})
            return
        self._send(200, {
            "ok": True,
            "model": _STATE.get("model_version"),
            "device": _STATE.get("device"),
        })

    def do_POST(self):
        if self.path.split("?")[0] != "/generate":
            self._send(404, {"error": "not found"})
            return
        try:
            n = int(self.headers.get("Content-Length", 0))
            req = json.loads(self.rfile.read(n) or b"{}")
        except Exception as e:
            self._send(400, {"error": f"bad request body: {e}"})
            return

        subject = (req.get("subject") or "").strip()
        if not subject:
            self._send(400, {"error": "missing 'subject'"})
            return
        endpoint = (req.get("endpoint") or "http://localhost:3030").rstrip("/")
        # Defaults are deliberately permissive for the interactive gesture:
        # show the model's actual best guess (flagged low-confidence in the
        # UI) rather than silently discarding it. The batch pipeline keeps
        # its own stricter defaults — these only apply to /generate calls.
        confidence = float(req.get("confidence", 0.0))
        token_floor = float(req.get("min_token_prob", 0.01))
        max_candidates = int(req.get("max_candidates", 6))
        do_post = bool(req.get("post", True))

        try:
            with _GEN_LOCK:
                result = self._generate(
                    subject, endpoint, confidence, token_floor,
                    max_candidates, do_post,
                )
            self._send(200, result)
        except requests.RequestException as e:
            self._send(502, {"error": f"could not reach Loka at {endpoint}: {e}"})
        except Exception as e:  # noqa: BLE001 - surface to the browser
            self._send(500, {"error": f"{type(e).__name__}: {e}"})

    def _generate(self, subject, endpoint, confidence, token_floor,
                  max_candidates, do_post):
        # Pivot path: base Qwen + Emma's BFS+embedding retrieval against
        # the vectorised Loka the double-click points at. No fine-tune.
        # (confidence/token_floor/max_candidates kept for API compat —
        # the base model doesn't use the from-scratch decoder knobs.)
        from retrieval_generate import generate as rg_generate
        triples, nt = rg_generate(endpoint, subject, hops=1, budget=28)
        generated = [{"s": t["s"], "p": t["p"], "o": t["o"],
                      "confidence": t.get("confidence")} for t in triples]

        reason, detail = None, None
        if not generated:
            reason = "no_generation"
            detail = (f"The base model produced no parseable new triples for "
                      f"{subject} from the retrieved context. Try a node with "
                      f"a richer neighbourhood, or one that exists in the "
                      f"vectorised graph this endpoint serves.")

        inserted, errors = 0, []
        if do_post and nt.strip():
            r = requests.post(
                f"{endpoint.rstrip('/')}/triples",
                data=(nt + "\n").encode("utf-8"),
                headers={"Content-Type": "text/plain; charset=utf-8"},
                timeout=120,
            )
            if r.status_code == 200:
                j = r.json()
                inserted, errors = j.get("inserted", 0), j.get("errors", [])
            else:
                errors = [f"POST /triples -> {r.status_code}: {r.text[:200]}"]

        return {
            "subject": subject,
            "labeled_subject": subject,
            "model": _STATE.get("model_version", "qwen2.5-1.5b-base"),
            "subject_fact_count": len(generated),
            "generated": generated,
            "reason": reason,
            "detail": detail,
            "nt": nt,
            "inserted": inserted,
            "errors": errors,
        }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, default=8092)
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument(
        "--device", default="cpu",
        help="cpu (default — safe, the model is tiny) or cuda",
    )
    ap.add_argument("--checkpoint", default=None, help="default: MODEL.json pin")
    ap.add_argument("--vocab", default=None, help="default: MODEL.json pin")
    ap.add_argument(
        "--bpe-tokenizer", default=None,
        help="default: the tokenizer pinned alongside the checkpoint",
    )
    args = ap.parse_args()

    # PIVOT: base Qwen + Emma's BFS+embedding retrieval, NO fine-tune.
    # The masked-SFT adapter lobotomised the model (tools/_ft_probe3.py;
    # planning/base-retrieval.md). Pre-warm base Qwen + the MiniLM
    # retrieval encoder at startup so a double-click is ~retrieval(~14s)
    # + generation, not + model-load.
    sys.path.insert(0, str(REPO_ROOT / "tools"))
    print("Pre-warming base Qwen2.5-1.5B + MiniLM retrieval encoder "
          "(one-time)…", file=sys.stderr)
    import retrieval_generate as _rg
    from graph_retrieval import _encoder as _ge
    _rg._load()   # base Qwen resident
    _ge()         # MiniLM resident
    _STATE["model_version"] = "qwen2.5-1.5b-base"
    _STATE["device"] = "cpu"

    httpd = ThreadingHTTPServer((args.host, args.port), Handler)
    print(
        f"\n  Loka inference sidecar  ·  model {_STATE['model_version']}  ·  base+retrieval (no fine-tune)\n"
        f"  http://{args.host}:{args.port}/health\n"
        f"  POST http://{args.host}:{args.port}/generate  {{subject, endpoint}}\n"
        f"\n  Double-click a node in the :8091 Knowledge Graph tab to use it.\n"
        f"  Ctrl+C to stop.\n",
        file=sys.stderr,
    )
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nstopping sidecar", file=sys.stderr)
        httpd.server_close()


if __name__ == "__main__":
    main()
