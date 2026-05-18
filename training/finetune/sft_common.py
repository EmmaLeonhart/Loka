"""Shared SFT prompt formatting for the fine-tune track.

`finetune.py` (training) and `infer.py` (generation) MUST build the prompt
identically or the adapter sees one format and is queried with another. So
the format lives here once, not copied into both.

Prompt shape (planning/fine-tuning-track.md §"Training data format"):

    Context:
    - Tokyo | instance of | capital city
    - Tokyo | country | Japan

    Predict the object of: Tokyo | population | ?

The model is trained to complete with the answer string only. We feed it
through the base model's chat template (Qwen/Llama instruct), training the
completion tokens and masking the prompt tokens.
"""
from __future__ import annotations

_SLOT_WORD = {"object": "object", "subject": "subject", "predicate": "predicate"}


def build_prompt(context, target, slot: str) -> str:
    """Render one example's user-turn text. `context` is a list of
    [s, p, o] triples; `target` is the masked [s, p, o] (one slot "?")."""
    lines = ["Context:"]
    if context:
        for c in context:
            lines.append(f"- {c[0]} | {c[1]} | {c[2]}")
    else:
        lines.append("- (none)")
    lines.append("")
    lines.append(
        f"Predict the {_SLOT_WORD.get(slot, 'object')} of: "
        f"{target[0]} | {target[1]} | {target[2]}"
    )
    return "\n".join(lines)


def render_chat(tokenizer, prompt: str, answer: str | None):
    """Apply the base model's chat template.

    Returns (full_text, prompt_text). `prompt_text` is the prompt up to and
    including the assistant generation prefix (used to mask labels in
    training and as the generation input at inference). When `answer` is
    None, only the prompt prefix is returned (inference)."""
    msgs = [{"role": "user", "content": prompt}]
    prompt_text = tokenizer.apply_chat_template(
        msgs, tokenize=False, add_generation_prompt=True
    )
    if answer is None:
        return prompt_text, prompt_text
    full_text = prompt_text + answer + tokenizer.eos_token
    return full_text, prompt_text
