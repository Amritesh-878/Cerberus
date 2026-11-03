# Third-party notices

Code in this folder is copied unchanged from its upstream project and built into the app. Each
copy keeps its own licence file beside it.

## zsh-autosuggestions

| | |
|---|---|
| Upstream | [zsh-users/zsh-autosuggestions](https://github.com/zsh-users/zsh-autosuggestions) |
| Version | v0.7.1, commit `e52ee8ca55bcc56a17c828767a3f98f22a68d4eb` |
| Files | `zsh-autosuggestions/zsh-autosuggestions.zsh` (sha256 `eec7ba8f7a71414ace0ea0fab0908d005b24cf65d83b169c0ff97815d3cfc51a`), `zsh-autosuggestions/LICENSE` |
| Licence | MIT, Copyright (c) 2013 Thiago de Arruda, Copyright (c) 2016-2021 Eric Freese |
| Used for | Suggestions in zsh tabs. The app writes the file into its data folder and a zsh tab sources it after your own `.zshrc`. The file's header carries the copyright and permission notice, so every copy the app writes keeps it. |

To update it, replace both files with those of a newer release tag, then change the version,
commit and checksum here and the version the Rust test `the_bundled_plugin_is_the_pinned_release_with_its_licence` checks.

```
Copyright (c) 2013 Thiago de Arruda
Copyright (c) 2016-2021 Eric Freese

Permission is hereby granted, free of charge, to any person
obtaining a copy of this software and associated documentation
files (the "Software"), to deal in the Software without
restriction, including without limitation the rights to use,
copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.
```
