package ai.openidea.build;

import java.io.IOException;
import java.util.LinkedHashMap;
import org.eclipse.core.internal.events.BuildCommand;
import org.eclipse.core.internal.resources.Project;
import org.eclipse.core.resources.IMarker;
import org.eclipse.core.resources.IProject;
import org.eclipse.core.resources.IResource;
import org.eclipse.core.resources.IResourceChangeEvent;
import org.eclipse.core.resources.IResourceChangeListener;
import org.eclipse.core.resources.IWorkspace;
import org.eclipse.core.resources.IncrementalProjectBuilder;
import org.eclipse.core.resources.ResourcesPlugin;
import org.eclipse.core.runtime.CoreException;
import org.eclipse.core.runtime.IProgressMonitor;
import org.eclipse.core.runtime.NullProgressMonitor;
import org.eclipse.core.runtime.OperationCanceledException;
import org.eclipse.core.runtime.jobs.Job;
import org.eclipse.jdt.core.IJavaModelMarker;
import org.eclipse.jdt.core.IJavaProject;
import org.eclipse.jdt.core.JavaCore;
import org.eclipse.jdt.core.compiler.CompilationParticipant;
import org.eclipse.jdt.internal.core.JavaModelManager;
import org.eclipse.jdt.internal.core.builder.JavaBuilder;
import org.eclipse.jdt.ls.core.internal.JavaLanguageServerPlugin;
import org.osgi.framework.BundleActivator;
import org.osgi.framework.BundleContext;

/** One transaction for the JDT builders that actually run in a workspace auto build. */
public final class AutoBuildOutputs implements BundleActivator, IResourceChangeListener {
    private static final ThreadLocal<Build> current = new ThreadLocal<>();
    private static volatile boolean retryAfterExplicitRollback;

    static void explicitBuildFinished(boolean rolledBack) {
        retryAfterExplicitRollback = rolledBack;
    }

    @Override
    public void start(BundleContext context) {
        ResourcesPlugin.getWorkspace().addResourceChangeListener(this,
                IResourceChangeEvent.PRE_BUILD | IResourceChangeEvent.POST_BUILD);
    }

    @Override
    public void stop(BundleContext context) {
        ResourcesPlugin.getWorkspace().removeResourceChangeListener(this);
    }

    @Override
    public void resourceChanged(IResourceChangeEvent event) {
        var workspace = ResourcesPlugin.getWorkspace();
        if (event.getBuildKind() != IncrementalProjectBuilder.AUTO_BUILD || event.getSource() != workspace) return;
        if (event.getType() == IResourceChangeEvent.PRE_BUILD) {
            Job job = Job.getJobManager().currentJob();
            if (current.get() == null && workspace.isAutoBuilding()
                    && job != null && job.belongsTo(ResourcesPlugin.FAMILY_AUTO_BUILD)) {
                if (retryAfterExplicitRollback) {
                    retryAfterExplicitRollback = false;
                    prepareRetry(workspace, job);
                }
                current.set(new Build(job));
            }
        } else {
            Build build = current.get();
            current.remove();
            if (build != null) build.finish();
        }
    }

    public static final class Participant extends CompilationParticipant {
        @Override
        public boolean isActive(IJavaProject project) {
            return current.get() != null;
        }

        @Override
        public int aboutToBuild(IJavaProject project) {
            Build build = current.get();
            if (build != null) build.capture(project);
            return READY_FOR_BUILD;
        }
    }

    private record Snapshot(IncrementalProjectBuilder builder, BuildOutputs outputs) {}

    private static final class Build {
        private final LinkedHashMap<IProject, Snapshot> projects = new LinkedHashMap<>();
        private final Job job;
        private final IProgressMonitor monitor;
        private boolean captureFailed;

        Build(Job job) {
            this.job = job;
            monitor = monitor(job);
        }

        void capture(IJavaProject java) {
            IProject project = java.getProject();
            if (projects.containsKey(project)) return;
            try {
                var builder = javaBuilder(project);
                if (builder == null) throw new IllegalStateException("No active Java builder for " + project.getName());
                var state = JavaLanguageServerPlugin.getInstance().getStateLocation().toPath();
                // The auto build owns the job monitor's progress budget.
                var outputs = BuildOutputs.capture(state, BuildOutputs.folders(java), monitor.slice(0));
                projects.put(project, new Snapshot(builder, outputs));
            } catch (OperationCanceledException e) {
                captureFailed = true;
                throw e;
            } catch (CoreException | IOException | RuntimeException e) {
                captureFailed = true;
                // Abort this project's Java builder before it writes unprotected output.
                throw new IllegalStateException("Cannot protect auto build outputs for " + project.getName(), e);
            }
        }

