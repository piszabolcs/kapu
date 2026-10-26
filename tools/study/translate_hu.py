"""K-12 (D150): the 60 Hungarian questions of bench/korpusz/q-hu.jsonl as an agent
writes them after reading `lang=en` on the map: in English, the same intent.
The translations were written by Claude (the agent of the measurement), one per
question, without looking at the expected answer file.

    tools/.venv/Scripts/python tools/study/translate_hu.py <q-hu.jsonl> <out.jsonl>
"""
import json
import sys

ENGLISH = [
    "how to find the real root cause of a bug instead of guessing at fixes",
    "write the test first, then the code? red-green-refactor cycle",
    "Excel spreadsheet with formulas, recalculating the cells afterwards",
    "extract text and tables from a PDF, merge pages with python",
    "create a Word document with javascript, what to watch out for",
    "how to name MCP server tools so they do not clash with other servers",
    "ten-question evaluation for an MCP server in XML format",
    "MCP server in typescript with the new SDK version",
    "why OKLCH color space is better than hex for design tokens",
    "accessibility legal minimum and Core Web Vitals budget for a website",
    "chat interface for AI, streaming response and stop button",
    "small footer signature button with version number and info popup at the bottom of the app",
    "rules for writing a portfolio README, headers and badges",
    "parallel working directory on a branch so I do not disturb the main work",
    "development done, all tests green, how to finish the branch: merge or PR",
    "writing a detailed step by step implementation plan before writing code",
    "brainstorming a new feature, ask about my goals before building",
    "trace back where a bad value comes from in the call chain",
    "how to write my own skill and test that it really works",
    "evaluating the quality of a CLAUDE.md file, what should go into it",
    "backend API design with fault tolerance, retries and idempotent requests",
    "explain step by step what you are doing because I want to learn from the project",
    "edit a Google Sheets spreadsheet, cell ranges and formulas in Drive",
    "create design tokens and variables for a Figma component library",
    "translating animations from design to code, which library to choose",
    "consolidate memory files, clean up duplicates and outdated facts",
    "make a card layout adapt to the width of its own container instead of the whole screen",
    "color the div around a field red when the input inside is invalid, without JavaScript",
    "can I write nested selectors in plain CSS without Sass or another preprocessor",
    "fix the order of reset, component and utility styles so selector specificity does not decide",
    "with nested light and dark blocks, make the nearest theme's link color always win regardless of source order",
    "position a tooltip relative to a button with CSS only, no JavaScript library",
    "why does the popover disappear without animation on close in Firefox while it fades nicely in Chrome",
    "animate elements on scroll in pure CSS, and what if the browser does not support it",
    "animate a list thumbnail into the large image of the detail view when I update the DOM",
    "compute a lighter or less saturated variant of an existing color in CSS by rewriting its channels",
    "set dark and light color in one declaration so it follows the system theme setting",
    "in a form, put every label in the same column when each field is wrapped in its own div",
    "which browser does not support the wrapping that prevents a single orphan word at the end of a paragraph",
    "textarea that grows by itself while typing, but at most twelve lines",
    "the scrollbar jumps when I skip rendering offscreen sections, what placeholder size should I give them",
    "write CSS that mirrors automatically on an Arabic or Hebrew page",
    "how to calculate the slope and base value so the font size grows evenly between 320 and 1280 pixels",
    "circular progress indicator that fills with animation to a given percentage",
    "open a floating panel on button click that closes on outside click or Esc, without JavaScript",
    "where does focus go after closing a modal dialog, and what if the opener button was removed from the DOM meanwhile",
    "animate the height of the disclosure panel to auto and replace the default triangle marker",
    "load the main hero image faster while the images below load only on scroll",
    "on a prerendered page, keep the visit counter from running before the page is actually opened",
    "send a web component's encapsulated shadow DOM as server-side HTML so it is styled without JavaScript",
    "let an old module use a different version of a dependency than the rest of the app, without a build step",
    "cancel a fetch request still running when the user has started a new one while typing",
    "infinite scroll list without listening to scroll events and measuring sizes",
    "show the red error border only after the user filled in and left the field, not on page load",
    "how a reusable submit button in React knows that the parent form is being submitted",
    "which React components run only on the server, and how to pass data to a button running in the browser",
    "where to define my own brand colors so they become bg- and text- classes when there is no config.js anymore",
    "the sticky header hides the focused element, which accessibility requirement does this break and how to fix it",
    "respect the user's reduced motion setting of the operating system in animations",
    "the page reacts slowly to clicks, how to break long JavaScript tasks into smaller pieces",
]


def main():
    src, out = sys.argv[1], sys.argv[2]
    lines = [json.loads(l) for l in open(src, encoding="utf-8") if l.strip()]
    assert len(lines) == len(ENGLISH), (len(lines), len(ENGLISH))
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        for q, en in zip(lines, ENGLISH):
            f.write(json.dumps({**q, "kerdes": en, "nyelv": "en"}, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
