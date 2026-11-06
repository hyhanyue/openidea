package ai.openidea.build;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import org.eclipse.core.resources.IFolder;
import org.eclipse.core.resources.IProject;
import org.eclipse.core.resources.IWorkspace;
import org.eclipse.core.resources.IncrementalProjectBuilder;
import org.eclipse.core.resources.ResourcesPlugin;
import org.eclipse.core.runtime.CoreException;
import org.eclipse.core.runtime.IProgressMonitor;
import org.eclipse.core.runtime.OperationCanceledException;
import org.eclipse.core.runtime.Status;
import org.eclipse.core.runtime.SubMonitor;
import org.eclipse.jdt.core.JavaCore;
import org.eclipse.jdt.ls.core.internal.BuildWorkspaceStatus;
import org.eclipse.jdt.ls.core.internal.IDelegateCommandHandler;
import org.eclipse.jdt.ls.core.internal.JavaLanguageServerPlugin;
import org.eclipse.jdt.ls.core.internal.handlers.BuildWorkspaceHandler;

/** Runs the standard JDT LS build with rollback under the workspace's scheduling rule. */
public final class BuildCommand implements IDelegateCommandHandler {
    @Override
    public Object executeCommand(String command, List<Object> args, IProgressMonitor monitor) throws CoreException {
        IWorkspace workspace = ResourcesPlugin.getWorkspace();
        BuildWorkspaceStatus[] result = { BuildWorkspaceStatus.FAILED };
        try {
            workspace.run(progress -> {
                try {
                    // The client can also wait for a cancelled build to release this rule.
                    result[0] = args.isEmpty() ? build(workspace, progress) : BuildWorkspaceStatus.SUCCEED;
                } catch (IOException | UncheckedIOException e) {
                    throw new CoreException(Status.error("Cannot protect Java build outputs: " + e.getMessage(), e));
                }
            }, workspace.getRoot(), IWorkspace.AVOID_UPDATE, monitor);
        } catch (OperationCanceledException e) {
            return BuildWorkspaceStatus.CANCELLED.ordinal();
        }
        return result[0].ordinal();
    }

    private BuildWorkspaceStatus build(IWorkspace workspace, IProgressMonitor monitor)
            throws CoreException, IOException {
        Set<IFolder> outputs = new LinkedHashSet<>();
        for (IProject project : workspace.getRoot().getProjects()) {
            if (!project.isOpen() || !project.hasNature(JavaCore.NATURE_ID)) continue;
            outputs.addAll(BuildOutputs.folders(JavaCore.create(project)));
        }
        var state = JavaLanguageServerPlugin.getInstance().getStateLocation().toPath();
        var marker = state.resolve("openidea-rebuild-java");
        var progress = SubMonitor.convert(monitor, 3);
        try (var backup = BuildOutputs.capture(state, outputs, progress.split(1))) {
            // The builder can persist state for prerequisite outputs we roll back.
            // Force the next explicit build to rebuild them, even after a server restart.
            boolean rebuild = Files.exists(marker);
            if (!rebuild) Files.createFile(marker);
            BuildWorkspaceStatus result = BuildWorkspaceStatus.FAILED;
            try {
                var handler = new BuildWorkspaceHandler(JavaLanguageServerPlugin.getProjectsManager());
                if (rebuild) {
                    // Force all builders to run without invoking their separate clean hooks.
                    workspace.build(IncrementalProjectBuilder.FULL_BUILD, progress.split(1));
                }
                result = handler.buildWorkspace(false, progress.split(rebuild ? 1 : 2));
                if (progress.isCanceled()) result = BuildWorkspaceStatus.CANCELLED;
                return result;
            } catch (CoreException e) {
                JavaLanguageServerPlugin.log(e);
                return BuildWorkspaceStatus.FAILED;
            } finally {
                boolean succeeded = result == BuildWorkspaceStatus.SUCCEED;
                boolean rolledBack = !succeeded && backup.restore();
                if (succeeded || rolledBack) AutoBuildOutputs.explicitBuildFinished(rolledBack);
                if (succeeded || (!rolledBack && !rebuild)) {
                    try {
                        Files.deleteIfExists(marker);
                    } catch (IOException e) {
                        JavaLanguageServerPlugin.logException("Cannot clear Java rebuild marker", e);
                    }
                }
            }
        }
    }
}
