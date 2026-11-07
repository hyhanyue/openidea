package ai.openidea.spring;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicLong;
import java.util.zip.ZipFile;
import org.eclipse.core.runtime.*;
import org.eclipse.core.resources.ResourcesPlugin;
import org.eclipse.jdt.core.*;
import org.eclipse.jdt.core.dom.*;
import org.eclipse.jdt.ls.core.internal.*;
import org.eclipse.lsp4j.Location;

/** A static model of registrations, scoped exclusively by JDT's resolved build path.
 * Never loads application classes. Binary bindings use JDT's class-file reader. */
public final class BeansCommand implements IDelegateCommandHandler {
    private static final String SPRING = "org.springframework.";
    private static final String IMPORTS = "META-INF/spring/org.springframework.boot.autoconfigure.AutoConfiguration.imports";
    private static final String AUTO = "org.springframework.boot.autoconfigure.EnableAutoConfiguration";
    private static final int MAX_UNITS = 4000, MAX_TYPES = 16000, MAX_BEANS = 12000;
    private static final int MAX_BYTES = 1024 * 1024, MAX_ARCHIVES = 512, MAX_ENTRIES = 100000;
    private static final AtomicLong REVISION = new AtomicLong();
    private record Cached(long revision, String fingerprint, Map<String, Object> value) {}
    private static final Map<String, Cached> CACHE = new LinkedHashMap<>();
    private static final java.util.concurrent.locks.ReentrantLock BUILD = new java.util.concurrent.locks.ReentrantLock();
    private static final IElementChangedListener LISTENER = e -> {
        REVISION.incrementAndGet();
        synchronized (CACHE) { CACHE.clear(); }
    };
    static { JavaCore.addElementChangedListener(LISTENER, ElementChangedEvent.POST_CHANGE | ElementChangedEvent.POST_RECONCILE); }
    public static void dispose() {
        JavaCore.removeElementChangedListener(LISTENER);
        synchronized (CACHE) { CACHE.clear(); }
    }
    private static void check(IProgressMonitor m) { if (m != null && m.isCanceled()) throw new OperationCanceledException(); }
    @Override public Object executeCommand(String command, List<Object> args, IProgressMonitor monitor) throws Exception {
        if (args.isEmpty() || !(args.get(0) instanceof String uri)) throw new IllegalArgumentException("Expected source URI");
        ITypeRoot selected = JDTUtils.resolveTypeRoot(uri);
        IJavaProject project = selected == null ? project(uri) : selected.getJavaProject();
        if (project == null || !project.getProject().isOpen()) return Map.of("beans", List.of(), "incomplete", true);
        String root = args.size() > 1 && args.get(1) instanceof String s ? s : "";
        // Serializes bounded model builds; waiting clients remain cancellable.
        while (!BUILD.tryLock(50, TimeUnit.MILLISECONDS)) check(monitor);
        try {
            check(monitor);
            String key = project.getHandleIdentifier() + "\n" + root;
            var fingerprint = java.security.MessageDigest.getInstance("SHA-256");
            for (IPackageFragmentRoot r : project.getAllPackageFragmentRoots()) {
                check(monitor);
                fingerprint.update(r.getHandleIdentifier().getBytes(java.nio.charset.StandardCharsets.UTF_8));
                if (r.getSourceAttachmentPath() != null) fingerprint.update(r.getSourceAttachmentPath().toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
                if (r.isArchive()) {
                    Path p = path(r);
                    fingerprint.update((Files.exists(p) ? Files.getLastModifiedTime(p).toMillis() + ":" + Files.size(p) : "missing").getBytes(java.nio.charset.StandardCharsets.UTF_8));
                }
            }
            for (ICompilationUnit copy : JavaCore.getWorkingCopies(null)) {
                check(monitor);
                if (copy.hasUnsavedChanges()) { fingerprint.update(copy.getHandleIdentifier().getBytes(java.nio.charset.StandardCharsets.UTF_8)); fingerprint.update(copy.getSource().getBytes(java.nio.charset.StandardCharsets.UTF_8)); }
            }
            for (ICompilationUnit copy : JavaCore.getWorkingCopies(JavaLanguageServerPlugin.getInstance().getWorkingCopyOwner())) {
                check(monitor);
                if (copy.hasUnsavedChanges()) { fingerprint.update(copy.getHandleIdentifier().getBytes(java.nio.charset.StandardCharsets.UTF_8)); fingerprint.update(copy.getSource().getBytes(java.nio.charset.StandardCharsets.UTF_8)); }
            }
            String stamp = java.util.HexFormat.of().formatHex(fingerprint.digest());
            long revision = REVISION.get();
            synchronized (CACHE) {
                Cached cached = CACHE.get(key);
                if (cached != null && cached.revision == revision && cached.fingerprint.equals(stamp)) return cached.value;
            }
            Builder builder = new Builder(project, monitor);
            Map<String, Object> result = builder.build(root);
            if (REVISION.get() == revision) synchronized (CACHE) {
                if (CACHE.size() >= 8) CACHE.remove(CACHE.keySet().iterator().next());
                CACHE.put(key, new Cached(revision, stamp, result));
            }
            return result;
        } finally { BUILD.unlock(); }
    }
    private static IJavaProject project(String uri) {
        Path requested;
        try { requested = Path.of(java.net.URI.create(uri)).toFile().getCanonicalFile().toPath(); } catch (Exception e) { return null; }
        IJavaProject best = null; int length = -1;
        for (var resource : ResourcesPlugin.getWorkspace().getRoot().getProjects()) {
            if (!resource.isOpen() || resource.getLocation() == null) continue;
            var candidate = JavaCore.create(resource);
            try {
                Path root = resource.getLocation().toFile().getCanonicalFile().toPath();
                if (candidate.exists() && requested.startsWith(root) && root.getNameCount() > length) { best = candidate; length = root.getNameCount(); }
            } catch (java.io.IOException e) { /* An unavailable resource is not a resolved model. */ }
        }
        return best;
    }
    private static Path path(IPackageFragmentRoot root) {
        return root.getResource() != null && root.getResource().getLocation() != null
            ? root.getResource().getLocation().toFile().toPath() : root.getPath().toFile().toPath();
    }
    private static String name(ITypeBinding t) { return t == null ? "" : t.getErasure().getQualifiedName(); }
    private static Object value(IAnnotationBinding a, String key) {
        for (IMemberValuePairBinding p : a.getAllMemberValuePairs()) if (p.getName().equals(key)) return p.getValue();
        return null;
    }
    private static List<Object> values(Object v) { return v == null ? List.of() : v instanceof Object[] a ? Arrays.asList(a) : List.of(v); }
    private static List<String> strings(IAnnotationBinding a, String... keys) {
        Set<String> result = new LinkedHashSet<>();
        for (String key : keys) for (Object v : values(value(a, key))) if (v instanceof String s && !s.isEmpty()) result.add(s);
        return new ArrayList<>(result);
    }
    private static List<IAnnotationBinding> annotations(IAnnotationBinding[] annotations) {
        List<IAnnotationBinding> result = new ArrayList<>();
        collect(annotations, result, new HashSet<>(), 0);
        return result;
    }
    private static void collect(IAnnotationBinding[] input, List<IAnnotationBinding> out, Set<String> seen, int depth) {
        if (depth >= 8 || out.size() >= 128) return;
        for (IAnnotationBinding a : input) {
            String n = name(a.getAnnotationType());
            if (!seen.add(n) || n.startsWith("java.lang.annotation.")) continue;
            out.add(a); collect(a.getAnnotationType().getAnnotations(), out, seen, depth + 1); seen.remove(n);
        }
    }
    private static boolean has(List<IAnnotationBinding> as, String n) { return as.stream().anyMatch(a -> name(a.getAnnotationType()).equals(n)); }
    private static String decap(String n) { return n.length() > 1 && Character.isUpperCase(n.charAt(0)) && Character.isUpperCase(n.charAt(1)) ? n : Character.toLowerCase(n.charAt(0)) + n.substring(1); }
    private static Location location(IJavaElement e) throws JavaModelException {
        Location l = JDTUtils.toLocation(e);
        if (l == null && e instanceof IMember m && m.getClassFile() != null) l = JDTUtils.toLocation(m.getClassFile());
        return l;
    }
    private static final class Builder {
        final IJavaProject project;
        final IProgressMonitor monitor;
        final Map<String, ITypeBinding> types = new LinkedHashMap<>();
        final Map<String, String> origins = new HashMap<>();
        final List<Map<String, Object>> injections = new ArrayList<>();
        final Map<String, Integer> states = new LinkedHashMap<>(); // 1 unknown, 2 statically registered
        final Set<String> auto = new LinkedHashSet<>(), excluded = new HashSet<>(), imported = new HashSet<>();
        final Map<String, Integer> scans = new LinkedHashMap<>();
        final List<IPackageFragmentRoot> binaries = new ArrayList<>();
        final Set<String> checkedBinaries = new HashSet<>();
        long classBytes;
        boolean incomplete;
        int units, visited;
        Builder(IJavaProject p, IProgressMonitor m) { project = p; monitor = m; }
        void type(ITypeBinding binding, String origin) { type(binding, origin, 0); }
        void type(ITypeBinding binding, String origin, int depth) {
            check(monitor);
            if (depth >= 24) { incomplete = true; return; }
            if (binding == null || binding.isRecovered() || types.size() >= MAX_TYPES) { incomplete = true; return; }
            if (binding.getJavaElement() instanceof IType type && type.isBinary() && !safeBinary(type)) return;
            String n = name(binding);
            if (n.isEmpty() || types.containsKey(n)) return;
            types.put(n, binding); origins.put(n, origin);
            for (ITypeBinding nested : binding.getDeclaredTypes()) type(nested, origin, depth + 1);
        }
        void source(ICompilationUnit unit) throws JavaModelException {
            if (units >= MAX_UNITS) { incomplete = true; return; }
            units++;
            String source = unit.getSource();
            if (source == null || source.length() > MAX_BYTES) { incomplete = true; return; }
            ASTParser p = ASTParser.newParser(AST.getJLSLatest());
            p.setSource(unit); p.setResolveBindings(true); p.setBindingsRecovery(false);
            p.setWorkingCopyOwner(JavaLanguageServerPlugin.getInstance().getWorkingCopyOwner());
            CompilationUnit ast = (CompilationUnit)p.createAST(monitor);
            ast.accept(new ASTVisitor() {
                @Override public void preVisit(ASTNode n) { check(monitor); }
                @Override public boolean visit(TypeDeclaration t) { type(t.resolveBinding(), "source"); return true; }
                @Override public boolean visit(EnumDeclaration t) { type(t.resolveBinding(), "source"); return true; }
                @Override public boolean visit(VariableDeclarationFragment v) {
                    IVariableBinding binding = v.resolveBinding();
                    if (binding != null && binding.isField()) injection(binding.getType(), v.getParent() instanceof FieldDeclaration f ? f.getType() : v.getName());
                    return true;
                }
                @Override public boolean visit(SingleVariableDeclaration v) { injection(v.getType().resolveBinding(), v.getType()); return true; }
                void injection(ITypeBinding binding, ASTNode n) {
                    if (binding == null || binding.isRecovered() || injections.size() >= MAX_BEANS) return;
                    try {
                        ITypeBinding requested = binding;
                        while (n instanceof ArrayType array) n = array.getElementType();
                        if (n instanceof ParameterizedType parameterized && Set.of("java.util.Optional", "java.util.List", "java.util.Set", "java.util.Collection", "java.util.Map", "org.springframework.beans.factory.ObjectProvider", "javax.inject.Provider").contains(name(requested))) {
                            if (!parameterized.typeArguments().isEmpty()) n = (ASTNode)parameterized.typeArguments().get(parameterized.typeArguments().size() - 1);
                        }
                        while (requested.isArray()) requested = requested.getElementType();
                        if (Set.of("java.util.Optional", "java.util.List", "java.util.Set", "java.util.Collection", "java.util.Map", "org.springframework.beans.factory.ObjectProvider", "javax.inject.Provider").contains(name(requested))) {
                            ITypeBinding[] args = requested.getTypeArguments();
                            if (args.length > 0) requested = args[args.length - 1];
                        }
                        int start = n.getStartPosition();
                        if (n instanceof SimpleType simple) { Name sn = simple.getName(); if (sn instanceof org.eclipse.jdt.core.dom.QualifiedName q) start = q.getName().getStartPosition(); }
                        else if (n instanceof QualifiedType q) start = q.getName().getStartPosition();
                        else if (n instanceof NameQualifiedType q) start = q.getName().getStartPosition();
                        injections.add(Map.of("type", name(requested), "location", JDTUtils.toLocation(unit, start, Math.max(1, requested.getName().length()))));
                    } catch (JavaModelException e) { incomplete = true; }
                }
            });
        }
        void metadata(IPackageFragmentRoot root) {
            check(monitor);
            Path path = path(root);
            // Only named metadata entries are opened. Never extract or recursively open archives.
            try {
                if (!Files.isRegularFile(path) || Files.size(path) > 256L * 1024 * 1024) { incomplete = true; return; }
            } catch (java.io.IOException e) { incomplete = true; return; }
            try (ZipFile zip = new ZipFile(path.toFile())) {
                if (zip.size() > MAX_ENTRIES) { incomplete = true; return; }
                for (String file : List.of(IMPORTS, "META-INF/spring.factories")) {
                    check(monitor);
                    var entry = zip.getEntry(file);
                    if (entry == null) continue;
                    if (entry.getSize() > MAX_BYTES) { incomplete = true; continue; }
                    byte[] bytes;
                    try (var stream = zip.getInputStream(entry)) { bytes = stream.readNBytes(MAX_BYTES + 1); }
                    if (bytes.length > MAX_BYTES) { incomplete = true; continue; }
                    List<String> names;
                    if (file.equals(IMPORTS)) names = Arrays.asList(new String(bytes, java.nio.charset.StandardCharsets.UTF_8).split("\\R"));
                    else {
                        Properties properties = new Properties();
                        properties.load(new java.io.ByteArrayInputStream(bytes));
                        names = Arrays.asList(properties.getProperty(AUTO, "").split(","));
                    }
                    for (String n : names) {
                        check(monitor);
                        n = n.split("#", 2)[0].trim();
                        if (n.matches("[\\p{javaJavaIdentifierStart}][\\p{javaJavaIdentifierPart}.$]*") && auto.size() < MAX_TYPES) auto.add(n.replace('$', '.'));
                        else if (!n.isEmpty()) incomplete = true;
                    }
                }
            } catch (java.io.IOException e) { incomplete = true; }
        }
        boolean safeBinary(IType type) {
            if (!type.isBinary() || checkedBinaries.contains(type.getHandleIdentifier())) return true;
            check(monitor);
            String entryName = type.getFullyQualifiedName('$').replace('.', '/') + ".class";
            if (entryName.split("/").length > 32 || entryName.chars().filter(c -> c == '$').count() > 24 || classBytes > 64L * MAX_BYTES) { incomplete = true; return false; }
            IPackageFragmentRoot root = (IPackageFragmentRoot)type.getAncestor(IJavaElement.PACKAGE_FRAGMENT_ROOT);
            if (root == null) { incomplete = true; return false; }
            Path archive = path(root);
            try {
                if (root.isArchive()) {
                    if (!Files.isRegularFile(archive) || Files.size(archive) > 256L * MAX_BYTES) { incomplete = true; return false; }
                    try (ZipFile zip = new ZipFile(archive.toFile())) {
                        if (zip.size() > MAX_ENTRIES) { incomplete = true; return false; }
                        var entry = zip.getEntry(entryName);
                        if (entry == null || entry.getSize() > MAX_BYTES) { incomplete = true; return false; }
                        try (var stream = zip.getInputStream(entry)) {
                            int bytes = stream.readNBytes(MAX_BYTES + 1).length;
                            if (bytes > MAX_BYTES) { incomplete = true; return false; }
                            classBytes += bytes;
                        }
                    }
                } else {
                    Path file = archive.resolve(entryName).toRealPath();
                    if (!file.startsWith(archive.toRealPath()) || Files.size(file) > MAX_BYTES) { incomplete = true; return false; }
                    classBytes += Files.size(file);
                }
                checkedBinaries.add(type.getHandleIdentifier()); return true;
            } catch (java.io.IOException e) { incomplete = true; return false; }
        }
        ITypeBinding resolve(String n) throws JavaModelException {
            check(monitor);
            if (types.containsKey(n)) return types.get(n);
            IType t = project.findType(n, monitor);
            if (t == null || !safeBinary(t)) return null;
            ASTParser p = ASTParser.newParser(AST.getJLSLatest());
            p.setProject(project); p.setResolveBindings(true);
            IBinding[] bindings = p.createBindings(new IJavaElement[] {t}, monitor);
            if (bindings.length > 0 && bindings[0] instanceof ITypeBinding binding) {
                type(binding, t.isBinary() ? "dependency" : "source"); return binding;
            }
            incomplete = true; return null;
        }
        int condition(List<IAnnotationBinding> as) throws JavaModelException {
            int state = 2;
            for (IAnnotationBinding a : as) {
                check(monitor);
                String n = name(a.getAnnotationType());
                if (n.equals("org.springframework.boot.autoconfigure.condition.ConditionalOnClass")) {
                    for (String required : strings(a, "name")) if (project.findType(required, monitor) == null) return 0;
                    for (Object v : values(value(a, "value"))) if (!(v instanceof ITypeBinding t) || t.isRecovered() || project.findType(name(t), monitor) == null) return 0;
                } else if (n.equals("org.springframework.context.annotation.Profile") || n.startsWith(SPRING + "boot.autoconfigure.condition.ConditionalOn") || n.equals("org.springframework.context.annotation.Conditional")) {
                    // OnClass's own meta Conditional is handled by the branch above.
                    if (n.equals("org.springframework.context.annotation.Conditional") && !values(value(a, "value")).isEmpty() && values(value(a, "value")).stream().allMatch(v -> v instanceof ITypeBinding t && name(t).equals("org.springframework.boot.autoconfigure.condition.OnClassCondition"))) continue;
                    state = 1;
                }
            }
            return state;
        }
        void register(String n, int state) throws JavaModelException {
            ITypeBinding t = types.get(n);
            if (t != null) state = Math.min(state, condition(annotations(t.getAnnotations())));
            if (state > states.getOrDefault(n, 0)) states.put(n, state);
        }
        Map<String, Object> build(String selected) throws Exception {
            int archives = 0;
            Map<String, ICompilationUnit> copies = new HashMap<>();
            for (ICompilationUnit c : JavaCore.getWorkingCopies(JavaLanguageServerPlugin.getInstance().getWorkingCopyOwner())) copies.put(c.getPrimary().getHandleIdentifier(), c);
            for (IPackageFragmentRoot root : project.getAllPackageFragmentRoots()) {
                check(monitor);
                if (root.getRawClasspathEntry().isTest()) continue;
                if (root.getKind() == IPackageFragmentRoot.K_SOURCE) {
                    for (IJavaElement c : root.getChildren()) if (c instanceof IPackageFragment pkg) for (ICompilationUnit u : pkg.getCompilationUnits()) {
                        source(copies.getOrDefault(u.getPrimary().getHandleIdentifier(), u));
                        if (units >= MAX_UNITS) { incomplete = true; break; }
                    }
                } else if (!root.getElementName().startsWith("java.")) {
                    binaries.add(root);
                    if (root.isArchive() && ++archives <= MAX_ARCHIVES) metadata(root);
                    else if (archives > MAX_ARCHIVES) incomplete = true;
                }
            }
            List<String> boots = types.entrySet().stream().filter(e -> has(annotations(e.getValue().getAnnotations()), "org.springframework.boot.autoconfigure.SpringBootApplication")).map(Map.Entry::getKey).toList();
            String root = selected.isEmpty() && boots.size() == 1 ? boots.get(0) : selected;
            ITypeBinding rootType = types.get(root);
            if (rootType == null) return Map.of("beans", List.of(), "injections", injections, "incomplete", true, "root", root);
            List<IAnnotationBinding> rootAnnotations = annotations(rootType.getAnnotations());
            for (IAnnotationBinding a : rootAnnotations) for (Object v : values(value(a, "exclude"))) if (v instanceof ITypeBinding t) excluded.add(name(t));
            for (IAnnotationBinding a : rootAnnotations) excluded.addAll(strings(a, "excludeName"));
            boolean enabledAuto = has(rootAnnotations, AUTO);
            register(root, 2);
            if (enabledAuto) for (String n : auto) if (!excluded.contains(n) && resolve(n) != null) { imported.add(n); register(n, 2); }
            Set<String> processed = new HashSet<>(), scanned = new HashSet<>();
            while (true) {
                boolean changed = false;
                for (var e : new ArrayList<>(states.entrySet())) {
                    check(monitor);
                    if (!processed.add(e.getKey() + ":" + e.getValue())) continue;
                    changed = true;
                    ITypeBinding t = resolve(e.getKey());
                    if (t == null) { incomplete = true; continue; }
                    List<IAnnotationBinding> as = annotations(t.getAnnotations());
                    int state = Math.min(e.getValue(), condition(as));
                    
                    if (state == 0) continue;
                    for (ITypeBinding iface : t.getInterfaces()) if (Set.of("org.springframework.context.annotation.ImportSelector", "org.springframework.context.annotation.DeferredImportSelector", "org.springframework.context.annotation.ImportBeanDefinitionRegistrar").contains(name(iface))) incomplete = true;
                    for (IAnnotationBinding a : as) {
                        String n = name(a.getAnnotationType());
                        if (n.equals("org.springframework.context.annotation.Import")) for (Object v : values(value(a, "value"))) if (v instanceof ITypeBinding imported) {
                            type(imported, imported.getJavaElement() instanceof IType it && it.isBinary() ? "dependency" : "source"); this.imported.add(name(imported)); register(name(imported), state);
                        }
                        if (n.equals("org.springframework.context.annotation.ComponentScan") || n.equals("org.springframework.boot.autoconfigure.SpringBootApplication")) {
                            int scanState = state;
                            if ((!values(value(a, "excludeFilters")).isEmpty() && !has(as, "org.springframework.boot.autoconfigure.SpringBootApplication")) || !values(value(a, "includeFilters")).isEmpty() || Boolean.FALSE.equals(value(a, "useDefaultFilters"))) scanState = 1;
                            if (n.endsWith("ComponentScan") && has(as, "org.springframework.boot.autoconfigure.SpringBootApplication") && as.stream().anyMatch(x -> name(x.getAnnotationType()).endsWith("SpringBootApplication") && (!strings(x, "scanBasePackages").isEmpty() || !values(value(x, "scanBasePackageClasses")).isEmpty()))) continue;
                            List<String> packages = strings(a, "basePackages", "scanBasePackages", "value");
                            for (String key : List.of("basePackageClasses", "scanBasePackageClasses")) for (Object v : values(value(a, key))) if (v instanceof ITypeBinding b) packages.add(b.getPackage().getName());
                            if (packages.isEmpty()) packages.add(t.getPackage().getName());
                            for (String pkg : packages) {
                                if (pkg.isEmpty() || pkg.contains("${")) { incomplete = true; continue; }
                                scans.merge(pkg, scanState, Math::max);
                            }
                        }
                    }
                    for (ITypeBinding nested : t.getDeclaredTypes()) if (has(annotations(nested.getAnnotations()), "org.springframework.context.annotation.Configuration")) { imported.add(name(nested)); register(name(nested), state); }
                }
                for (var scan : new ArrayList<>(scans.entrySet())) {
                    if (!scanned.add(scan.getKey() + ":" + scan.getValue())) continue;
                    for (IPackageFragmentRoot r : binaries) for (IJavaElement child : r.getChildren()) {
                        check(monitor);
                        if (!(child instanceof IPackageFragment pkg) || !(pkg.getElementName().equals(scan.getKey()) || pkg.getElementName().startsWith(scan.getKey() + "."))) continue;
                        for (IOrdinaryClassFile f : pkg.getOrdinaryClassFiles()) {
                            if (visited >= MAX_TYPES) { incomplete = true; break; }
                            visited++;
                            resolve(f.getType().getFullyQualifiedName('.'));
                        }
                    }
                }
                for (var t : new ArrayList<>(types.entrySet())) {
                    String pkg = t.getValue().getPackage().getName();
                    if (!has(annotations(t.getValue().getAnnotations()), "org.springframework.stereotype.Component")) continue;
                    for (var scan : scans.entrySet()) if (pkg.equals(scan.getKey()) || pkg.startsWith(scan.getKey() + ".")) {
                        int before = states.getOrDefault(t.getKey(), 0); register(t.getKey(), scan.getValue()); changed |= states.getOrDefault(t.getKey(), 0) > before;
                    }
                }
                if (!changed || processed.size() > MAX_TYPES * 2) break;
            }
            List<Map<String, Object>> beans = new ArrayList<>();
            for (var e : states.entrySet()) {
                check(monitor);
                ITypeBinding t = types.get(e.getKey());
                if (t == null || e.getValue() == 0 || t.isInterface() || Modifier.isAbstract(t.getModifiers())) continue;
                List<IAnnotationBinding> as = annotations(t.getAnnotations());
                Set<String> hierarchy = new HashSet<>(); supers(t, hierarchy, 0);
                boolean dynamic = hierarchy.stream().anyMatch(n -> Set.of("org.springframework.context.annotation.ImportSelector", "org.springframework.context.annotation.DeferredImportSelector", "org.springframework.context.annotation.ImportBeanDefinitionRegistrar").contains(n));
                if (dynamic) { incomplete = true; continue; }
                if (has(as, "org.springframework.stereotype.Component") || e.getKey().equals(root) || imported.contains(e.getKey())) bean(beans, t, t.getJavaElement(), as, imported.contains(e.getKey()) ? t.getBinaryName() : decap(t.getName()), "component", e.getValue(), t);
                for (IMethodBinding method : methods(t)) {
                    List<IAnnotationBinding> mas = annotations(method.getAnnotations());
                    if (has(mas, "org.springframework.context.annotation.Bean")) bean(beans, method.getReturnType(), method.getJavaElement(), mas, method.getName(), "factory", Math.min(e.getValue(), condition(mas)), t);
                }
            }
            return Map.of("beans", beans, "injections", injections, "incomplete", incomplete, "root", root, "revision", REVISION.get());
        }
        List<IMethodBinding> methods(ITypeBinding owner) {
            List<IMethodBinding> result = new ArrayList<>();
            Set<String> signatures = new HashSet<>(), seen = new HashSet<>();
            Deque<ITypeBinding> todo = new ArrayDeque<>(); todo.add(owner);
            while (!todo.isEmpty() && seen.size() < 64 && result.size() < 2048) {
                check(monitor); ITypeBinding t = todo.remove();
                if (!seen.add(name(t))) continue;
                for (IMethodBinding m : t.getDeclaredMethods()) {
                    String signature = m.getName() + Arrays.toString(Arrays.stream(m.getParameterTypes()).map(BeansCommand::name).toArray());
                    if (signatures.add(signature)) result.add(m);
                }
                if (t.getSuperclass() != null) todo.add(t.getSuperclass());
                todo.addAll(Arrays.asList(t.getInterfaces()));
            }
            if (!todo.isEmpty()) incomplete = true;
            return result;
        }
        void bean(List<Map<String, Object>> out, ITypeBinding ty, IJavaElement element, List<IAnnotationBinding> as, String fallback, String kind, int state, ITypeBinding owner) throws JavaModelException {
            if (state == 0) return;
            if (out.size() >= MAX_BEANS || ty == null || ty.isRecovered()) { incomplete = true; return; }
            Location location = location(element);
            if (location == null) { incomplete = true; return; }
            List<String> names = new ArrayList<>(), qualifiers = new ArrayList<>();
            for (IAnnotationBinding a : as) {
                String n = name(a.getAnnotationType());
                if (n.equals("org.springframework.context.annotation.Bean")) names.addAll(strings(a, "name", "value"));
                else if (n.equals("org.springframework.beans.factory.annotation.Qualifier")) qualifiers.addAll(strings(a, "value"));
            }
            if (names.isEmpty()) {
                for (IAnnotationBinding a : as) if (name(a.getAnnotationType()).startsWith(SPRING + "stereotype.")) names.addAll(strings(a, "value"));
            }
            if (names.isEmpty()) names.add(fallback);
            Map<String, Object> record = new LinkedHashMap<>();
            record.put("name", names.get(0)); record.put("aliases", names.subList(1, names.size())); record.put("qualifiers", qualifiers);
            record.put("type", name(ty)); record.put("generic", ty.isParameterizedType() || ty.isGenericType() || ty.isTypeVariable());
            record.put("dependencies", element instanceof IMethod method ? Arrays.stream(method.getParameterTypes()).map(signature -> Signature.toString(Signature.getTypeErasure(signature))).toList() : List.of());
            record.put("primary", has(as, "org.springframework.context.annotation.Primary"));
            record.put("state", state == 2 ? "active" : "unknown"); record.put("kind", kind);
            record.put("location", location); record.put("owner", name(owner)); record.put("origin", element instanceof IMember member && member.isBinary() ? "dependency" : origins.getOrDefault(name(owner), "source"));
            Set<String> supers = new LinkedHashSet<>(); supers(ty, supers, 0); record.put("supers", supers);
            out.add(record);
        }
        void supers(ITypeBinding t, Set<String> out, int depth) {
            check(monitor);
            if (t == null || depth >= 24 || out.size() >= 256 || !out.add(name(t))) return;
            supers(t.getSuperclass(), out, depth + 1); for (ITypeBinding i : t.getInterfaces()) supers(i, out, depth + 1);
        }
    }
}
