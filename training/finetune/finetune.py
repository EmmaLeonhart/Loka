"""QLoRA fine-tune a base LLM on Loka's masked-triple SFT JSONL.

Status: STUB. The CLI shape is fixed but the trainer body is intentionally
empty so the implementation can be slotted in once the engine fix
(commit c36760b) is verified at scale and we have a real corpus ready.

Implementation plan (per planning/fine-tuning-track.md):

1. Load the JSONL from `--input` (one example per line, schema produced by
   prepare_jsonl.py).
2. Build the chat-template prompt:

       Context:
       - Tokyo | instance of | capital city
       - Tokyo | country | Japan

       Predict the {slot} of: Tokyo | population | ?

   Train completion: the answer string.
3. Load `--base-model` (default Qwen 2.5 1.5B-Instruct) in 4-bit via
   bitsandbytes, attach LoRA adapters via peft.LoraConfig.
4. Train with trl.SFTTrainer; save adapter + tokenizer to `--output`.

Run:

    python training/finetune/finetune.py \
        --input training/finetune/data/sft_v1.jsonl \
        --base-model Qwen/Qwen2.5-1.5B-Instruct \
        --output training/finetune/adapters/qwen2.5-1.5b-loka-v1 \
        --epochs 1 --batch-size 4
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--input", required=True, type=Path, help="SFT JSONL produced by prepare_jsonl.py")
    ap.add_argument("--base-model", default="Qwen/Qwen2.5-1.5B-Instruct")
    ap.add_argument("--output", required=True, type=Path, help="Adapter output directory")
    ap.add_argument("--epochs", type=int, default=1)
    ap.add_argument("--batch-size", type=int, default=4)
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--lora-r", type=int, default=16)
    ap.add_argument("--lora-alpha", type=int, default=32)
    ap.add_argument("--lora-dropout", type=float, default=0.05)
    ap.add_argument("--max-seq-length", type=int, default=2048)
    args = ap.parse_args()

    print(
        "training/finetune/finetune.py is a stub — see module docstring for the\n"
        "implementation plan. Args parsed successfully:\n"
        f"  input        = {args.input}\n"
        f"  base_model   = {args.base_model}\n"
        f"  output       = {args.output}\n"
        f"  epochs       = {args.epochs}\n"
        f"  batch_size   = {args.batch_size}\n"
        f"  lr           = {args.lr}\n"
        f"  lora_r       = {args.lora_r}\n"
        f"  lora_alpha   = {args.lora_alpha}\n"
        f"  lora_dropout = {args.lora_dropout}\n"
        f"  max_seq_len  = {args.max_seq_length}\n",
        file=sys.stderr,
    )
    sys.exit(2)  # not-yet-implemented


if __name__ == "__main__":
    main()
