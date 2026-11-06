package ai.openidea.build;

import static java.nio.file.LinkOption.NOFOLLOW_LINKS;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import org.eclipse.core.resources.IFile;
import org.eclipse.core.resources.IFolder;
import org.eclipse.core.resources.IResource;
import org.eclipse.core.runtime.CoreException;
import org.eclipse.core.runtime.IPath;
import org.eclipse.core.runtime.IProgressMonitor;
import org.eclipse.core.runtime.OperationCanceledException;
import org.eclipse.core.runtime.SubMonitor;
import org.eclipse.jdt.ls.core.internal.JavaLanguageServerPlugin;

/** Copies the configured output files, and undoes only changes recorded by Eclipse's resource tree. */
final class BuildOutputs implements AutoCloseable {
    private record SavedFile(IFile resource, Path saved, Path output, long stamp) {}

    private final Path temporary;
    private final Map<Path, IFolder> outputs = new LinkedHashMap<>();
    private final Map<Path, SavedFile> previous = new LinkedHashMap<>();
    private boolean keep;

    private BuildOutputs(Path temporary) {
        this.temporary = temporary;
    }

    static BuildOutputs capture(Path state, Set<IFolder> folders, IProgressMonitor monitor)
            throws CoreException, IOException {
        var backup = new BuildOutputs(Files.createTempDirectory(state, "openidea-jdt-build-"));
        boolean captured = false;
        try {
            var progress = SubMonitor.convert(monitor, folders.size());
            for (IFolder folder : folders) {
                var folderProgress = progress.split(1);
                if (folder.getLocation() == null || folder.getLocation().isPrefixOf(folder.getProject().getLocation())) {
                    continue;
                }
                Path output = folder.getLocation().toPath();
                if (backup.outputs.putIfAbsent(output, folder) != null || !Files.isDirectory(output, NOFOLLOW_LINKS)) continue;
                // A preceding rollback may have restored a file on disk that Eclipse
                // still considers deleted. Start with matching resource stamps.
                folder.refreshLocal(IResource.DEPTH_INFINITE, folderProgress);
                try (var files = Files.walk(output)) {
                    for (Path path : (Iterable<Path>) files::iterator) {
                        if (monitor.isCanceled()) throw new OperationCanceledException();
                        if (!Files.isRegularFile(path, NOFOLLOW_LINKS) || backup.previous.containsKey(path)) continue;
                        IFile resource = file(folder, output, path);
                        Path saved = backup.temporary.resolve(resource.getFullPath().makeRelative().toOSString());
                        Files.createDirectories(saved.getParent());
                        Files.copy(path, saved, StandardCopyOption.COPY_ATTRIBUTES, NOFOLLOW_LINKS);
                        backup.previous.put(path, new SavedFile(resource, saved, output, resource.getModificationStamp()));
                    }
                }
            }
            captured = true;
            return backup;
        } finally {
            if (!captured) backup.close();
        }
    }

    boolean restore() throws IOException {
        keep = true;
        try {
            boolean changed = restoreFiles();
            keep = false;
            return changed;
        } catch (IOException | UncheckedIOException e) {
            throw new IOException("Previous outputs retained at " + temporary, e);
        }
    }

    private boolean restoreFiles() throws IOException {
        boolean changed = false;
        for (var entry : previous.entrySet()) {
            Path path = entry.getKey();
            SavedFile old = entry.getValue();
            IFile file = old.resource();
            if (file.getModificationStamp() == old.stamp() || !canRestore(path, old)) continue;
            if (file.exists() && Files.mismatch(path, old.saved()) == -1) continue;
            Files.createDirectories(path.getParent());
            Path staged = Files.createTempFile(path.getParent(), ".openidea-build-", ".tmp");
            try {
                Files.copy(old.saved(), staged, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.COPY_ATTRIBUTES);
                if (canRestore(path, old)) {
                    Files.move(staged, path, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
                    changed = true;
                }
            } finally {
                Files.deleteIfExists(staged);
            }
        }
        for (var entry : outputs.entrySet()) {
            Path output = entry.getKey();
            if (!Files.isDirectory(output, NOFOLLOW_LINKS)) continue;
            try (var files = Files.walk(output)) {
                for (Path path : (Iterable<Path>) files::iterator) {
                    if (previous.containsKey(path) || !Files.isRegularFile(path, NOFOLLOW_LINKS)) continue;
                    IFile file = file(entry.getValue(), output, path);
                    if (file.isDerived() && isCurrentOutput(path, file) && Files.deleteIfExists(path)) changed = true;
                }
            }
        }
        return changed;
    }

    private static boolean canRestore(Path path, SavedFile old) throws IOException {
        for (Path parent = path.getParent(); parent.startsWith(old.output()); parent = parent.getParent()) {
            if (Files.isSymbolicLink(parent)) return false;
        }
        // If JDT deleted the file, leave a replacement by another writer alone.
        return old.resource().exists() ? isCurrentOutput(path, old.resource()) : Files.notExists(path, NOFOLLOW_LINKS);
    }

    private static IFile file(IFolder folder, Path output, Path path) {
        return folder.getFile(IPath.fromOSString(output.relativize(path).toString()));
    }

    private static boolean isCurrentOutput(Path path, IFile file) throws IOException {
        // Preserve an external writer's newer version when its timestamp differs from
        // the one Eclipse recorded. The workspace rule serializes Eclipse writers.
        return file.exists() && Files.isRegularFile(path, NOFOLLOW_LINKS)
                && Files.getLastModifiedTime(path, NOFOLLOW_LINKS).toMillis() == file.getLocalTimeStamp();
    }

    @Override
    public void close() {
        if (keep) {
            JavaLanguageServerPlugin.logError("Previous Java build outputs retained at " + temporary);
            return;
        }
        try (var files = Files.walk(temporary)) {
            for (Path file : (Iterable<Path>) files.sorted(Comparator.reverseOrder())::iterator) Files.delete(file);
        } catch (IOException | UncheckedIOException e) {
            JavaLanguageServerPlugin.logException("Cannot remove Java build backup " + temporary, e);
        }
    }
}
