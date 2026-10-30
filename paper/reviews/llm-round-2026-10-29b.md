# LLM review round 2, 2026-10-29 (evening)

Emma asked for every LLM reviewer to be run again on the changed paper. Paper: the `/arxiv/`
build of `c49db95` (18 pages, `pages/arxiv/loka-retractable-provenance.pdf`; title "Loka:
Provenance for Retracting Model-Generated Triples in an RDF-star Store", cites v0.4.7), with
the same prompt as round 1 (`llm-round-2026-10-29.md`):

> You are a critical peer reviewer for a systems/databases venue. Review the attached paper,
> which is about to be submitted to arXiv (cs.DB). Give: (1) a one-paragraph summary; (2) a
> recommendation (Strong Accept / Accept / Weak Accept / Weak Reject / Reject); (3) the main
> weaknesses, each with the section it concerns and a concrete fix; (4) any claims that are
> overstated, unsupported, or inconsistent with the paper's own tables; (5) minor issues
> (clarity, terminology, citations). Be specific and cite section numbers.

| Reviewer | Verdict | Link |
|---|---|---|
| Meta AI | **Weak Accept** for arXiv ("Weak Reject for a top-tier venue in current form") | meta.ai/prompt/37905ade-5f45-4dbe-83ba-c4b1faeda481 |
| DeepSeek | none: Google sign-in hits DeepSeek's slider CAPTCHA, with Proton VPN on and again after Emma turned it off (agents may not solve CAPTCHAs) | — |
| Gemini (Flash) | **Weak Accept** ("leaning Accept for arXiv") | gemini.google.com/app/3610c94b3ec46721 |
| ChatGPT | **Weak Accept** (round 1: Weak Reject) | chatgpt.com/c/6ae3f785-eaf8-83e8-b966-f699a761a2e1 |
| Claude (Fable 5.1) | **Weak Reject** for a main-track venue, Accept at a workshop; "nothing here blocks an arXiv posting" (round 1: Weak Accept) | claude.ai/chat/e0133834-b0cc-4531-a332-275c5d48fa4a |
| Grok | **Weak Accept** ("no claim is contradicted by the tables") | grok.com/c/151b7336-b287-4366-850a-10dea830a49f |
| Mistral (Vibe) | **Weak Accept** ("with W1–W3 addressed I would move to Accept"; checked the arithmetic and found it clean) | chat.mistral.ai/work/e1c93fc9-4824-49d3-a7be-3bfedaa6b801 |
| Qwen (3.7-Plus, guest) | **Weak Accept**. Run after Emma gave her birth year for the age check; guests cannot upload, so it got the Markdown source of `c49db95` as text | chat.qwen.ai (guest chat, no saved link) |
| Microsoft Copilot | none: Emma tried to sign in; Microsoft refused to create the account ("account creation has been blocked") | — |
| Perplexity (free plan, after Emma signed in) | **Weak Reject** for a full systems paper, "not an objection to posting a preprint". Run on the reframed 17-page build (`c60a23b`); its first answer was a plain summary, so the prompt was sent as a follow-up | perplexity.ai/search/044546cf-6ced-46ae-8718-deb4bb365c66 |
| DeepSeek via HuggingChat | none: HuggingChat requires a Hugging Face login in this Chrome | — |

Seven reviews: **six Weak Accept** (Meta AI, Gemini, ChatGPT, Grok, Mistral, Qwen) and **one Weak
Reject for a main-track venue** (Claude, which adds "nothing here blocks an arXiv posting").
Round 1 was four Weak Accept and one Weak Reject (ChatGPT); ChatGPT moved up, Claude moved down.
Texts were read with the extension's page-text tool; links above.

Qwen's points: node-rooted retraction is the main practical limit (W1); a stronger generator
such as RotatE (W2); profile where retraction time goes (W3); "the preview removed a median of
83 triples" should say "identified"; `prov:` prefix clashes with PROV-O; the §3.1 guard paragraph
should be a list mapping each invariant to its guards; the RDF-star CG report needs a year;
frame the 6–10× as a limit of this executor, not of SPARQL-star.

Perplexity's points (on the reframed build): (W1) "cites everything it used" overstates. The
record cites one neighbour statement carrying the proposed predicate, not necessarily the
matching statement, and predicate ranking, the top-N cutoff, the negative "subject lacks this
predicate" test and label lookups are not recorded. Wants a precise completeness condition
scoped to whole-node deletion. (W2) The correctness tests check traversal, not dependency
capture: a missing citation would be absent from both the store and the reference. Wants a
capture oracle on small graphs. (W3) Annotations attach to triple content, not to an
assertion. A triple that is both curated and generated is unresolved. (W4) Novelty versus the
engineering integration.

## Checked and not real

- "Green et al. 2007 missing from References" (Meta AI): it is there.
- "§6.5 table typo `9 / 10.3 19 / / 36`" (Gemini) and "`[SEP_0]`, role `0`" (Mistral): PDF text
  extraction; the source reads `9 / 10.3 / 19 / 36`, `[SEP_O]`, `O`.
- "§5 missing" (Meta AI): extraction.

## Checked and real

- **§6.2 2M-store row ledger** (Claude): 2,000,623 + 672 + 13,318 − 5,377 ≠ 2,009,885. The log
  (`training/logs/retract_commit_hf2m.json`) has `rows_before` 2,015,262, and 2,015,262 − 5,377 =
  2,009,885, so the number is right but the paper never gives the starting count (649 rows of
  the starting store are not accounted for by the three numbers it does give).

## Points raised by more than one reviewer

- **Exclusion is a pipeline property, not a store invariant** (Meta W1, ChatGPT W3, Mistral W2,
  Claude W8): the abstract states it unconditionally; the training side has one guard, not three.
- **Selection provenance vs what the model learned** (ChatGPT W1, Grok, Gemini): retraction does
  not remove a deleted statement's influence on trained weights; §2.4/§2.6 wording overreaches.
- **Abstract timing** (Meta, Gemini, Grok, Mistral, ChatGPT): "about 0.1 s" is the 85 ms
  in-memory preview, not a commit; the two §6.2 tables give different `retract_set` times.
- **8,621 is (entity, generated triple) pairs**, not triples (ChatGPT, Mistral).
- **Store-size claim rests on two points** (all six): wants a store-size × removal-size grid.
- **No other RDF-star store** (Gemini, Claude, Grok, ChatGPT): run the SPARQL closure on Jena or
  Oxigraph; "any SPARQL-star store can compute it" is untested.
- **§6.3**: reification baseline uses `prov:wasDerivedFrom` while §2.2 recommends `prov:used`
  (ChatGPT, Claude); named graphs counted, not queried (Meta, Mistral, ChatGPT).
- **`prov:` prefix clashes with PROV-O** (Claude, Qwen): use `loka:` or similar.
- **Hardware not stated** (ChatGPT, Claude, Mistral).
- **Node-rooted, single-justification semantics** (all): state formally; measure over-deletion.
- **Model-debugging history is long for cs.DB** (Claude, Mistral).
