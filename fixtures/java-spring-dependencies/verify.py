"""Java 8 container oracle and independent dependency JAR; no server or TCP port.
Usage: python3 verify.py [isolated-output-directory]
Reads cached Spring jars only. The Maven parent/reactor is also importable in the IDE.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

fixture = Path(__file__).resolve().parent
work = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else fixture / 'target' / 'oracle'
work.mkdir(parents=True, exist_ok=True)
java = Path(os.environ.get('JAVA8_HOME', '/Library/Java/JavaVirtualMachines/jdk-1.8.jdk/Contents/Home')) / 'bin'
repo = Path(os.environ.get('MAVEN_CACHE', str(Path.home() / '.m2/repository')))
jars = [repo / 'org/springframework' / artifact / '5.3.31' / (artifact + '-5.3.31.jar')
        for artifact in ['spring-context', 'spring-core', 'spring-beans', 'spring-aop', 'spring-expression', 'spring-jcl']]
jars += [repo / 'org/springframework/boot' / artifact / '2.7.18' / (artifact + '-2.7.18.jar')
         for artifact in ['spring-boot', 'spring-boot-autoconfigure']]
assert all(p.is_file() for p in jars), jars
project = work / 'project'
if project.exists(): shutil.rmtree(project)
shutil.copytree(fixture, project, ignore=shutil.ignore_patterns('target', '__pycache__', 'lib'))

def compile(source, output, classpath):
    output.mkdir(parents=True, exist_ok=True)
    sources = list(source.rglob('*.java'))
    subprocess.run([str(java / 'javac'), '-source', '8', '-target', '8', '-parameters', '-cp', os.pathsep.join(map(str, classpath)), '-d', str(output), *map(str, sources)], check=True)

api = project / 'api/target/classes'
compile(project / 'api/src/main/java', api, jars)
lib = work / 'library-classes'
compile(project / 'library/src/main/java', lib, [api, *jars])
archive = project / 'lib/vendor.jar'
archive.parent.mkdir()
with zipfile.ZipFile(archive, 'w') as z:
    for d in sorted(p for p in lib.rglob('*') if p.is_dir()): z.writestr(str(d.relative_to(lib)) + '/', '')
    for f in sorted(lib.rglob('*.class')):
        assert int.from_bytes(f.read_bytes()[6:8], 'big') == 52, f
        z.write(f, str(f.relative_to(lib)))
    for f in (project / 'library/src/main/resources').rglob('*'):
        if f.is_file(): z.write(f, str(f.relative_to(project / 'library/src/main/resources')))
with zipfile.ZipFile(project / 'lib/vendor-sources.jar', 'w') as z:
    for f in (project / 'library/src/main/java').rglob('*.java'): z.write(f, str(f.relative_to(project / 'library/src/main/java')))
app = project / 'app/target/classes'
compile(project / 'app/src', app, [api, archive, *jars])
cp = [app, api, archive, *jars]
env = {k:v for k,v in os.environ.items() if k not in {'LOG_FORMAT', 'LOG_LEVEL'}}
result = subprocess.run([str(java / 'java'), '-cp', os.pathsep.join(map(str, cp)), 'demo.app.ContainerCheck'], capture_output=True, text=True, env=env)
assert 'JAVA8_CONTAINER_OK' in result.stdout, result.stdout + result.stderr
(work / 'container.txt').write_text(result.stdout + result.stderr)
(work / 'classpath.json').write_text(json.dumps([str(p) for p in jars]))
print(result.stdout)
print('Evidence:', work)
