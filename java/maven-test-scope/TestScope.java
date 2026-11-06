package ai.openidea.maven;

import java.io.File;
import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Proxy;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.HashMap;
import java.util.Map;
import org.apache.maven.AbstractMavenLifecycleParticipant;
import org.apache.maven.MavenExecutionException;
import org.apache.maven.execution.ExecutionEvent;
import org.apache.maven.execution.ExecutionListener;
import org.apache.maven.execution.MavenSession;
import org.apache.maven.plugin.MojoExecution;
import org.apache.maven.project.MavenProject;
import org.codehaus.plexus.util.xml.Xpp3Dom;

/** Applies a selection to the effective test mojo configuration in each reactor project. */
public final class TestScope extends AbstractMavenLifecycleParticipant {
    @Override
    public void afterProjectsRead(MavenSession session) throws MavenExecutionException {
        String file = session.getUserProperties().getProperty("openidea.maven.testSelection");
        if (file == null) {
            return;
        }

        Map<File, Selection> active = new HashMap<>();
        try {
            Map<Path, Selection> selected = readSelection(Paths.get(file));
            for (MavenProject project : session.getProjects()) {
                File dir = project.getBasedir();
                if (dir == null) {
                    continue;
                }
                Selection tests = selected.remove(dir.toPath().toRealPath());
                if (tests != null) {
                    active.put(dir, tests);
                }
            }
            if (!selected.isEmpty()) {
                throw new MavenExecutionException("Selected test modules are outside the Maven reactor: "
                    + selected.keySet(), (File) null);
            }
        } catch (IOException e) {
            throw new MavenExecutionException("Cannot read the OpenIDEA Maven test selection", e);
        }

        // Maven has merged defaults, plugin management, and execution-specific options by mojoStarted.
        ExecutionListener previous = session.getRequest().getExecutionListener();
        session.getRequest().setExecutionListener((ExecutionListener) Proxy.newProxyInstance(
            ExecutionListener.class.getClassLoader(), new Class<?>[] { ExecutionListener.class },
            (proxy, method, args) -> {
                if ("mojoStarted".equals(method.getName())) {
                    ExecutionEvent event = (ExecutionEvent) args[0];
                    configure(event.getMojoExecution(), active.get(event.getProject().getBasedir()));
                }
                if (previous == null) {
                    return null;
                }
                try {
                    return method.invoke(previous, args);
                } catch (InvocationTargetException e) {
                    throw e.getCause();
                }
            }));
    }

    private static Map<Path, Selection> readSelection(Path file) throws IOException {
        String[] fields = new String(Files.readAllBytes(file), StandardCharsets.UTF_8).split("\u0000", -1);
        Map<Path, Selection> selection = new HashMap<>();
        for (int i = 0; i + 2 < fields.length; i += 3) {
            selection.put(Paths.get(fields[i]).toRealPath(), new Selection(fields[i + 1], fields[i + 2]));
        }
        return selection;
    }

    private static void configure(MojoExecution mojo, Selection selection) {
        if (!"org.apache.maven.plugins".equals(mojo.getGroupId())) {
            return;
        }
        String goal = mojo.getGoal();
        if ("maven-surefire-plugin".equals(mojo.getArtifactId()) && "test".equals(goal)) {
            String tests = selection == null ? "" : selection.surefire;
            if (tests.isEmpty()) {
                set(mojo, "skipTests", "true");
            } else {
                set(mojo, "test", tests);
            }
        } else if ("maven-failsafe-plugin".equals(mojo.getArtifactId())) {
            String tests = selection == null ? "" : selection.failsafe;
            if ("integration-test".equals(goal) || "verify".equals(goal)) {
                if (tests.isEmpty()) {
                    set(mojo, "skipITs", "true");
                } else if ("integration-test".equals(goal)) {
                    set(mojo, "test", tests);
                }
            }
        }
    }

    private static void set(MojoExecution mojo, String name, String value) {
        Xpp3Dom original = mojo.getConfiguration();
        Xpp3Dom config = original == null ? new Xpp3Dom("configuration") : new Xpp3Dom(original);
        Xpp3Dom option = config.getChild(name);
        if (option == null) {
            option = new Xpp3Dom(name);
            config.addChild(option);
        }
        option.setValue(value);
        mojo.setConfiguration(config);
    }

    private static final class Selection {
        final String surefire;
        final String failsafe;

        Selection(String surefire, String failsafe) {
            this.surefire = surefire;
            this.failsafe = failsafe;
        }
    }
}
