#!/usr/bin/env python3
"""
OCR double-page spread PNGs and produce a single raw text file.

Each input PNG is a two-page spread (left page + right page side by side),
as produced by pdftoppm on a scanned book PDF. This script splits each image
in half, OCRs left and right halves separately with Qwen2-VL-7B-Instruct
(4-bit), and concatenates all results into one _raw_ocr.txt file.

Downloads model from HuggingFace on first run (~16 GB, cached after).

Usage (from wordglow root with venv active):
    source books-src/env/bin/activate
    python3 scripts/ocr_spreads.py
"""

import re
from pathlib import Path

from PIL import Image
from transformers import Qwen2VLForConditionalGeneration, AutoProcessor, BitsAndBytesConfig
import torch

PAGES_DIR = Path("/tmp/pages")
OUTPUT_DIR = Path("/home/user/git/wordglow/books-src/book")
MODEL_ID = "Qwen/Qwen2-VL-7B-Instruct"

#PROMPT = "Transcribe every word of text visible in this image. Write only the words, line by line. Do not output #coordinates or bounding boxes."
PROMPT = """Extract all story text from this book page. Rules:                                          
Include all narrative and dialogue text exactly as printed                                            
Include chapter numbers and titles (e.g. "1" then "Mr Sherlock Holmes")                               
Include section headings: "A Note about This Story", "The People in This Story"                       
Include character descriptions from "The People in This Story" section                                
SKIP: page numbers (lone digits at page bottom/top)                                                   
SKIP: illustration captions in italics                                                                
SKIP: publisher info, library stamps, level descriptions                                              
SKIP: "Points for Understanding" and anything after it                                                
If page is an illustration with little text, output only text labels if any                           
Preserve paragraph breaks with blank lines                                                            
Output extracted text only."""   

CHAPTER_SPLITS = {
    "ch02": 3500,
    "ch09": 3500,
    "ch10": 3500,
    "ch12": 3500,
}

STOP_AT = "Points for Understanding"


def load_model():
    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"Loading {MODEL_ID} on {device} (4-bit)...")
    bnb_config = BitsAndBytesConfig(
        load_in_4bit=True,
        bnb_4bit_compute_dtype=torch.float16,
    )
    model = Qwen2VLForConditionalGeneration.from_pretrained(
        MODEL_ID,
        quantization_config=bnb_config,
        device_map="auto",
    )
    processor = AutoProcessor.from_pretrained(MODEL_ID)
    return model, processor, device


def ocr_half(image: Image.Image, model, processor) -> str:
    messages = [
        {
            "role": "user",
            "content": [
                {"type": "image", "image": image},
                {"type": "text", "text": PROMPT},
            ],
        }
    ]
    text_input = processor.apply_chat_template(
        messages, tokenize=False, add_generation_prompt=True
    )
    inputs = processor(
        text=[text_input],
        images=[image],
        return_tensors="pt",
    ).to(model.device)

    with torch.no_grad():
        output_ids = model.generate(
            **inputs,
            max_new_tokens=1024,
            do_sample=False,
        )
    generated = output_ids[:, inputs["input_ids"].shape[1]:]
    raw = processor.batch_decode(generated, skip_special_tokens=True)[0].strip()
    return clean(raw)


def clean(text: str) -> str:
    # remove bounding box coordinates emitted by Qwen2-VL grounding mode
    text = re.sub(r"\(\d+,\d+\),\(\d+,\d+\)\s*", "", text)
    # remove lone page numbers on their own line
    text = re.sub(r"(?m)^\s*\d{1,3}\s*$", "", text)
    # collapse 3+ blank lines to 2
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


def ocr_page(image_path: Path, model, processor) -> str:
    img = Image.open(image_path).convert("RGB")
    w, h = img.size
    mid = w // 2
    left = img.crop((0, 0, mid, h))
    right = img.crop((mid, 0, w, h))
    left_text = ocr_half(left, model, processor)
    right_text = ocr_half(right, model, processor)
    parts = [t for t in (left_text, right_text) if t]
    return "\n\n".join(parts)


