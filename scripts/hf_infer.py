from __future__ import annotations

import json
import os
import sys
import threading


def _to_prompt(messages: list[dict[str, object]]) -> str:
    lines: list[str] = []
    for m in messages:
        role = str(m.get("role", "")).upper()
        content = str(m.get("content", ""))
        lines.append(f"{role}: {content}")
    lines.append("ASSISTANT:")
    return "\n".join(lines)

def _emit_sse_delta(delta: str) -> None:
    if not delta:
        return
    payload = json.dumps({"delta": delta}, ensure_ascii=False)
    sys.stdout.write(f"event: delta\ndata: {payload}\n\n")
    sys.stdout.flush()


def main() -> int:
    try:
        req = json.load(sys.stdin)
    except Exception as exc:  # noqa: BLE001
        print(f"invalid JSON input: {exc}", file=sys.stderr)
        return 2

    model = str(req.get("model", "")).strip()
    if not model:
        print("missing model", file=sys.stderr)
        return 2

    messages = req.get("messages") or []
    if not isinstance(messages, list):
        print("messages must be a list", file=sys.stderr)
        return 2

    temperature = float(req.get("temperature", 0.4))
    max_new_tokens = int(req.get("max_new_tokens", 256))
    device = str(req.get("device", "auto")).strip().lower()
    local_only = bool(req.get("local_only", False)) or os.getenv("SPIRAL_CODER_HF_LOCAL_ONLY", "0") == "1"
    stream = bool(req.get("stream", False))

    try:
        import torch  # type: ignore
        from transformers import (  # type: ignore
            AutoModelForCausalLM,
            AutoTokenizer,
            TextIteratorStreamer,
            pipeline,
        )
    except Exception as exc:  # noqa: BLE001
        print(f"transformers import failed: {exc}", file=sys.stderr)
        return 3

    prompt = _to_prompt(messages)

    use_cuda = device == "cuda" or (device == "auto" and torch.cuda.is_available())
    torch_device = torch.device("cuda" if use_cuda else "cpu")

    if stream:
        tokenizer = AutoTokenizer.from_pretrained(model, local_files_only=local_only)
        hf_model = AutoModelForCausalLM.from_pretrained(model, local_files_only=local_only)
        hf_model.to(torch_device)

        inputs = tokenizer(prompt, return_tensors="pt")
        inputs = {k: v.to(torch_device) for k, v in inputs.items()}

        streamer = TextIteratorStreamer(
            tokenizer,
            skip_prompt=True,
            skip_special_tokens=True,
        )

        gen_kwargs: dict[str, object] = dict(
            **inputs,
            max_new_tokens=max_new_tokens,
            do_sample=True,
            temperature=temperature,
            streamer=streamer,
        )
        pad_id = getattr(tokenizer, "eos_token_id", None)
        if pad_id is not None:
            gen_kwargs["pad_token_id"] = pad_id

        t = threading.Thread(target=hf_model.generate, kwargs=gen_kwargs, daemon=True)
        t.start()

        try:
            for chunk in streamer:
                _emit_sse_delta(str(chunk))
        finally:
            t.join(timeout=1)
        return 0

    device_arg = 0 if use_cuda else -1
    gen = pipeline(
        "text-generation",
        model=model,
        device=device_arg,
        local_files_only=local_only,
    )
    out = gen(
        prompt,
        max_new_tokens=max_new_tokens,
        temperature=temperature,
        do_sample=True,
        return_full_text=True,
    )
    text = out[0].get("generated_text", "") if out else ""
    answer = text[len(prompt) :].strip() if isinstance(text, str) and text.startswith(prompt) else str(text).strip()

    json.dump({"content": answer, "model": model}, sys.stdout, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
