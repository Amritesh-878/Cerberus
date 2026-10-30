# Code signing with SignPath Foundation

SignPath Foundation signs open-source releases for free. The certificate is issued to SignPath
Foundation, so Windows shows "SignPath Foundation" as the publisher, and every signing request
needs a person to approve it in SignPath. Nothing here is set up yet: this file is what to do
once the project applies and is accepted.

Their conditions are at <https://signpath.org/terms>; read them again before applying, since
they change.

## Before applying

- **Release v0 unsigned first.** The project must already be released in the form it wants
  signed, so apply after the first installer is on GitHub Releases.
- **Licence:** MIT, an OSI licence, and nothing proprietary in the build. Cerberus qualifies.
- **Built from source in CI:** `.github/workflows/release.yml` builds the installer from a tag
  on GitHub-hosted runners, which is what SignPath's GitHub integration verifies.
- **Multi-factor authentication** on the GitHub account and, later, on the SignPath account.
- **A code signing policy on the project's home page.** Add this section to `readme.md` when
  applying (the wording in quotes is required as written):

  ```md
  ## Code signing policy

  Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by
  [SignPath Foundation](https://signpath.org/).

  - Committers and reviewers: [Amritesh-878](https://github.com/Amritesh-878)
  - Approvers: [Amritesh-878](https://github.com/Amritesh-878)

  Privacy policy: this program will not transfer any information to other networked systems
  unless specifically requested by the user or the person installing or operating it. Cerberus
  runs the Claude Code CLI, which talks to Anthropic under that tool's own terms.
  ```

  The Releases page needs a link to the same section, so add "Code signing policy" with a link
  to the readme to each release's notes.

## Applying

1. Open <https://signpath.org/apply> and fill in the form: project name, repository URL
   (`https://github.com/Amritesh-878/Cerberus`), the MIT licence, a short description, and the
   release download page.
2. Wait for approval. SignPath then creates an organization with a project for Cerberus.

## Setting up SignPath once approved

In SignPath:

1. Add the predefined trusted build system **GitHub.com** to the organization and link it to the
   Cerberus project.
2. Install the SignPath GitHub App on `Amritesh-878/Cerberus` (needed for the audit log).
3. Create an artifact configuration whose root is `<zip-file>` holding the one `pe-file`
   `Cerberus_*_x64-setup.exe`, since the upload step zips what it uploads.
4. In the signing policy, restrict file metadata: product name `Cerberus` and the product
   version the build sets. Tauri writes both into the installer from `tauri.conf.json`.
5. Create a CI user with submitter rights and copy its API token.

In GitHub (Settings, Secrets and variables, Actions):

- Secret `SIGNPATH_API_TOKEN`: the CI user's token.
- Variable `SIGNPATH_ORGANIZATION_ID`: the organization id from SignPath.

## The workflow change, left out until approved

Add these steps to `release.yml` between "Hash the installer" and "Draft the release", and move
the hashing after them so the hash is of the signed file. Keep the job's `permissions`
including `contents: write`, and add `actions: read`.

```yaml
- name: Upload the unsigned installer for signing
  id: unsigned
  if: github.event_name == 'push'
  uses: actions/upload-artifact@v7
  with:
    name: unsigned-installer
    path: src-tauri/target/release/bundle/nsis/*-setup.exe
    retention-days: 1

- name: Sign the installer
  if: github.event_name == 'push'
  uses: signpath/github-action-submit-signing-request@v3
  with:
    api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
    organization-id: ${{ vars.SIGNPATH_ORGANIZATION_ID }}
    project-slug: Cerberus
    signing-policy-slug: release-signing
    github-artifact-id: ${{ steps.unsigned.outputs.artifact-id }}
    wait-for-completion: true
    wait-for-completion-timeout-in-seconds: 3600
    output-artifact-directory: src-tauri/target/release/bundle/nsis
```

Use the project and policy slugs SignPath actually shows. Pin the action to a commit SHA once
its release is chosen.

How a signed release then runs:

1. The tag push builds the installer as today.
2. The unsigned installer goes up as an Actions artifact kept for one day, so it costs almost
   no Actions storage.
3. SignPath receives the request and holds it for approval. The job waits (up to an hour above)
   while the approver checks in SignPath that it came from the right tag, then approves it.
4. The signed installer overwrites the unsigned one, is hashed, and goes on the draft release.

## Limits to know

- Only the installer is signed. The `cerberus.exe` inside it and the uninstaller stay unsigned:
  signing those needs the app exe signed before Tauri packs it (`npx tauri build --no-bundle`,
  sign, then `npx tauri bundle`), which is a second request and a second approval per release.
  Add that only if Defender or SmartScreen flag the installed app.
- A signature from a new certificate still builds SmartScreen reputation over time, so the first
  downloads may still see a warning.
- Never re-sign or replace an asset already submitted to winget; publish a new version instead.
