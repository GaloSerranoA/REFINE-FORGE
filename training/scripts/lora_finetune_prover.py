#!/usr/bin/env python3
"""Reference QLoRA fine-tune for Stage-1 expert iteration of a downloaded prover.

THIS IS AN OPERATOR-RUN REFERENCE SCRIPT — not part of the Rust build, not tested
in CI, and deliberately minimal. It exists so the expert-iteration loop is
concrete. Run it on YOUR GPU after the Rust `mine_proofs` step has produced a
verified-only SFT dataset (`sft-chat.jsonl`). Review + adapt before any real use.

What it does: 4-bit (QLoRA) load a base prover, LoRA-adapt it on the mined
chat-format dataset, and save the adapter (optionally merged). Every training
example is a Lean-VERIFIED proof — the trust property is enforced upstream by the
miner, not here.

Requirements (install yourself):
    pip install "transformers>=4.44" "peft>=0.12" "trl>=0.9" "datasets>=2.20" \
                "bitsandbytes>=0.43" accelerate

Hardware notes for a P40 + 5080 box:
  * P40 (Pascal, sm_61): NO bf16 (Ampere+ only). Use --dtype fp16. 4-bit NF4 via
    bitsandbytes works on Pascal but is slow; a 7B QLoRA still fits its 24 GB.
  * RTX 5080 (Blackwell): bf16 is fine and faster — use --dtype bf16.
  * Pick the card with CUDA_VISIBLE_DEVICES (e.g. the P40 for training while the
    5080 keeps serving inference, or vice-versa).

Example:
    CUDA_VISIBLE_DEVICES=1 python training/scripts/lora_finetune_prover.py \
        --base deepseek-ai/DeepSeek-Prover-V2-7B \
        --data sft-chat.jsonl --out adapters/prover-round-1 \
        --dtype fp16 --epochs 1 --batch 1 --grad-accum 16

Then re-serve the adapted model (vLLM supports LoRA adapters directly):
    vllm serve deepseek-ai/DeepSeek-Prover-V2-7B \
        --enable-lora --lora-modules round1=adapters/prover-round-1 --port 8000
…and point the next `refine-train run` at model `round1`, bump --round, repeat.
"""
import argparse
import sys


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--base", required=True, help="base prover model id or path")
    ap.add_argument("--data", required=True, help="mined chat JSONL ({'messages':[...]})")
    ap.add_argument("--out", required=True, help="output adapter dir")
    ap.add_argument("--dtype", choices=["fp16", "bf16"], default="fp16",
                    help="fp16 for Pascal/P40, bf16 for Ampere+/5080")
    ap.add_argument("--epochs", type=float, default=1.0)
    ap.add_argument("--batch", type=int, default=1)
    ap.add_argument("--grad-accum", type=int, default=16)
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--lora-r", type=int, default=16)
    ap.add_argument("--lora-alpha", type=int, default=32)
    ap.add_argument("--max-seq-len", type=int, default=2048)
    ap.add_argument("--merge", action="store_true", help="also save a merged fp16 model")
    args = ap.parse_args()

    try:
        import torch
        from datasets import load_dataset
        from peft import LoraConfig
        from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig
        from trl import SFTTrainer, SFTConfig
    except ImportError as e:
        print(f"missing dependency: {e}\n  pip install transformers peft trl datasets "
              f"bitsandbytes accelerate", file=sys.stderr)
        return 2

    compute_dtype = torch.float16 if args.dtype == "fp16" else torch.bfloat16

    quant = BitsAndBytesConfig(
        load_in_4bit=True,
        bnb_4bit_quant_type="nf4",
        bnb_4bit_compute_dtype=compute_dtype,
        bnb_4bit_use_double_quant=True,
    )
    tok = AutoTokenizer.from_pretrained(args.base)
    if tok.pad_token is None:
        tok.pad_token = tok.eos_token
    model = AutoModelForCausalLM.from_pretrained(
        args.base, quantization_config=quant, device_map="auto", torch_dtype=compute_dtype,
    )

    lora = LoraConfig(
        r=args.lora_r, lora_alpha=args.lora_alpha, lora_dropout=0.05, bias="none",
        task_type="CAUSAL_LM",
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj",
                        "gate_proj", "up_proj", "down_proj"],
    )

    # The miner emits chat-format rows; let the chat template render them to text.
    ds = load_dataset("json", data_files=args.data, split="train")

    def to_text(row):
        return {"text": tok.apply_chat_template(row["messages"], tokenize=False)}

    ds = ds.map(to_text, remove_columns=ds.column_names)

    cfg = SFTConfig(
        output_dir=args.out,
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch,
        gradient_accumulation_steps=args.grad_accum,
        learning_rate=args.lr,
        fp16=(args.dtype == "fp16"),
        bf16=(args.dtype == "bf16"),
        logging_steps=5,
        save_strategy="epoch",
        max_seq_length=args.max_seq_len,
        dataset_text_field="text",
        gradient_checkpointing=True,
    )
    trainer = SFTTrainer(model=model, args=cfg, train_dataset=ds, peft_config=lora)
    trainer.train()
    trainer.save_model(args.out)
    print(f"saved LoRA adapter to {args.out}")

    if args.merge:
        merged = trainer.model.merge_and_unload()
        merged.save_pretrained(f"{args.out}-merged")
        tok.save_pretrained(f"{args.out}-merged")
        print(f"saved merged model to {args.out}-merged")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
