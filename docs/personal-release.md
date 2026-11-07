# Personal Mac release defaults

Starting with 0.1.40, the signed updater package includes two default plugins:
`hyhanyue.mybatis` and `hyhanyue.spring-beans`. They are installed into the app's
managed plugin store at startup, without links to a development checkout. Their
package version includes the app version so future releases update their code.
After the first setup, a user's disabled choice is retained.

Resource preparation derives the plugins' minimum API from the current WIT and
SDK versions and refuses mismatches or stale component source fingerprints. It
uses `plugins/build.sh --fingerprint` without compiling or downloading tools.
Run `python3 -m unittest discover -s scripts -p test_fork_resources.py` for these
packaging gates before preparing release resources.

The first setup migrates the earlier `*-local` development links. Personal
Spring icon overrides from that local plugin are saved in app data and applied
to the installed copy. Those assets are never part of the public package.
Replaced upstream Spring and MyBatis providers stay disabled to avoid duplicates.

On this Mac, Spring Tools starts automatically for Java/properties/YAML files
when the selected Maven reactor or linked Gradle build uses Spring Boot. Module
dependencies and cached resolved dependencies count; selected Maven profiles and
unlinked projects follow the existing build model. Commented Gradle plugins do
not enable services or create phantom Boot tasks.
Ordinary Java projects do not start Spring Tools. Existing custom Spring server
settings take precedence, including an explicit disabled choice. The server uses
an independent suitable tooling JDK (21+), preferring the valid configured
language-server JDK; the project's Java 8 SDK is unchanged.
JDT receives the bundled official Spring extensions for its Java model bridge.

Spring Tools 2.3.0 comes from the pinned official Open VSX archive, verified by
SHA-256 during resource preparation. Its complete license/notice files are kept
with the extension; the public source is https://github.com/spring-projects/spring-tools.
The commercial IntelliJ server, its trial state, EULA acceptance, index caches
and the user's original commercial Spring icons are not release resources.
Existing authorized IntelliJ project configurations can continue to be used.

The fork's `Fork macOS Release` workflow builds Apple Silicon updater packages
using the existing `TAURI_SIGNING_PRIVATE_KEY` repository secret. Its verification
checks the release version, own-fork download URL, matching signature asset and
required bundled defaults. The generic upstream release workflow is disabled
for this personal fork; other repositories keep its original behavior.

Release procedure:

1. Run `node scripts/release-prepare.ts <version>` with the Rust toolchain on PATH.
2. Build/check the changed plugins; run host default-install/migration tests and
   relevant Java 8 and editor checks. Commit and merge into the fork's main branch.
3. Create the matching tag. Dispatch `fork-release.yml` with that tag and
   `publish=false` to retain a draft for local acceptance.
4. Run `python3 scripts/verify-fork-release.py <tag> --download-dir <directory>`.
   Install the signed updater through the app and verify the default plugins,
   Spring injection navigation and Java 8 run workflow.
5. Publish the verified draft. `latest.json` then advertises it to this fork's
   installed clients. A routine dispatch can instead use `publish=true`.

Only macOS Apple Silicon is advertised by this fork's personal updater release.
