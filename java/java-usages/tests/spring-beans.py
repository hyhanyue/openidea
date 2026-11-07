"""Real Maven/JDT regression for dependency registrations; consumes verify.py's oracle output.
Usage: python3 spring-beans.py /path/to/jdtls /path/to/oracle-output
Run with Java 25 on PATH; application fixture bytecode is Java 8.
"""
import json, os, pathlib, platform, queue, shutil, subprocess, sys, threading, time, zipfile
home = pathlib.Path(sys.argv[1]).resolve()
work = pathlib.Path(sys.argv[2]).resolve() / 'jdt'
work.mkdir(parents=True, exist_ok=True)
project = pathlib.Path(sys.argv[2]).resolve() / 'project'
# The independent library must remain binary, not be imported as a third source module.
shutil.rmtree(project / 'library', ignore_errors=True)
bundle = pathlib.Path(__file__).resolve().parents[1] / 'openidea-java-usages.jar'
config = work / 'configuration'
if not config.exists(): shutil.copytree(home / ('config_mac_arm' if platform.machine() == 'arm64' else 'config_mac'), config)
file = project / 'app/src/main/java/demo/app/App.java'
source = file.read_text()
repo = project.parents[1] / 'maven-repository'
settings = work / 'settings.xml'
settings.write_text('<settings><localRepository>' + str(repo) + '</localRepository><offline>true</offline></settings>')
log = (work / 'stderr.log').open('w')
proc = subprocess.Popen(['java', '-Dopenidea.model.only=true', '-Declipse.application=org.eclipse.jdt.ls.core.id1', '-Dosgi.bundles.defaultStartLevel=4', '-Declipse.product=org.eclipse.jdt.ls.core.product', '-Xmx1G', '--add-modules=ALL-SYSTEM', '--add-opens', 'java.base/java.util=ALL-UNNAMED', '--add-opens', 'java.base/java.lang=ALL-UNNAMED', '-jar', str(next((home / 'plugins').glob('org.eclipse.equinox.launcher_*.jar'))), '-configuration', str(config), '-data', str(work / 'metadata')], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log)
messages = queue.Queue()
lock = threading.Lock()
def send(message):
    data = json.dumps(message).encode()
    with lock:
        proc.stdin.write(f'Content-Length: {len(data)}\r\n\r\n'.encode() + data)
        proc.stdin.flush()
def read():
    while True:
        headers = {}
        while True:
            line = proc.stdout.readline()
            if not line: return
            if line == b'\r\n': break
            k, v = line.decode().split(':', 1)
            headers[k.lower()] = v.strip()
        message = json.loads(proc.stdout.read(int(headers['content-length'])))
        if 'method' in message and 'id' in message:
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': [] if message['method'] == 'workspace/configuration' else None})
        else: messages.put(message)
threading.Thread(target=read, daemon=True).start()
sequence = 0
transcript = []
def request(method, params):
    global sequence
    sequence += 1
    send({'jsonrpc': '2.0', 'id': sequence, 'method': method, 'params': params})
    deadline = time.monotonic() + 180
    while time.monotonic() < deadline:
        msg = messages.get(timeout=max(1, deadline - time.monotonic()))
        transcript.append(msg)
        if msg.get('id') == sequence:
            if 'error' in msg: raise RuntimeError(msg['error'])
            return msg.get('result')
    raise TimeoutError(method)

def query(context=''):
    return request('workspace/executeCommand', {'command':'openidea.java.spring.beans','arguments':[file.as_uri(),context]})
