#!/usr/bin/env python3
"""Test OCR on pages 3-6 only."""

import re
from pathlib import Path
from PIL import Image
from transformers import Qwen2VLForConditionalGeneration, AutoProcessor, BitsAndBytesConfig
import torch

PAGES_DIR = Path("/tmp/pages")
MODEL_ID = "Qwen/Qwen2-VL-7B-Instruct"

PROMPT = "Transcribe every word of text visible in this image. Write only the words, line by line. Do not output coordinates or bounding boxes."


def load_model():
    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"Loading model on {device} (4-bit)...")
    bnb_config = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.float16)
    model = Qwen2VLForConditionalGeneration.from_pretrained(
        MODEL_ID, quantization_config=bnb_config, device_map="auto"
    )
    processor = AutoProcessor.from_pretrained(MODEL_ID)
    return model, processor


def ocr_half(image, model, processor) -> str:
    messages = [{
        "role": "user",
        "content": [
            {"type": "image", "image": image},
            {"type": "text", "text": PROMPT},
        ],
    }]
    text_input = processor.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
    inputs = processor(text=[text_input], images=[image], return_tensors="pt").to(model.device)
    with torch.no_grad():
        out = model.generate(**inputs, max_new_tokens=800, do_sample=False)
    gen = out[:, inputs["input_ids"].shape[1]:]
    raw = processor.batch_decode(gen, skip_special_tokens=True)[0].strip()
    # strip coordinates
    raw = re.sub(r"\(\d+,\d+\),\(\d+,\d+\)\s*", "", raw)
    raw = re.sub(r"(?m)^\s*\d{1,3}\s*$", "", raw)
    return raw.strip()


def main():
    model, processor = load_model()

    for page_num in [4,5,6]:
        path = PAGES_DIR / f"page-{page_num:02d}.png"
        print(f"parse: {path}")
        if not path.exists():
            print(f"MISSING: {path}")
            continue

        img = Image.open(path).convert("RGB")
        w, h = img.size
        mid = w // 2

        left = img.crop((0, 0, mid, h))
        right = img.crop((mid, 0, w, h))

        #left_path = PAGES_DIR / f"page-{page_num:02d}_left.png"
        #right_path = PAGES_DIR / f"page-{page_num:02d}_right.png"
        #left.save(left_path)
        #right.save(right_path)
        #print(f"  saved {left_path.name}, {right_path.name}")

        print(f"\n{'='*60}")
        print(f"PAGE {page_num} LEFT")
        print('='*60)
        left_text = ocr_half(left, model, processor)
        print(left_text)

        print(f"\n{'='*60}")
        print(f"PAGE {page_num} RIGHT")
        print('='*60)
        right_text = ocr_half(right, model, processor)
        print(right_text)


if __name__ == "__main__":
    main()