def collect_all_text(model, processor) -> str:
    pages = sorted(PAGES_DIR.glob("page-*.png"))
    pages = [p for p in pages if p.stem not in ("page-01", "page-02", "page-03", "page-30", "page-31", "page-32", "page-33", "page-34", "page-35", "page-36", "page-37")]

    parts = []
    for page in pages:
        print(f"  OCR {page.name}...")
        text = ocr_page(page, model, processor)
        if STOP_AT in text:
            # trim from stop marker
            text = text[: text.index(STOP_AT)].strip()
            parts.append(text)
            print("  reached end of story content, stopping")
            break
        parts.append(text)

    return "\n\n".join(parts)


# Map heading pattern → filename slug
CHAPTER_MAP = [
    (r"A Note about This Story", "note"),
    (r"The People in This Story", "people"),
    (r"(?m)^1\s*\n\s*Mr Sherlock Holmes", "ch01"),
    (r"(?m)^2\s*\n\s*The Curse of the Baskervilles", "ch02"),
    (r"(?m)^3\s*\n\s*The Problem", "ch03"),
    (r"(?m)^4\s*\n\s*Sir Henry Baskerville", "ch04"),
    (r"(?m)^5\s*\n\s*The Stolen Boot", "ch05"),
    (r"(?m)^6\s*\n\s*Baskerville Hall", "ch06"),
    (r"(?m)^7\s*\n\s*The Stapletons of Merripit House", "ch07"),
    (r"(?m)^8\s*\n\s*Dr Watson.s First Report", "ch08"),
    (r"(?m)^9\s*\n\s*The Light On The Moor", "ch09"),
    (r"(?m)^10\s*\n\s*The Man On The Moor", "ch10"),
    (r"(?m)^11\s*\n\s*High Tor Farm", "ch11"),
    (r"(?m)^12\s*\n\s*Setting the Trap", "ch12"),
    (r"(?m)^13\s*\n\s*The Hound of the Baskervilles", "ch13"),
    (r"(?m)^14\s*\n\s*Back in Baker Street", "ch14"),
]


def split_chapters(full_text: str) -> dict[str, str]:
    # find all chapter boundaries
    boundaries = []
    for pattern, slug in CHAPTER_MAP:
        m = re.search(pattern, full_text, re.IGNORECASE)
        if m:
            boundaries.append((m.start(), m.end(), slug))

    boundaries.sort()
    chapters = {}
    for i, (start, end, slug) in enumerate(boundaries):
        next_start = boundaries[i + 1][0] if i + 1 < len(boundaries) else len(full_text)
        body = full_text[end:next_start].strip()
        # remove stray leading chapter number
        body = re.sub(r"^\d+\s*\n", "", body).strip()
        chapters[slug] = body

    return chapters


def auto_split(chapters: dict[str, str]) -> dict[str, str]:
    result = {}
    for slug, text in chapters.items():
        limit = CHAPTER_SPLITS.get(slug)
        if limit and len(text) > limit:
            mid = len(text) // 2
            split = text.find("\n\n", mid)
            if split == -1:
                split = mid
            result[f"{slug}-1"] = text[:split].strip()
            result[f"{slug}-2"] = text[split:].strip()
        else:
            result[slug] = text
    return result


def main():
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    model, processor, device = load_model()
    print(f"Running on {device}")

    print("\nOCR all pages...")
    full_text = collect_all_text(model, processor)

    raw_path = OUTPUT_DIR / "_raw_ocr.txt"
    raw_path.write_text(full_text)
    print(f"\nRaw OCR → {raw_path}  ({len(full_text)} chars)")

    print("\nSplitting into chapters...")
    chapters = split_chapters(full_text)
    chapters = auto_split(chapters)

    for slug, text in sorted(chapters.items()):
        out = OUTPUT_DIR / f"{slug}.txt"
        out.write_text(text + "\n")
        print(f"  {out.name:20s} {len(text):5d} chars")

    print("\nDone.")


if __name__ == "__main__":
    main()
