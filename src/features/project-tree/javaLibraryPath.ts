import { jdtClassLocation } from "@/platform/virtualDocs";
import { nameOf } from "./paths";

interface ArchiveEntries {
  root: string;
  entries: string[];
}

/** Paths to check, restricted to the archives already contributed by External Libraries. */
export function javaLibraryEntries(uri: string, roots: readonly string[]): ArchiveEntries[] {
  const location = jdtClassLocation(uri);
  if (!location) return [];
  const { archive, module, classEntry, sourceEntry } = location;
  const outer = sourceEntry.replace(/\$[^./]+\.java$/, ".java");
  const sources = outer === sourceEntry ? [sourceEntry] : [sourceEntry, outer];
  const binary = `${archive}!`;
  if (roots.includes(binary))
    return [{ root: binary, entries: archive.endsWith("-sources.jar") ? sources : [classEntry, ...sources] }];

  const jdk = /^(.*?)\/(?:lib\/jrt-fs\.jar|(?:jre\/)?lib\/rt\.jar|jmods\/[^/]+\.jmod)$/.exec(archive);
  if (jdk) {
    const sdkArchives = [`${jdk[1]}/lib/src.zip!`, `${jdk[1]}/src.zip!`];
    const entries = module ? [...sources.map((path) => `${module}/${path}`), ...sources] : sources;
    return sdkArchives.filter((root) => roots.includes(root)).map((root) => ({ root, entries }));
  }

  const sourceJar = archive.replace(/\.jar$/i, "-sources.jar");
  const sibling = `${sourceJar}!`;
  if (sourceJar !== archive && roots.includes(sibling)) return [{ root: sibling, entries: sources }];

  // Gradle stores the source and binary in separate hash directories of the same artifact version.
  const gradleVersion = (path: string) =>
    /^(.*\/caches\/modules-2\/files-2\.1\/[^/]+\/[^/]+\/[^/]+)\/[^/]+\/[^/]+$/.exec(path)?.[1];
  const version = gradleVersion(archive);
  const matches = version
    ? roots.filter((root) => gradleVersion(root) === version && nameOf(root) === nameOf(sibling))
    : [];
  return matches.length === 1 ? [{ root: matches[0]!, entries: sources }] : [];
}
