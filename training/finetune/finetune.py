"""QLoRA fine-tune a base LLM on Loka's masked-triple SFT JSONL.

Implements planning/fine-tuning-track.md. Deviations (permitted — the doc
says "subject to revision"), recorded there and in queue.md:

- No `trl` (not installed; transformers 5.x compat risk on Windows). This
  is a direct `transformers` + `peft` SFT loop.
- 8 GB-laptop QLoRA: bnb 4-bit nf4, micro-batch 1 + grad accumulation,
  capped seq length, gradient checkpointing, paged-adamw-8bit, bf16.
- Resilience over completeness: the adapter is saved to `<output>/epochN/`
  AND pushed to Hugging Face after EVERY epoch, a per-epoch loss line is
  appended to `<output>/train_log.jsonl`, and a restart resumes from the
  latest on-disk epoch. A thermal cutoff (CLAUDE.md hardware reality)
  costs one epoch, not the run.

Run:

    python training/finetune/finetune.py \
        --input  training/finetune/data/sft_v1.jsonl \
        --output training/finetune/adapters/qwen2.5-1.5b-loka-v1 \
        --hf-repo EmmaLeonhart/loka-qwen2.5-1.5b \
        --epochs 8 --batch-size 8
"""
from __future__ import annotations

import argparse
import json
import math
import os
import random
import sys
import time
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

sys.path.insert(0, str(Path(__file__).resolve().parent))
from sft_common import build_prompt, render_chat  # noqa: E402

# 8 GB-VRAM ceiling: 2048 (the doc's default) OOMs a 1.5B 4-bit on this card.
SEQ_CAP = 768


def load_examples(path: Path, limit: int):
    rows = []
    with path.open(encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))
            if limit and len(rows) >= limit:
                break
    return rows


def tokenize_example(tok, ex, max_len):
    """Full text = chat(prompt)+answer+eos. Labels mask the prompt span so
    loss is only on the answer (standard SFT)."""
    prompt = build_prompt(ex["context"], ex["target"], ex.get("slot", "object"))
    full_text, prompt_text = render_chat(tok, prompt, str(ex["answer"]))
    full = tok(full_text, truncation=True, max_length=max_len,
               add_special_tokens=False)
    plen = len(tok(prompt_text, add_special_tokens=False)["input_ids"])
    ids = full["input_ids"]
    labels = list(ids)
    for i in range(min(plen, len(labels))):
        labels[i] = -100
    if all(x == -100 for x in labels):  # answer got truncated away
        return None
    return {"input_ids": ids, "labels": labels}


def latest_epoch(out: Path) -> int:
    n = 0
    if out.exists():
        for d in out.glob("epoch*"):
            try:
                n = max(n, int(d.name[5:]))
            except ValueError:
                pass
    return n