try:
    init = request('initialize', {'processId':None, 'rootUri':project.as_uri(), 'workspaceFolders':[{'uri':project.as_uri(),'name':'dependency-fixture'}],
        'capabilities': {'textDocument':{'definition':{'linkSupport':False}}},
        'initializationOptions': {'bundles':[str(bundle)], 'extendedClientCapabilities':{'classFileContentsSupport':True},
          'settings': {'java': {'configuration': {'maven': {'userSettings':str(settings)}}, 'import': {'gradle':{'enabled':False},'maven':{'enabled':True,'offline':{'enabled':True}}}}}}})
    assert 'openidea.java.spring.beans' in init['capabilities']['executeCommandProvider']['commands'], init
    send({'jsonrpc':'2.0','method':'initialized','params':{}})
    send({'jsonrpc':'2.0','method':'textDocument/didOpen','params':{'textDocument':{'uri':file.as_uri(),'languageId':'java','version':1,'text':source}}})
    request('java/buildWorkspace', True)
    deadline=time.monotonic()+30
    while True:
        result=query()
        (work/'last-model.json').write_text(json.dumps(result,indent=2))
        if any(b['name']=='libraryPort' for b in result['beans']): break
        if time.monotonic()>deadline: raise AssertionError({k:result.get(k) for k in ['root','incomplete']} | {'beans':[b['name'] for b in result['beans']]})
        time.sleep(.3)
    (work/'model.json').write_text(json.dumps(result,indent=2))
    byname={b['name']:b for b in result['beans']}
    for n in ['libraryPort','token','scannedLibrary','autoMessage','legacyNumber']:
        assert n in byname, (n, byname.keys())
        assert byname[n]['state']=='active', byname[n]
        assert byname[n]['origin']=='dependency', byname[n]
        assert byname[n]['location']['uri'].startswith('jdt:'), byname[n]
    assert byname['moduleValue']['origin']=='source', byname['moduleValue']
    assert byname['moduleValue']['type']=='demo.api.ModuleValue', byname['moduleValue']
    assert byname['libraryPort']['type']=='demo.api.Port', byname['libraryPort']
    assert byname['libraryPort']['aliases']==['portAlias'], byname['libraryPort']
    assert byname['conditionalNumber']['state']=='unknown', byname['conditionalNumber']
    assert not {'unscanned','absentNumber'} & byname.keys(), byname.keys()
    assert any(i['type']=='demo.api.Port' for i in result['injections']), result['injections']
    assert any(i['type']=='demo.api.Token' for i in result['injections']), result['injections']
    contents=request('java/classFileContents',{'uri':byname['libraryPort']['location']['uri']})
    assert 'LibraryConfig' in contents and 'libraryPort' in contents, contents
    attached = request('workspace/executeCommand', {'command':'java.project.updateSourceAttachment','arguments':[{'classFileUri':byname['libraryPort']['location']['uri'],'attributes':{'jarPath':str(project/'lib/vendor.jar'),'sourceAttachmentPath':str(project/'lib/vendor-sources.jar'),'sourceAttachmentEncoding':'UTF-8'}}]})
    assert not attached.get('errorMessage'), attached
    deadline = time.monotonic() + 10
    while True:
        with_sources = query()
        source_factory = next(b for b in with_sources['beans'] if b['name']=='libraryPort')
        if source_factory['location']['range']['start']['line'] > 0: break
        if time.monotonic() > deadline: raise AssertionError(source_factory)
        time.sleep(.1)
    (work/'with-sources.json').write_text(json.dumps(with_sources,indent=2))
    # Metadata changes at the same authorized classpath path invalidate the snapshot.
    archive = project/'lib/vendor.jar'
    original = archive.read_bytes()
    with zipfile.ZipFile(archive) as z: entries = {n:z.read(n) for n in z.namelist()}
    try:
        entries['META-INF/spring.factories'] = b''
        entries['META-INF/spring/org.springframework.boot.autoconfigure.AutoConfiguration.imports'] = b'x' * (1024*1024+1)
        with zipfile.ZipFile(archive,'w') as z:
            for n,data in entries.items(): z.writestr(n,data)
        limited = query()
        assert limited['incomplete'], limited
        assert not any(b['name'] in {'legacyNumber','autoMessage'} for b in limited['beans']), [b['name'] for b in limited['beans']]
    finally: archive.write_bytes(original)
    assert any(b['name']=='autoMessage' for b in query()['beans'])
    # An alternate selected root is unavailable, never an empty resolved context.
    assert query('missing.Other')['incomplete']
    # Unsaved source imports invalidate the cache. A library does not become scanned just by existing.
    changed=source.replace('vendor.LibraryConfig.class','vendor.Unscanned.class')
    send({'jsonrpc':'2.0','method':'textDocument/didChange','params':{'textDocument':{'uri':file.as_uri(),'version':2},'contentChanges':[{'text':changed}]}})
    after=query()
    assert not any(b['name']=='libraryPort' for b in after['beans']), after
    assert any(b['type']=='vendor.Unscanned' for b in after['beans']), after
    # Cancellation reaches JDT's model traversal and lock wait through the request monitor.
    send({'jsonrpc':'2.0','method':'textDocument/didClose','params':{'textDocument':{'uri':file.as_uri()}}})
    restored=query()
    assert any(b['name']=='libraryPort' for b in restored['beans']), restored
    print(json.dumps({'status':'JDT_MAVEN_DEPENDENCIES_OK','beans':len(result['beans']),'incomplete':result['incomplete'],'types':[byname[n]['type'] for n in ['libraryPort','token']],'evidence':str(work)},indent=2))
finally:
    (work/'transcript.json').write_text(json.dumps(transcript,indent=2))
    proc.terminate(); proc.wait(timeout=20)
