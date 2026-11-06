"""Build the embedded Maven extension with JDK 21 and Apache Maven 3.9.11.

Usage: python3 java/maven-test-scope/build.py /path/to/maven
Only OpenIDEA classes and the Plexus descriptor go into the JAR.
"""
import pathlib
import subprocess
import sys
import tempfile
import zipfile

root = pathlib.Path(__file__).resolve().parent
maven = pathlib.Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory() as output:
    subprocess.run([
        'javac', '--release', '8', '-g:none', '-proc:none', '-classpath', str(maven / 'lib/*'),
        '-d', output, str(root / 'TestScope.java'),
    ], check=True)
    classes = pathlib.Path(output)
    files = {'META-INF/plexus/components.xml': root / 'components.xml'}
    files.update({p.relative_to(classes).as_posix(): p for p in classes.rglob('*.class')})
    with zipfile.ZipFile(root / 'openidea-maven-test-scope.jar', 'w') as jar:
        for name, path in sorted(files.items()):
            jar.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), path.read_bytes())
