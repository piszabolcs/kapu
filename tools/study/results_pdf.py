"""Print the results page to PDF, in English and in Hungarian, with a headless
Edge or Chrome (the charts need the page's own d3, loaded from cdnjs).

    python tools/study/results_pdf.py

Writes docs/results/kapu-measurements-en.pdf and -hu.pdf: A4 landscape, the
page's print styles (`@media print` in results_template.html).
"""

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PAGE = ROOT / "docs" / "results" / "index.html"
CANDIDATES = [
    Path(os.environ.get("ProgramFiles(x86)", "")) / "Microsoft/Edge/Application/msedge.exe",
    Path(os.environ.get("ProgramFiles", "")) / "Google/Chrome/Application/chrome.exe",
]


def browser():
    for path in CANDIDATES:
        if path.is_file():
            return str(path)
    for name in ("msedge", "google-chrome", "chromium", "chrome"):
        if shutil.which(name):
            return shutil.which(name)
    sys.exit("no Edge or Chrome found")


def main():
    exe = browser()
    with tempfile.TemporaryDirectory() as profile:
        for lang in ("en", "hu"):
            out = PAGE.parent / f"kapu-measurements-{lang}.pdf"
            subprocess.run([exe, "--headless=new", "--disable-gpu", "--no-pdf-header-footer",
                            f"--user-data-dir={profile}", "--window-size=1100,800",
                            "--virtual-time-budget=20000", f"--print-to-pdf={out}",
                            f"{PAGE.as_uri()}?lang={lang}"], check=True)
            print("written", out, out.stat().st_size, "bytes")


if __name__ == "__main__":
    main()