def push_epoch(repo, out_dir: Path, epoch: int, no_push: bool):
    """Best-effort per-epoch HF push. A failure logs and returns — training
    must not die because the network blipped (the local checkpoint stands)."""
    if no_push or not repo:
        return
    try:
        from huggingface_hub import HfApi
        api = HfApi()
        api.create_repo(repo, repo_type="model", exist_ok=True, private=False)
        api.upload_folder(
            repo_id=repo, repo_type="model",
            folder_path=str(out_dir), path_in_repo=f"epoch{epoch}",
            commit_message=f"epoch {epoch} adapter",
        )
        try:
            api.create_tag(repo, repo_type="model", tag=f"epoch{epoch}",
                           tag_message=f"epoch {epoch}")
        except Exception as e:  # tag may already exist on resume
            print(f"  [hf] tag epoch{epoch}: {e}", file=sys.stderr)
        print(f"  [hf] pushed epoch{epoch} -> {repo}", file=sys.stderr)
    except Exception as e:  # noqa: BLE001
        print(f"  [hf] push FAILED (kept local): {type(e).__name__}: {e}",
              file=sys.stderr)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--input", required=True, type=Path, help="SFT JSONL produced by prepare_jsonl.py")
    ap.add_argument("--base-model", default="Qwen/Qwen2.5-1.5B-Instruct")
    ap.add_argument("--output", required=True, type=Path, help="Adapter output directory")
    ap.add_argument("--epochs", type=int, default=1)
    ap.add_argument("--batch-size", type=int, default=4, help="Effective batch (micro-batch is 1; this many grad-accum steps)")
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--lora-r", type=int, default=16)
    ap.add_argument("--lora-alpha", type=int, default=32)
    ap.add_argument("--lora-dropout", type=float, default=0.05)
    ap.add_argument("--max-seq-length", type=int, default=2048, help=f"Clamped to {SEQ_CAP} on 8 GB VRAM")
    ap.add_argument("--limit", type=int, default=0, help="Cap examples (0 = all in the JSONL)")
    ap.add_argument("--hf-repo", default="EmmaLeonhart/loka-qwen2.5-1.5b", help="Per-epoch push target (model repo)")
    ap.add_argument("--no-push", action="store_true", help="Disable per-epoch HF push (local checkpoints only)")
    ap.add_argument("--log-every", type=int, default=25)
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    import torch
    from transformers import (
        AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig,
    )
    from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
    import bitsandbytes as bnb

    random.seed(args.seed)
    torch.manual_seed(args.seed)
    max_len = min(args.max_seq_length, SEQ_CAP)
    args.output.mkdir(parents=True, exist_ok=True)

    print(f"loading {args.input} ...", file=sys.stderr)
    examples = load_examples(args.input, args.limit)
    print(f"  {len(examples):,} SFT examples", file=sys.stderr)

    tok = AutoTokenizer.from_pretrained(args.base_model)
    if tok.pad_token is None:
        tok.pad_token = tok.eos_token

    print(f"loading {args.base_model} in 4-bit nf4 ...", file=sys.stderr)
    bnb_cfg = BitsAndBytesConfig(
        load_in_4bit=True, bnb_4bit_quant_type="nf4",
        bnb_4bit_compute_dtype=torch.bfloat16,
        bnb_4bit_use_double_quant=True,
    )
    model = AutoModelForCausalLM.from_pretrained(
        args.base_model, quantization_config=bnb_cfg,
        device_map={"": 0}, dtype=torch.bfloat16,
    )
    model = prepare_model_for_kbit_training(
        model, use_gradient_checkpointing=True
    )
    lora = LoraConfig(
        r=args.lora_r, lora_alpha=args.lora_alpha,
        lora_dropout=args.lora_dropout, bias="none",
        task_type="CAUSAL_LM",
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj",
                        "gate_proj", "up_proj", "down_proj"],
    )
    model = get_peft_model(model, lora)
    model.config.use_cache = False
    model.print_trainable_parameters()

    start_epoch = latest_epoch(args.output)
    if start_epoch:
        from peft import PeftModel  # noqa: F401
        adapter = args.output / f"epoch{start_epoch}"
        print(f"resuming: loading adapter weights from {adapter}", file=sys.stderr)
        from safetensors.torch import load_file
        sd_path = adapter / "adapter_model.safetensors"
        if sd_path.exists():
            model.load_state_dict(load_file(str(sd_path)), strict=False)
        print(f"  resume -> starting at epoch {start_epoch + 1}", file=sys.stderr)

    device = next(model.parameters()).device
    trainable = [p for p in model.parameters() if p.requires_grad]
    optim = bnb.optim.PagedAdamW8bit(trainable, lr=args.lr)

    log_path = args.output / "train_log.jsonl"
    free, total = torch.cuda.mem_get_info()
    print(f"VRAM free/total {free/1e9:.2f}/{total/1e9:.2f} GB | seq<= {max_len} "
          f"| eff.batch {args.batch_size} | {len(examples):,} ex/epoch",
          file=sys.stderr)

    for epoch in range(start_epoch + 1, args.epochs + 1):
        model.train()
        random.shuffle(examples)
        t0 = time.time()
        run_loss, seen, micro = 0.0, 0, 0
        optim.zero_grad(set_to_none=True)
        for ex in examples:
            tk = tokenize_example(tok, ex, max_len)
            if tk is None:
                continue
            ids = torch.tensor([tk["input_ids"]], device=device)
            lab = torch.tensor([tk["labels"]], device=device)
            out = model(input_ids=ids, labels=lab)
            loss = out.loss / args.batch_size
            loss.backward()
            run_loss += out.loss.item()
            seen += 1
            micro += 1
            if micro == args.batch_size:
                torch.nn.utils.clip_grad_norm_(trainable, 1.0)
                optim.step()
                optim.zero_grad(set_to_none=True)
                micro = 0
            if seen % args.log_every == 0:
                rate = seen / (time.time() - t0)
                print(f"  ep{epoch} {seen}/{len(examples)} "
                      f"loss {run_loss/seen:.4f} ppl {math.exp(min(run_loss/seen,20)):.1f} "
                      f"{rate:.2f} ex/s", file=sys.stderr, flush=True)
        if micro:  # flush a partial accumulation tail
            torch.nn.utils.clip_grad_norm_(trainable, 1.0)
            optim.step()
            optim.zero_grad(set_to_none=True)

        avg = run_loss / max(seen, 1)
        dt = time.time() - t0
        ep_dir = args.output / f"epoch{epoch}"
        ep_dir.mkdir(parents=True, exist_ok=True)
        model.save_pretrained(str(ep_dir))
        tok.save_pretrained(str(ep_dir))
        rec = {"epoch": epoch, "avg_loss": round(avg, 5),
               "ppl": round(math.exp(min(avg, 20)), 2),
               "examples": seen, "seconds": round(dt, 1),
               "base_model": args.base_model}
        with log_path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(rec) + "\n")
        print(f"=== epoch {epoch} done: loss {avg:.4f} ppl {rec['ppl']} "
              f"({dt/60:.1f} min) -> {ep_dir}", file=sys.stderr, flush=True)
        push_epoch(args.hf_repo, ep_dir, epoch, args.no_push)

    print("training complete.", file=sys.stderr)


if __name__ == "__main__":
    main()
