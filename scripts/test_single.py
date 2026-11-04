#!/usr/bin/env python3
"""Debug OCR on a single image file."""

import sys
from pathlib import Path
from PIL import Image
from transformers import Qwen2VLForConditionalGeneration, AutoProcessor
import torch

MODEL_ID = "Qwen/Qwen2-VL-7B-Instruct"
IMAGE = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("/tmp/page-06_right.png")

PROMPT = "Transcribe every word of text visible in this image. Write only the words, line by line. Do not output coordinates or bounding boxes."

print(f"Image: {IMAGE}  exists={IMAGE.exists()}")
img = Image.open(IMAGE).convert("RGB")
print(f"Size: {img.size}")

device = "cuda" if torch.cuda.is_available() else "cpu"
print(f"Device: {device}")

from transformers import BitsAndBytesConfig
bnb_config = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_compute_dtype=torch.float16)

print("Loading model (7B 4-bit)...")
model = Qwen2VLForConditionalGeneration.from_pretrained(
    MODEL_ID,
    quantization_config=bnb_config,
    device_map="auto",
)
processor = AutoProcessor.from_pretrained(MODEL_ID)
print("Model loaded.")

messages = [{
    "role": "user",
    "content": [
        {"type": "image", "image": img},
        {"type": "text", "text": PROMPT},
    ],
}]

text_input = processor.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
inputs = processor(text=[text_input], images=[img], return_tensors="pt").to(device)

print(f"Input token count: {inputs['input_ids'].shape[1]}")

with torch.no_grad():
    out = model.generate(**inputs, max_new_tokens=1024, do_sample=False)

gen = out[:, inputs["input_ids"].shape[1]:]
raw = processor.batch_decode(gen, skip_special_tokens=True)[0]

print(f"\nOutput token count: {gen.shape[1]}")
print(f"Raw output length: {len(raw)} chars")
print("\n--- RAW OUTPUT ---")
print(repr(raw))
print("\n--- FORMATTED ---")
print(raw)
