#![cfg(unix)]
//! Opt-in real Boot 2.7.18 / Java 8 process regression; no dependency downloads here.
use ide_core::{IdeEvent, RunId, SharedSink};
use ide_run::{OutputChunk, ProcessManager, RunConfig, StartOptions};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

type Output = Arc<Mutex<String>>;
struct Harness {
    manager: ProcessManager,
    sink: SharedSink,
    events: mpsc::Receiver<IdeEvent>,
    root: tempfile::TempDir,
}
impl Harness {
    fn new() -> Self {
        let (tx, events) = mpsc::channel();
        let tx = Mutex::new(tx);
        Self {
            manager: ProcessManager::new(),
            sink: Arc::new(move |e| {
                let _ = tx.lock().send(e);
            }),
            events,
            root: tempfile::tempdir().unwrap(),
        }
    }
    fn start(&self, config: &RunConfig) -> (RunId, Output) {
        let output: Output = Arc::default();
        let capture = output.clone();
        let info = self
            .manager
            .start(
                self.root.path(),
                config,
                &StartOptions::default(),
                self.sink.clone(),
                Box::new(move |chunk| {
                    if let OutputChunk::Stdout { text } | OutputChunk::Stderr { text } = chunk {
                        capture.lock().push_str(&text);
                    }
                }),
            )
            .unwrap();
        (info.id, output)
    }
    fn exited(&self, id: RunId) -> Option<i32> {
        let end = Instant::now() + Duration::from_secs(40);
        loop {
            let event = self.events.recv_timeout(end.saturating_duration_since(Instant::now())).expect("process exit");
            if let IdeEvent::RunExited { run, code } = event
                && run == id
            {
                return code;
            }
        }
    }
    fn stop(&self, id: RunId) {
        self.manager.stop(id).unwrap();
        self.exited(id);
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.manager.kill_all();
    }
}

