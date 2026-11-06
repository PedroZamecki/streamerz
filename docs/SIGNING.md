# Windows signing preparation

Streamerz's Windows release is currently unsigned. SignPath Foundation offers free code signing to
qualifying open-source projects, subject to its review and conditions. This document is an onboarding
plan, not a claim that Streamerz has been accepted or that its releases are signed.

## Apply

1. Read the [SignPath Foundation conditions](https://signpath.org/terms.html), then use its
   [application page](https://signpath.org/apply). The project is
   [PedroZamecki/streamerz](https://github.com/PedroZamecki/streamerz), uses the MIT license, and
   publishes a Windows EXE through GitHub Actions and GitHub Releases.
2. Check the conditions against the complete Windows build, including bundled dependencies, system
   changes, privacy and uninstall instructions. SignPath decides eligibility. Assign actual authors,
   reviewers and signing approvers and enable MFA for the people who need access.
3. Prepare a truthful privacy policy and code signing policy. SignPath requires specific attribution,
   role membership and privacy information on the project home page and download pages. Publish the
   signing attribution only when it accurately describes the service in use. Review the app's network
   requests and third-party services before making privacy claims.
4. Complete any account, identity and application steps SignPath requests. Do not put tokens, passwords
   or user data in the repository, issues or chat.

## Configure after acceptance

Follow the [GitHub trusted build system documentation](https://docs.signpath.io/trusted-build-systems/github)
with the actual SignPath project and signing policy created during onboarding:

- Link GitHub.com as the trusted build system and configure the approved repository and workflow. Install
  the SignPath GitHub App if required for the chosen verification policies, reviewing its permissions.
- Create an artifact configuration for the GitHub Actions artifact containing
  `Streamerz-windows-x86_64.exe`. GitHub's upload-artifact v4 produces a ZIP, so the SignPath artifact
  configuration must match that archive and the EXE inside it.
- Set and enforce the required Windows file metadata. `build.rs` embeds the icon and Windows version information;
  product name and matching product version attributes are generated from the release tag in `build.rs`; the Windows build checks them. Verify them on the real release candidate too.
- Create the signing policy and approver roles with SignPath. Its Foundation conditions require each
  signing request to be approved by a trusted team member. Configure origin and pipeline restrictions
  with SignPath so only intended repository builds can be signed.
- Store the SignPath API token as a GitHub Actions secret with the minimum available permissions. Keep
  the organization ID, project slug, policy slug and artifact configuration slug in GitHub Actions
  variables or other non-secret configuration. Do not paste the token into source code or logs.

## Release workflow design

The current `.github/workflows/build.yml` builds and tests on a pull request, uploads artifacts named by
version and tree hash, and reuses them on the merge when both match. Signing must preserve that property:

1. Build and test the Windows EXE, then upload it as a distinct unsigned input artifact.
2. For a trusted, eligible release candidate, submit that artifact to SignPath using its official GitHub
   Action. Wait for the configured signing approval and download the signed output into a separate path.
3. Verify the signature, publisher, timestamp, filename and version metadata on Windows. Run the
   applicable checks on the signed EXE. Upload only the verified signed EXE as the version-and-tree
   artifact that the release job can reuse.
4. On merge, publish that exact artifact. Ensure the reuse lookup cannot select an unsigned artifact,
   including builds made before signing was configured. A missing, failed or denied signing request
   must not silently produce an unsigned release once signing is required.
5. Test fork pull requests and untrusted changes without exposing signing credentials or signing their
   artifacts. Account for manual approval, GitHub job time limits, cancellation on new PR pushes, and
   the existing fallback build when no matching PR artifact is available. Agree the approved build
   origins and release fallback with SignPath before enabling the workflow.

Signing changes the EXE bytes, so the signed artifact must be stored and reused, not reconstructed or
signed again during publication. Do not change the repository tree between building and release.
The exact SignPath slugs, policy, action version and approval timeout are onboarding decisions, so no
live signing step is enabled yet.

## Verify on Windows

For a downloaded release, compare `Get-FileHash -Algorithm SHA256` with the GitHub asset digest and use
`Get-AuthenticodeSignature` to inspect signature status, signer and timestamp. A matching hash confirms
that the file matches GitHub's metadata; the signature establishes the publisher identity when valid.
Test a browser download separately because it carries Windows download-origin information. Record the
SmartScreen result without changing protection settings. Even a valid trusted signature does not promise
an immediate positive SmartScreen reputation.
