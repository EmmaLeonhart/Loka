"""Selection provenance cites exactly the statements a proposal depended on.

Run: python -m pytest training/test_selection_provenance.py
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import infer_with_citations as iwc  # noqa: E402

EX = "http://ex.org/"


def uri(x: str) -> dict:
    return {"type": "uri", "value": EX + x}


def graph():
    """S has 12 statements; only `p_city -> Paris` is shared with neighbour N,
    which also has `p_mayor`. S lacks `p_mayor`, so `p_mayor` is proposed and
    its only evidence is S's `p_city -> Paris` (the 12th statement, past the
    old first-ten cut-off)."""
    subj_facts = {
        EX + "S": [(EX + f"p{i}", uri(f"o{i}")) for i in range(11)]
        + [(EX + "p_city", uri("Paris"))],
        EX + "N": [(EX + "p_city", uri("Paris")), (EX + "p_mayor", uri("Hidalgo"))],
    }
    pred_usage: dict[str, list] = {}
    for s, facts in subj_facts.items():
        for p, o in facts:
            pred_usage.setdefault(p, []).append((s, o))
    labels = {EX + k: k for k in ["S", "N", "p_city", "p_mayor", "Paris", "Hidalgo"]}
    labels.update({EX + f"p{i}": f"p{i}" for i in range(11)})
    labels.update({EX + f"o{i}": f"o{i}" for i in range(11)})
    return subj_facts, pred_usage, labels


def test_evidence_is_exactly_the_matching_statements():
    subj_facts, pred_usage, labels = graph()
    cand, evidence = iwc.candidate_predicates_with_evidence(
        EX + "S", labels=labels, subj_facts=subj_facts, pred_usage=pred_usage)
    assert cand == [EX + "p_mayor"]
    assert evidence[EX + "p_mayor"] == [(EX + "p_city", uri("Paris"))]


def test_candidate_predicates_unchanged():
    subj_facts, pred_usage, labels = graph()
    assert iwc.candidate_predicates(
        EX + "S", labels=labels, subj_facts=subj_facts, pred_usage=pred_usage
    ) == [EX + "p_mayor"]


def test_emitted_citations_are_the_evidence(monkeypatch):
    subj_facts, pred_usage, labels = graph()
    monkeypatch.setattr(iwc, "predict_object", lambda *a, **k: ("Anne Hidalgo", 0.9))
    lines, _ = iwc.generate_for_subject(
        None, EX + "S", labels=labels, subj_facts=subj_facts, pred_usage=pred_usage,
        vocab=None, inv_vocab=None, tokens_per_role=8, device="cpu",
        model_version="test",
    )
    cited = [ln for ln in lines if iwc.LOKA_INFERRED_FROM in ln]
    assert len(cited) == 1
    assert f"<< <{EX}S> <{EX}p_city> <{EX}Paris> >> ." in cited[0]