fn wait_text(output: &Output, needle: &str) -> String {
    let end = Instant::now() + Duration::from_secs(40);
    loop {
        let text = output.lock().clone();
        if text.contains(needle) {
            return text;
        }
        assert!(Instant::now() < end, "startup milestone not observed: {needle}");
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn http_port(output: &Output) -> u16 {
    let text = wait_text(output, "Started DemoApplication in");
    text.split("Tomcat started on port(s): ")
        .nth(1)
        .expect("embedded HTTP port")
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
fn probe(port: u16) -> Value {
    let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    socket.write_all(b"GET /launch-probe HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap()
}
fn config(h: &Harness, profile: &str) -> RunConfig {
    let fixture = PathBuf::from(std::env::var_os("OPENIDEA_JAVA8_FIXTURE").expect("set OPENIDEA_JAVA8_FIXTURE"));
    let java = PathBuf::from(std::env::var_os("JAVA_HOME").expect("set Java 8 JAVA_HOME")).join("bin/java");
    let cp = format!(
        "{}:{}",
        fixture.join("target/classes").display(),
        std::fs::read_to_string(fixture.join("target/classpath.txt")).expect("build fixture classpath").trim()
    );
    let cwd = h.root.path().join(format!("working {profile}"));
    std::fs::create_dir_all(&cwd).unwrap();
    let mut config = RunConfig {
        id: format!("user:{profile}"),
        name: profile.into(),
        kind: "spring-boot".into(),
        command: java.to_string_lossy().into_owned(),
        args: vec![
            "-Dprobe.vm=vm-marker".into(),
            "-cp".into(),
            cp,
            "com.example.demo.DemoApplication".into(),
            "--server.port=0".into(),
            "--server.address=127.0.0.1".into(),
            format!("--spring.profiles.active={profile}"),
            "--probe.arg=argument with spaces".into(),
        ],
        cwd: Some(cwd.into()),
        env: BTreeMap::from([("OPENIDEA_PROBE".into(), format!("configured-{profile}"))]),
        detected: false,
        is_test: false,
        debug_adapter: Some("java-debug".into()),
        allow_parallel: false,
        extras: Default::default(),
    };
    std::fs::write(h.root.path().join("fixture.env"), "OPENIDEA_PROBE=file-value\nFILE_PROBE=file-marker\n").unwrap();
    config.extras.env_files = vec!["fixture.env".into()];
    config.extras.include_system_env = false;
    config.extras.module = Some("app".into());
    config.extras.params = BTreeMap::from([
        ("activeProfiles".into(), profile.into()),
        ("vmOptions".into(), "-Dprobe.vm=vm-marker".into()),
        ("programArgs".into(), "--probe.arg='argument with spaces'".into()),
    ]);
    config
}
fn resume_jdwp(output: &Output) {
    let text = wait_text(output, "Listening for transport dt_socket at address:");
    let port: u16 = text
        .split("Listening for transport dt_socket at address:")
        .nth(1)
        .unwrap()
        .trim()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    socket.write_all(b"JDWP-Handshake").unwrap();
    let mut handshake = [0u8; 14];
    socket.read_exact(&mut handshake).unwrap();
    assert_eq!(&handshake, b"JDWP-Handshake");
    socket.write_all(&[0, 0, 0, 11, 0, 0, 0, 1, 0, 1, 9]).unwrap(); // VirtualMachine.Resume
    loop {
        let mut header = [0u8; 11];
        socket.read_exact(&mut header).unwrap();
        let len = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        assert!((11..=1_048_576).contains(&len));
        let mut body = vec![0; len - 11];
        socket.read_exact(&mut body).unwrap();
        if header[8] == 0x80 && header[4..8] == [0, 0, 0, 1] {
            assert_eq!(&header[9..11], &[0, 0]);
            break;
        }
    }
}

#[test]
#[ignore = "requires built Java 8 Boot fixture and permission to bind loopback sockets"]
fn java8_boot_profiles_environment_debug_stop_restart_and_failures() {
    let h = Harness::new();
    let dev = config(&h, "dev");
    let mut prod = config(&h, "prod");
    prod.extras.local = true;
    ide_run::save_user_configs(h.root.path(), &[dev.clone(), prod.clone()]).unwrap();
    let configs = ide_run::load_user_configs(h.root.path()).unwrap();
    assert_eq!(configs, vec![dev, prod]);
    let (dev_id, dev_out) = h.start(&configs[0]);
    let dev_port = http_port(&dev_out);
    let actual = probe(dev_port);
    assert_eq!(actual["profiles"], serde_json::json!(["dev"]));
    assert_eq!(actual["env"], "configured-dev");
    assert_eq!(actual["file"], "file-marker");
    assert_eq!(actual["vm"], "vm-marker");
    assert_eq!(actual["arg"], "argument with spaces");
    assert_eq!(actual["java"], "1.8");
    assert_eq!(actual["inheritedHome"], false);
    assert_eq!(actual["cwd"], configs[0].cwd.as_ref().unwrap().canonicalize().unwrap().to_string_lossy().as_ref());
    let mut debug = configs[1].clone();
    debug.args.insert(0, "-agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=127.0.0.1:0".into());
    let (prod_id, prod_out) = h.start(&debug);
    wait_text(&prod_out, "Listening for transport");
    assert!(!prod_out.lock().contains("Started DemoApplication"));
    resume_jdwp(&prod_out);
    let prod_port = http_port(&prod_out);
    let actual = probe(prod_port);
    assert_eq!(actual["profiles"], serde_json::json!(["prod"]));
    assert_eq!(actual["env"], "configured-prod");
    assert_eq!(actual["java"], "1.8");
    h.stop(dev_id);
    assert_eq!(probe(prod_port)["profiles"], serde_json::json!(["prod"]));
    let (restart_id, restart_out) = h.start(&configs[0]);
    assert_eq!(probe(http_port(&restart_out))["profiles"], serde_json::json!(["dev"]));
    h.stop(restart_id);
    h.stop(prod_id);
    for (profile, needle) in [
        ("missing", "required a bean of type"),
        ("ambiguous", "required a single bean, but 2 were found"),
        ("binding", "Failed to bind properties under"),
    ] {
        let failed = config(&h, profile);
        let (id, output) = h.start(&failed);
        assert_ne!(h.exited(id), Some(0));
        assert!(output.lock().contains(needle), "expected {profile} failure classification");
        if profile == "binding" {
            assert!(output.lock().contains("class path resource [application-binding.properties] - 1:"));
        }
    }
    // Exercise the actual Maven Boot plugin with the frontend's independent fork options.
    if let Some(maven) = std::env::var_os("OPENIDEA_MAVEN") {
        let fixture = PathBuf::from(std::env::var_os("OPENIDEA_JAVA8_FIXTURE").unwrap());
        let work = h.root.path().join("maven application work");
        std::fs::create_dir_all(&work).unwrap();
        let mut maven_config = configs[0].clone();
        maven_config.id = "user:maven".into();
        maven_config.kind = "maven".into();
        maven_config.command = maven.to_string_lossy().into_owned();
        maven_config.cwd = Some(fixture.into());
        maven_config.env.insert("JAVA_HOME".into(), std::env::var("JAVA_HOME").unwrap());
        maven_config.args = vec!["-o".into(), "spring-boot:run".into(),
            "-Dspring-boot.run.profiles=dev".into(),
            "-Dspring-boot.run.jvmArguments=-Dprobe.vm=maven-vm".into(),
            r#"-Dspring-boot.run.arguments=--server.port=0 --server.address=127.0.0.1 '--probe.arg=It'"'"'s a "quoted" value'"#.into(),
            format!("-Dspring-boot.run.workingDirectory={}", work.display()),
        ];
        for debug in [false, true] {
            let mut launch = maven_config.clone();
            if debug {
                launch
                    .args
                    .iter_mut()
                    .find(|arg| arg.starts_with("-Dspring-boot.run.jvmArguments="))
                    .unwrap()
                    .push_str(" -agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=127.0.0.1:0");
            }
            let (id, output) = h.start(&launch);
            if debug {
                resume_jdwp(&output);
            }
            let actual = probe(http_port(&output));
            assert_eq!(actual["profiles"], serde_json::json!(["dev"]));
            assert_eq!(actual["vm"], "maven-vm");
            assert_eq!(actual["arg"], r#"It's a "quoted" value"#);
            assert_eq!(actual["env"], "configured-dev");
            assert_eq!(actual["java"], "1.8");
            assert_eq!(actual["cwd"], work.canonicalize().unwrap().to_string_lossy().as_ref());
            h.stop(id);
        }
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut occupied = configs[0].clone();
    occupied.args.retain(|arg| !arg.starts_with("--server.port="));
    occupied.args.push(format!("--server.port={port}"));
    let (id, output) = h.start(&occupied);
    assert_ne!(h.exited(id), Some(0));
    assert!(output.lock().contains("was already in use"));
    assert!(listener.local_addr().is_ok());
}
