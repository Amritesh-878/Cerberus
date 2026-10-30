# Packaging

How Cerberus is released and listed. `signing.md` covers code signing.

## Releasing a version

1. Set the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`,
   and merge that through a PR.
2. Tag the merged commit and push the tag: `git tag v0.1.0` then `git push origin v0.1.0`.
3. `.github/workflows/release.yml` runs the gate, builds the NSIS installer, and creates a
   **draft** release named after the tag with `Cerberus_<version>_x64-setup.exe` and its
   `.sha256` file. It fails if the tag is not `v` plus the version in `tauri.conf.json`.
4. Check the draft on GitHub, edit the notes, and publish it.

To test the workflow without releasing, run it by hand (Actions, release, Run workflow, or
`gh workflow run release.yml --ref <branch>`). A manual run builds and hashes the installer and
uploads nothing.

Never replace an asset on a published release, since winget and anyone who checked the hash
rely on it. Fix forward with a new version.

## winget

`winget/` holds the three manifest files for the first submission, with placeholders:

| Placeholder | Fill with |
|---|---|
| `<version>` | the version without the `v`, such as `0.1.0` |
| `<sha256>` | the hash from the release's `.sha256` file, upper or lower case |
| `<release-date>` | the publish date, `YYYY-MM-DD` |

The package id is `Amritesh-878.Cerberus`. `AppsAndFeaturesEntries` matches what the installer
writes under `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Cerberus`, so winget
recognises an installed copy.

### First submission

Submit only after the release is published, from a copy outside the repo:

1. Install the tool: `winget install Microsoft.WingetCreate`.
2. Copy `winget/` to `manifests/a/Amritesh-878/Cerberus/<version>/` in a scratch folder and fill
   in the placeholders.
3. Check it:
   - `winget validate <folder>` passes.
   - The URL downloads, and `Get-FileHash` of the download matches `<sha256>`.
   - `winget settings --enable LocalManifestFiles` (admin, once), then
     `winget install --manifest <folder>` installs it, it starts, and
     `winget uninstall Amritesh-878.Cerberus` removes it.
4. Submit: `wingetcreate submit --prtitle "New package: Amritesh-878.Cerberus version <version>" <folder>`.
   It asks for a GitHub sign-in (or takes `--token`) and opens a PR on `microsoft/winget-pkgs`.

`wingetcreate new <installer url>` is the interactive alternative: it downloads the installer,
asks for each field, and can submit at the end. Use the values in `winget/` as the answers.

The winget-pkgs bot then validates the manifest, downloads the installer, checks the hash, scans
it with Defender and installs it silently. A new unsigned app may wait for manual validation.

### A new version

Once the first version is in winget-pkgs, publish the new release and run:

```sh
wingetcreate update Amritesh-878.Cerberus --version <version> --urls "https://github.com/Amritesh-878/Cerberus/releases/download/v<version>/Cerberus_<version>_x64-setup.exe|x64|user" --submit
```

It downloads the installer, works out the new hash, and opens the PR. Quote the URL: `|` is a
pipe in every shell. Update `winget/` here if a field changes.
