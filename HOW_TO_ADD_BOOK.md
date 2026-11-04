# How to Add a Scanned PDF Book to Wordglow

## Prerequisites

- `pdftoppm` installed (`sudo apt install poppler-utils`)
- Python venv with GPU packages: `source books-src/env/bin/activate`
  - `torch`, `transformers`, `bitsandbytes`, `accelerate`, `Pillow`
  - GPU with ≥8GB VRAM (Qwen2-VL-7B runs 4-bit, ~6GB)
- `teacher-cli` built (`cargo build --bin teacher-cli`)

---

## Step 1 — Convert PDF to spread PNGs

Scanned book PDFs typically have one image per PDF page, where each image is a
**two-page spread** — left page and right page side by side in one image.
`pdftoppm` extracts these as PPM files; convert to PNG for the OCR step.

```bash
# Extract spreads as PPM at 150 dpi
mkdir -p /tmp/book_pages
pdftoppm -r 150 "MyBook.pdf" /tmp/book_pages/page

# Convert PPM → PNG (scripts/ppm2png.py uses pure stdlib — no system Pillow needed)
python3 scripts/ppm2png.py
# Resulting files: /tmp/book_pages/page-01.png, page-02.png, ...
```

150 dpi gives ~1500×1100 px per spread — enough for clean OCR. Lower dpi produces
blurry results; higher dpi is slower with no quality gain.

---

## Step 2 — OCR spread PNGs → single `_raw_ocr.txt`

`ocr_spreads.py` loads each spread PNG, splits it down the middle into left and
right half-pages, OCRs each half separately, and concatenates everything into one
raw text file. This avoids the model reading both pages interleaved.

Edit `ocr_spreads.py`:

| Variable | What to set |
|---|---|
| `PAGES_DIR` | `/tmp/book_pages` |
| `OUTPUT_DIR` | `books-src/<bookslug>/` |
| Pages to skip in `collect_all_text()` | Cover, TOC, end-matter appendix pages |
| `STOP_AT` | First string that marks end of story content (e.g. first word of an exercises section) |

```bash
source books-src/env/bin/activate
python3 scripts/ocr_spreads.py
# Output: books-src/<bookslug>/_raw_ocr.txt
```

Model: `Qwen/Qwen2-VL-7B-Instruct` (4-bit). Downloads ~16 GB on first run, cached after.  
Runtime: ~30–60 s per page on an 8 GB GPU.

---

## Step 3 — Split raw OCR into chapter files

Edit `split_raw.py`:

| Variable | What to set |
|---|---|
| `INPUT` | `books-src/<bookslug>/_raw_ocr.txt` |
| `OUTPUT_DIR` | `books-src/<bookslug>/` |
| `CHAPTER_MAP` | One entry per chapter: `(pattern, slug)` |
| `CHAPTER_SPLITS` | Slugs whose char count exceeds ~3500 — will be split to Part 1/2 |

**Finding chapter patterns — step by step:**

**1. List all headings from the raw file**

```bash
# Print every line that looks like a chapter title:
# short line, not starting with a quote or lowercase
grep -n "^[A-Z]" books-src/<bookslug>/_raw_ocr.txt | awk -F: 'length($2) < 60'
```

This gives candidate lines with line numbers. Cross-reference with the book's table of contents.

**2. Check context around each candidate**

```bash
grep -n -A2 -B2 "Chapter Title" books-src/<bookslug>/_raw_ocr.txt
```

A real chapter heading appears on its **own line** surrounded by blank lines.  
If the title also appears inside body sentences (e.g. the book title repeated mid-story), you must anchor it.

**3. Write the pattern**

| Situation | Pattern |
|---|---|
| Title appears only as a heading | `r"Chapter Title"` |
| Title also appears in body text | `r"(?m)^Chapter Title$"` — `^` and `$` match whole line only |
| Apostrophe (OCR may use `'` or `'`) | `r"(?m)^The Doctor.s Report$"` — `.` matches any char |
| OCR produced `\section*{Title}` | `r"\\section\*\{Title\}"` — escape backslash and braces |
| No heading at all in the OCR | Add to `inject_missing_headings()`: replace a **unique first sentence** of that chapter with `"Heading\n\n<first sentence>"` |

**4. Verify no false matches**

`re.search()` finds the **first** occurrence. If a later chapter heading also appears in earlier body text, the split lands in the wrong place. Test with:

```python
import re
text = open("books-src/<bookslug>/_raw_ocr.txt").read()
for pattern, slug in CHAPTER_MAP:
    m = re.search(pattern, text, re.IGNORECASE)
    if m:
        print(f"{slug:10s}  line ~{text[:m.start()].count(chr(10))+1:4d}  {repr(text[m.start():m.start()+40])}")
    else:
        print(f"{slug:10s}  NO MATCH")
```

Expected: line numbers should increase monotonically (each chapter starts after the previous one).

**5. Set `CHAPTER_SPLITS` for long chapters**

After a first run, check the output sizes:

```bash
wc -c books-src/<bookslug>/*.txt | sort -n
```

Any chapter over ~3500 chars is long for a language-learning lesson. Add its slug to `CHAPTER_SPLITS` and re-run — `split_raw.py` will split it at the nearest paragraph break past the midpoint.

Common OCR quirks to watch for:
- Heading printed as `\section*{Title}` → LaTeX artifact from the OCR model; use escaped pattern and it's auto-stripped from the body
- Apostrophes may be straight `'` or curly `'` → use `.` wildcard
- Chapter with no heading → `inject_missing_headings()` in `split_raw.py`

```bash
python3 scripts/split_raw.py
# Output: books-src/<bookslug>/ch01.txt, ch02-1.txt, ch02-2.txt, ...
```

---

## Step 4 — Add book and lessons to DB

```bash
# Create book
cargo run --bin teacher-cli -- add-book \
  --title "Book Title" \
  --author "Author Name" \
  --language "English"

# Add lessons in order (ch01 → ch02-1 → ch02-2 → ...)
cargo run --bin teacher-cli -- add-lesson \
  --book "Book Title" \
  --title "Chapter 1: Title" \
  --text-path books-src/<bookslug>/ch01.txt

# ... repeat for each chapter file
```

Verify order:
```bash
cargo run --bin teacher-cli -- list-lessons --book "Book Title"
```

---

## Step 5 — Generate TTS audio

```bash
# Get lesson IDs from list-lessons, then:
cargo run --bin teacher-cli -- generate-tts-free \
  --lesson-id <UUID>

# To regenerate (after CLI fixes):
cargo run --bin teacher-cli -- generate-tts-free \
  --lesson-id <UUID> --force
```

Default voice: `en-US-SteffanNeural`. Pass `--voice-name` to change.

Batch all at once:
```bash
for id in <id1> <id2> ...; do
  cargo run --bin teacher-cli -- generate-tts-free --lesson-id "$id"
done
```

---

## Troubleshooting

| Problem | Fix |
|---|---|
| OCR outputs `(10,10),(992,992)` coordinates | Model too small — must use 7B, not 2B |
| Chapter body starts mid-sentence | Pattern matched heading inside body text — add `^` and `$` anchors |
| Paragraph split across blank line | `fix_split_paragraphs()` in `split_raw.py` handles this automatically (merges paragraphs starting with lowercase) |
| `audio already up to date` when you want fresh | Add `--force` |
