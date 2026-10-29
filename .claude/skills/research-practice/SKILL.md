---
name: research-practice
description: Use when the work in a project is research on any topic, not only computer science — pinning down the question, gathering and weighing sources, keeping notes with citations, and maintaining a living summary — whether it is a single question or a long-running inquiry.
---

# Research practice

For research on any subject: history, a technical field, a market, a hobby, a
health question, a policy debate. It suits a single question and a long-running
inquiry that grows over months. (Replicating one specific paper is a different
job; that is `cleanvibe replicate`.)

## Layout (create it when the research starts, not before)
- `research/SUMMARY.md`: the living answer. What is known now, how sure, what
  is contested, what is still open. Rewrite it as understanding changes; it
  should always make sense read on its own.
- `research/sources.md`: one entry per source, with the citation or link, date
  accessed, what kind of source it is (primary data, peer-reviewed, reporting,
  opinion, vendor material), what it contributes, and how far to trust it.
- `research/notes/`: one Markdown file per sub-question, with every claim tied
  to a source.
- Downloads and datasets you fetch for the research go in
  `data_lake/downloads/` (committed, kept apart from the user's own material
  in the rest of `data_lake/`); throwaway fetches go in `scratch/`.

## How to work
- **Pin the question down.** If the user is here and replying, ask them
  (AskUserQuestion is fine then): what they want to know, why, how deep to go,
  and what would count as an answer. If they are not, infer the question from
  the chat, the material in `data_lake/` and the project name, write it at the
  top of `SUMMARY.md` as a stated assumption, and start.
- **Survey wide, then go deep.** First map the main positions and the key
  sources; then dig into what matters for the user's question.
- **Every claim gets a source.** Keep what a source says separate from your own
  inference, and label the inference.
- **Weigh sources rather than counting them.** Prefer primary and established
  sources, note dates and conflicts of interest, and when sources disagree,
  record both positions and what would settle it instead of averaging them.
- **Say how sure you are**, in `SUMMARY.md` and when you report back: lead with
  the answer, then the confidence, then the evidence.
- **Long-running inquiries** get a dated "What changed" entry in `SUMMARY.md`
  each session, so the user can see how the picture moved.
- **Link, don't copy.** Quote briefly; never commit whole copyrighted texts.