        void finish() {
            if (projects.isEmpty()) return;
            boolean cancelled = monitor.isCanceled();
            boolean rebuild = false;
            try {
                if (captureFailed || cancelled || interrupted(job) || !ResourcesPlugin.getWorkspace().isAutoBuilding()
                        || projects.keySet().stream().anyMatch(Build::hasFailedBuild)) {
                    for (Snapshot snapshot : projects.sequencedValues().reversed()) {
                        try {
                            rebuild |= snapshot.outputs().restore();
                        } catch (IOException e) {
                            rebuild = true;
                            JavaLanguageServerPlugin.logException("Cannot restore Java auto build outputs", e);
                        }
                    }
                }
            } finally {
                for (var entry : projects.entrySet()) {
                    Snapshot snapshot = entry.getValue();
                    snapshot.outputs().close();
                    if (rebuild) forgetBuildState(entry.getKey(), snapshot.builder());
                }
                if (rebuild && cancelled) allowNextBuild(job);
            }
        }

        private static boolean hasFailedBuild(IProject project) {
            return JavaModelManager.getJavaModelManager().getLastBuiltState(project, null) == null || hasErrors(project);
        }

        private static boolean hasErrors(IProject project) {
            try {
                if (project.findMaxProblemSeverity(IJavaModelMarker.BUILDPATH_PROBLEM_MARKER, true,
                        IResource.DEPTH_INFINITE) == IMarker.SEVERITY_ERROR) return true;
                for (IMarker marker : JavaBuilder.getProblemsFor(project)) {
                    if (marker.getAttribute(IMarker.SEVERITY, -1) == IMarker.SEVERITY_ERROR) return true;
                }
                return false;
            } catch (CoreException e) {
                JavaLanguageServerPlugin.logException("Cannot check Java auto build diagnostics", e);
                return true;
            }
        }
    }

    private static IncrementalProjectBuilder javaBuilder(IProject project) throws CoreException {
        // A public project description is a copy without the live builder instances.
        for (var command : ((Project) project).internalGetDescription().getBuildSpec(false)) {
            if (JavaCore.BUILDER_ID.equals(command.getBuilderName())) {
                var builder = ((BuildCommand) command).getBuilder(project.getActiveBuildConfig());
                if (builder != null) return builder;
            }
        }
        return null;
    }

    private static void prepareRetry(IWorkspace workspace, Job job) {
        for (IProject project : workspace.getRoot().getProjects()) {
            if (!project.isOpen()) continue;
            try {
                var builder = javaBuilder(project);
                if (builder != null) forgetBuildState(project, builder);
            } catch (CoreException e) {
                JavaLanguageServerPlugin.logException("Cannot reset Java build state for " + project.getName(), e);
            }
        }
        // Eclipse would otherwise skip this build after the explicit build.
        allowNextBuild(job);
    }

    private static void forgetBuildState(IProject project, IncrementalProjectBuilder builder) {
        // JDT caches types and Eclipse caches which builders need a delta.
        JavaModelManager.getJavaModelManager().setLastBuiltState(project, null);
        builder.forgetLastBuiltState();
    }

    private static IProgressMonitor monitor(Job job) {
        // POST_BUILD runs before the job publishes its result and exposes no monitor.
        // Read the running job's monitor to detect cancellation between modules.
        try {
            var getter = Job.class.getSuperclass().getDeclaredMethod("getProgressMonitor");
            getter.setAccessible(true);
            if (getter.invoke(job) instanceof IProgressMonitor progress) return progress;
        } catch (ReflectiveOperationException | RuntimeException e) {
            JavaLanguageServerPlugin.logException("Cannot read Java auto build cancellation state", e);
        }
        return new NullProgressMonitor();
    }
    private static boolean interrupted(Job job) {
        // AutoBuildJob also interrupts builds to yield the workspace rule, without
        // cancelling the monitor. Reading the flag avoids triggering a late interrupt.
        try {
            var flag = job.getClass().getDeclaredField("interrupted");
            flag.setAccessible(true);
            return flag.getBoolean(job);
        } catch (ReflectiveOperationException | RuntimeException e) {
            JavaLanguageServerPlugin.logException("Cannot read Java auto build interruption state", e);
            return false;
        }
    }
    private static void allowNextBuild(Job job) {
        // forceBuild only sets a flag for the next scheduled job.
        try {
            var force = job.getClass().getDeclaredMethod("forceBuild");
            force.setAccessible(true);
            force.invoke(job);
        } catch (ReflectiveOperationException | RuntimeException e) {
            JavaLanguageServerPlugin.logException("Cannot reenable the next Java auto build", e);
        }
    }
}
