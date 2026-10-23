# Third-party notices

`skills/` vendors skill files from four upstream repositories. Their licences require the
copyright and permission notice to travel with any copy, so they are reproduced here.

Each vendored skill also carries a `.source` file naming its upstream repo, path, pinned commit
and licence. `python scripts/pick-skills.py --check-upstream` re-reads those pins.

**When a skill is installed into a project, this file goes with it.** `pick-skills.py` copies the
skill directory and its `.source`, and writes a copy of this file to `.claude/skills/notice.md`
the first time a skill whose `origin` is not `first-party` is installed. A project that installs
only first-party skills never gets the copy, because it has nothing to carry a notice for.

---

## MIT

Applies to skills vendored from:

- `mattpocock/skills` — Copyright (c) 2026 Matt Pocock
- `emilkowalski/skills` — Copyright (c) 2026 Emil Kowalski
- `fallow-rs/fallow` — Copyright (c) 2026 Bart Waardenburg

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Modified MIT

Applies to skills vendored from `millionco/react-doctor` — Copyright (c) 2026 Million Software, Inc.
(`skills/tool-backed/react-doctor`, `improve-react`, `improve-threejs`, `performance`).

The MIT text above applies, with this addition, quoted verbatim:

```
Our only modification is that the following uses require prior written
permission from the copyright holder. To request permission, contact
founders@million.dev.
1. Using the Software, its source code, or any derivative works thereof, in
   whole or in part, as training, fine-tuning, or evaluation data, or as input
   to any automated pipeline for training or improving any machine learning
   model or AI system.
2. Selling the Software, or offering it to third parties as a paid, hosted, or
   managed product or service (including any commercial API or SaaS offering)
   whose value derives entirely or substantially from the Software.
```

**Clause 1 is unresolved and needs a human decision.** These are skill files, and a skill is
loaded into an LLM's context as instructions. Whether that counts as "input to any automated
pipeline for training or improving any machine learning model or AI system" is a judgement about
the licensor's intent, not something this repo can settle. Reading a document is not training on
it, and the clause names training, fine-tuning and evaluation data specifically. If that reading
is wrong, the four react-doctor skills are the ones to remove. Ask
[founders@million.dev](mailto:founders@million.dev) before relying on them commercially.
