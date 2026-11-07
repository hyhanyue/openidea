//! Explicit, read-only loopback Actuator access. Raw configuration never crosses IPC.
use ide_core::{CancellationToken, IdeError, IdeResult};
use serde_json::{Value, json};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

const ENDPOINTS: [&str; 5] = ["beans", "conditions", "env", "configprops", "mappings"];
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_ROWS: usize = 20_000;

fn base_url(input: &str) -> IdeResult<reqwest::Url> {
    let mut url = reqwest::Url::parse(input).map_err(|_| IdeError::invalid("Invalid runtime URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.host_str().is_some_and(|host| {
            host == "localhost" || host.trim_matches(['[', ']']).parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
        })
    {
        return Err(IdeError::invalid("Only loopback HTTP URLs without credentials, query or fragment are allowed"));
    }
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn entries(v: &Value) -> impl Iterator<Item = (&String, &Value)> {
    v.as_object().into_iter().flatten()
}
fn strings(v: &Value) -> Vec<&str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}
fn push(rows: &mut Vec<Value>, row: Value) {
    if rows.len() < MAX_ROWS {
        rows.push(row);
    }
}

fn metadata_name(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || "._-$".contains(c))
}

fn local_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains("://")
        && !value.contains("${")
        && value.chars().all(|c| !c.is_control() && !"@?#=\"'".contains(c))
}

fn local_resource(value: &str) -> bool {
    ["class path resource [", "file ["]
        .iter()
        .any(|prefix| value.strip_prefix(prefix).and_then(|path| path.strip_suffix(']')).is_some_and(local_path))
}

fn source_name(value: &str, index: usize) -> String {
    if matches!(
        value,
        "systemProperties"
            | "systemEnvironment"
            | "commandLineArgs"
            | "server.ports"
            | "random"
            | "defaultProperties"
            | "spring.application.json"
            | "servletContextInitParams"
            | "servletConfigInitParams"
    ) {
        return value.into();
    }
    if let Some(url) = value.strip_prefix("configserver:")
        && let Ok(url) = reqwest::Url::parse(url)
        && matches!(url.scheme(), "http" | "https")
    {
        return format!("configserver:{}", url.origin().ascii_serialization());
    }
    if let Some(resource) = value.strip_prefix("Config resource '")
        && let Some((resource, location)) = resource.split_once("' via location '")
        && local_resource(resource)
    {
        let location = location.strip_suffix('\'').unwrap_or("");
        let location = location.strip_prefix("optional:").unwrap_or(location);
        if ["classpath:", "file:"].iter().any(|prefix| location.strip_prefix(prefix).is_some_and(local_path)) {
            return value.into();
        }
        return resource.into();
    }
    if let Some(path) = value.strip_prefix("applicationConfig: [").and_then(|path| path.strip_suffix(']'))
        && ["classpath:", "file:"].iter().any(|prefix| path.strip_prefix(prefix).is_some_and(local_path))
    {
        return value.into();
    }
    format!("#{index}")
}

fn keys(rows: &mut Vec<Value>, value: &Value, prefix: &str, context: &str, source: &str) {
    if rows.len() >= MAX_ROWS {
        return;
    }
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                keys(
                    rows,
                    child,
                    &if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") },
                    context,
                    source,
                );
            }
        }
        Value::Array(values) if !values.is_empty() => {
            for (i, child) in values.iter().enumerate() {
                keys(rows, child, &format!("{prefix}[{i}]"), context, source);
            }
        }
        _ => push(rows, json!({"name":prefix,"context":context,"detail":source,"masked":true})),
    }
}

/// Strict allowlist projection: condition messages and every configuration value are omitted.
/// Unknown shapes stay incompatible rather than masquerading as an empty application.
pub fn project(endpoint: &str, data: &Value) -> Value {
    let mut rows = Vec::new();
    let mut recognized = false;
    if endpoint == "env" {
        if let Some(sources) = data["propertySources"].as_array() {
            recognized = true;
            for (index, source) in sources.iter().enumerate() {
                let name = source_name(s(&source["name"]), index + 1);
                for (key, _) in entries(&source["properties"]) {
                    push(&mut rows, json!({"name":key,"context":"","detail":name,"masked":true}));
                }
            }
        }
    } else if let Some(contexts) = data["contexts"].as_object() {
        for (context, value) in contexts {
            match endpoint {
                "beans" => {
                    if value["beans"].is_object() {
                        recognized = true;
                        for (name, bean) in entries(&value["beans"]) {
                            push(
                                &mut rows,
                                json!({"name":name,"context":context,"detail":s(&bean["scope"]),"className":s(&bean["type"]),"dependencies":strings(&bean["dependencies"])}),
                            );
                        }
                    }
                }
                "conditions" => {
                    for (field, outcome) in [("positiveMatches", "matched"), ("negativeMatches", "notMatched")] {
                        if value[field].is_object() {
                            recognized = true;
                        }
                        for (index, (name, conditions)) in entries(&value[field]).enumerate() {
                            let source = if name.split('#').all(metadata_name) {
                                name.clone()
                            } else {
                                format!("#{}", index + 1)
                            };
                            let conditions = if conditions.is_array() { conditions } else { &conditions["notMatched"] };
                            let names: Vec<_> = conditions
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|c| c["condition"].as_str())
                                .filter(|name| metadata_name(name))
                                .collect();
                            push(
                                &mut rows,
                                json!({"name":source,"context":context,"detail":names.join(", "),"outcome":outcome}),
                            );
                        }
                    }
                    for (index, name) in strings(&value["unconditionalClasses"]).into_iter().enumerate() {
                        let source = if metadata_name(name) { name.to_owned() } else { format!("#{}", index + 1) };
                        push(&mut rows, json!({"name":source,"context":context,"detail":"","outcome":"unconditional"}));
                    }
                }
                "configprops" => {
                    if value["beans"].is_object() {
                        recognized = true;
                        for (index, (name, bean)) in entries(&value["beans"]).enumerate() {
                            let source = if metadata_name(name) { name.clone() } else { format!("#{}", index + 1) };
                            let prefix = s(&bean["prefix"]);
                            let prefix = if metadata_name(prefix) { prefix } else { "" };
                            keys(&mut rows, &bean["properties"], prefix, context, &source);
                        }
                    }
                }
                "mappings" => {
                    for field in ["dispatcherServlets", "dispatcherHandlers"] {
                        if value["mappings"][field].is_object() {
                            recognized = true;
                        }
                        for (dispatcher, mappings) in entries(&value["mappings"][field]) {
                            for mapping in mappings.as_array().into_iter().flatten() {
                                let method = &mapping["details"]["handlerMethod"];
                                let conditions = &mapping["details"]["requestMappingConditions"];
                                push(
                                    &mut rows,
                                    json!({"name":format!("{} {}", strings(&conditions["methods"]).join(", "), strings(&conditions["patterns"]).join(", ")),"context":context,"detail":dispatcher,"className":s(&method["className"]),"methodName":s(&method["name"]),"descriptor":s(&method["descriptor"]),"paths":strings(&conditions["patterns"]),"methods":strings(&conditions["methods"])}),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    json!({"state":if recognized {"ok"} else {"incompatible"},"rows":rows,"limited":rows.len() >= MAX_ROWS})
}

async fn fetch(
    client: &reqwest::Client,
    url: reqwest::Url,
    endpoint: &str,
    user: &str,
    password: &str,
    cancel: &CancellationToken,
) -> Value {
    let mut request = client.get(url).header("Accept", "application/json");
    if !user.is_empty() {
        request = request.basic_auth(user, Some(password));
    }
    let result = async {
        let mut response = request.send().await.map_err(|e| if e.is_timeout() { "timeout" } else { "unreachable" })?;
        let status = response.status().as_u16();
        if status != 200 {
            return Ok(
                json!({"state": match status { 401 => "unauthorized", 403 => "forbidden", 404 => "disabled", 300..=399 => "redirect", _ => "httpError" }, "status":status,"rows":[]}),
            );
        }
        if response.content_length().is_some_and(|n| n > MAX_BYTES as u64) {
            return Err("tooLarge");
        }
        let mut body = Vec::new();
        while let Some(chunk) =
            response.chunk().await.map_err(|e| if e.is_timeout() { "timeout" } else { "invalidResponse" })?
        {
            if body.len() + chunk.len() > MAX_BYTES {
                return Err("tooLarge");
            }
            body.extend_from_slice(&chunk);
        }
        let endpoint = endpoint.to_owned();
        tokio::task::spawn_blocking(move || {
            let value: Value = serde_json::from_slice(&body).map_err(|_| "incompatible")?;
            Ok(project(&endpoint, &value))
        })
        .await
        .map_err(|_| "invalidResponse")?
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => json!({"state":"cancelled","rows":[]}),
        result = result => result.unwrap_or_else(|state: &str| json!({"state":state,"rows":[]})),
    }
}

fn runtime_client() -> IdeResult<reqwest::Client> {
    use rustls_platform_verifier::BuilderVerifierExt;
    let tls =
        rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|_| IdeError::internal("Cannot configure runtime TLS"))?
            .with_platform_verifier()
            .map_err(|_| IdeError::internal("Cannot configure runtime TLS"))?
            .with_no_client_auth();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .no_proxy()
        .resolve_to_addrs(
            "localhost",
            &[
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), 0),
            ],
        )
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| IdeError::internal("Cannot create runtime client"))
}

pub async fn snapshot(base: &str, user: &str, password: &str, cancel: &CancellationToken) -> IdeResult<Value> {
    let base = base_url(base)?;
    let client = runtime_client()?;
    let mut result = serde_json::Map::new();
    for endpoint in ENDPOINTS {
        if cancel.is_cancelled() {
            return Err(IdeError::cancelled());
        }
        let url = base.join(endpoint).map_err(|_| IdeError::invalid("Invalid runtime URL"))?;
        result.insert(endpoint.into(), fetch(&client, url, endpoint, user, password, cancel).await);
    }
    if cancel.is_cancelled() {
        return Err(IdeError::cancelled());
    }
    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn rejects_non_loopback_and_credential_urls() {
        for input in [
            "https://example.com/actuator",
            "http://127.0.0.1.example.com",
            "http://localhost.example.com",
            "file:///tmp/a",
            "http://user:pass@localhost:8080",
            "http://localhost/?token=a",
            "http://localhost/#a",
            "http://0.0.0.0",
        ] {
            assert!(base_url(input).is_err(), "{input}");
        }
        for input in ["http://localhost:19001/actuator", "http://127.0.0.2/actuator/", "http://[::1]:19001/custom"] {
            assert!(base_url(input).is_ok(), "{input}");
        }
        assert_eq!(
            base_url("http://localhost:19001/custom").unwrap().join("beans").unwrap().as_str(),
            "http://localhost:19001/custom/beans"
        );
    }

    #[test]
    fn projects_only_keys_and_condition_names() {
        let secret = "RUNTIME_TEST_VALUE_MUST_NOT_CROSS_IPC";
        let env = project(
            "env",
            &json!({"propertySources":[{"name":"fixture","properties":{"test.key":{"value":secret,"origin":secret}}}]}),
        );
        let props = project(
            "configprops",
            &json!({"contexts":{"app":{"beans":{"props":{"prefix":"demo","properties":{"nested":[{"key":secret}]},"inputs":{"value":{"value":secret}}}}}}}),
        );
        let conditions = project(
            "conditions",
            &json!({"contexts":{"app":{"positiveMatches":{"Config":[{"condition":"OnPropertyCondition","message":secret}]},"negativeMatches":{"Other":{"notMatched":[{"condition":"OnClassCondition","message":secret}]}},"unconditionalClasses":[]}}}),
        );
        for result in [&env, &props, &conditions] {
            assert_eq!(result["state"], "ok");
            assert!(!result.to_string().contains(secret));
        }
        assert_eq!(env["rows"][0]["name"], "test.key");
        assert_eq!(props["rows"][0]["name"], "demo.nested[0].key");
        assert_eq!(conditions["rows"][1]["outcome"], "notMatched");
        assert_eq!(project("beans", &json!({"error":"not beans"}))["state"], "incompatible");
        assert_eq!(project("beans", &json!({"contexts":{"app":{"beans":{}}}}))["rows"], json!([]));
    }

    #[test]
    fn supports_servlet_and_reactive_mappings_without_raw_predicates() {
        for dispatcher in ["dispatcherServlets", "dispatcherHandlers"] {
            let raw = json!({"contexts":{"app":{"mappings":{dispatcher:{"main":[{"predicate":"secret header", "details":{"handlerMethod":{"className":"com.example.App","name":"get","descriptor":"()V"},"requestMappingConditions":{"methods":["GET"],"patterns":["/demo"]}}}]}}}}});
            let result = project("mappings", &raw);
            assert_eq!(result["state"], "ok");
            assert_eq!(result["rows"][0]["className"], "com.example.App");
            assert_eq!(result["rows"][0]["name"], "GET /demo");
            assert!(!result.to_string().contains("secret header"));
        }
    }

    async fn response(status: &str, body: &str) -> reqwest::Url {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = reqwest::Url::parse(&format!("http://{}/actuator/beans", listener.local_addr().unwrap())).unwrap();
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nLocation: http://example.com/private\r\n\r\n{body}",
            body.len()
        );
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).await.unwrap();
            assert!(String::from_utf8_lossy(&buffer[..n]).starts_with("GET /actuator/beans "));
            socket.write_all(reply.as_bytes()).await.unwrap();
        });
        url
    }

    #[tokio::test]
    async fn isolates_errors_and_refuses_redirects() {
        let client = runtime_client().unwrap();
        for (status, expected) in [
            ("401 Unauthorized", "unauthorized"),
            ("403 Forbidden", "forbidden"),
            ("404 Not Found", "disabled"),
            ("302 Found", "redirect"),
            ("500 Error", "httpError"),
            ("200 OK", "incompatible"),
        ] {
            let result = fetch(
                &client,
                response(status, "private error body").await,
                "beans",
                "",
                "",
                &CancellationToken::new(),
            )
            .await;
            assert_eq!(result["state"], expected);
            assert!(!result.to_string().contains("private"));
        }
    }

    #[tokio::test]
    async fn cancellation_and_unreachable_are_distinct() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = reqwest::Url::parse(&format!("http://{}/beans", listener.local_addr().unwrap())).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let client = runtime_client().unwrap();
        assert_eq!(fetch(&client, url.clone(), "beans", "", "", &cancel).await["state"], "cancelled");
        drop(listener);
        assert_eq!(fetch(&client, url, "beans", "", "", &CancellationToken::new()).await["state"], "unreachable");
    }

    #[tokio::test]
    #[ignore = "requires the isolated Java 8 Boot 2.7.18 fixture"]
    async fn real_java8_fixture() {
        let base = std::env::var("SPRING_RUNTIME_FIXTURE_URL").expect("fixture URL required");
        let data = snapshot(&base, "", "", &CancellationToken::new()).await.unwrap();
        for endpoint in ENDPOINTS {
            assert_eq!(data[endpoint]["state"], "ok", "{endpoint}");
        }
        let has = |endpoint: &str, field: &str, expected: &str| {
            data[endpoint]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[field].as_str().is_some_and(|s| s.contains(expected)))
        };
        assert!(has("beans", "className", "com.example.runtime.RuntimeApplication"));
        assert!(has("conditions", "name", "enabledFixture"));
        assert!(has("conditions", "outcome", "notMatched"));
        assert!(has("env", "name", "fixture.demo.value"));
        assert!(has("env", "detail", "application.properties"));
        assert!(has("configprops", "name", "fixture.demo.value"));
        assert!(has("mappings", "methodName", "fixture"));
        for (path, expected_class) in [
            ("/runtime-child/inherited", "com.example.runtime.base.BaseController"),
            ("/runtime-nested/inherited", "com.example.runtime.base.BaseController"),
            ("/runtime-override/override", "com.example.runtime.OverrideController"),
        ] {
            let mapping = data["mappings"]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["paths"].as_array().is_some_and(|paths| paths.iter().any(|p| p == path)))
                .expect("fixture mapping required");
            assert_eq!(mapping["className"], expected_class);
            assert_eq!(mapping["methodName"], "inherited");
            assert!(mapping["methods"].as_array().unwrap().iter().any(|method| method == "GET"));
        }
        assert!(has("beans", "className", "com.example.runtime.NestedFixture$InnerBean"));
        assert!(has("beans", "className", "com.example.runtime.NestedFixture$InnerController"));
        let nested = data["mappings"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                row["paths"].as_array().is_some_and(|paths| paths.iter().any(|p| p == "/runtime-nested/mapping"))
            })
            .expect("nested controller mapping required");
        assert_eq!(nested["className"], "com.example.runtime.NestedFixture.InnerController");
        assert_eq!(nested["methodName"], "nested");
        assert!(!data.to_string().contains("RUNTIME_TEST_VALUE_MUST_NOT_CROSS_IPC"));
        assert!(!data.to_string().contains("SYNTHETIC_SOURCE_AUTH_MUST_NOT_CROSS_IPC"));
        let external = data["env"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "fixture.external-origin")
            .expect("external source required");
        assert_eq!(external["detail"], "configserver:https://config.example.test");
        let programmatic = data["env"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == "fixture.programmatic-origin")
            .expect("programmatic source required");
        assert!(programmatic["detail"].as_str().unwrap().starts_with('#'));
    }
}

#[cfg(test)]
mod fixture_error_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires secured Java 8 fixture with env disabled; credentials in process environment only"]
    async fn real_java8_security_and_disabled_endpoint() {
        let base = std::env::var("SPRING_RUNTIME_FIXTURE_URL").expect("fixture URL required");
        let anonymous = snapshot(&base, "", "", &CancellationToken::new()).await.unwrap();
        assert_eq!(anonymous["beans"]["state"], "unauthorized");
        let user = std::env::var("FIXTURE_TEST_USER").expect("ephemeral fixture user required");
        let password = std::env::var("FIXTURE_TEST_PASSWORD").expect("ephemeral fixture password required");
        let authenticated = snapshot(&base, &user, &password, &CancellationToken::new()).await.unwrap();
        assert_eq!(authenticated["beans"]["state"], "ok");
        assert_eq!(authenticated["conditions"]["state"], "ok");
        assert_eq!(authenticated["mappings"]["state"], "ok");
        assert_eq!(authenticated["env"]["state"], "disabled");
        assert_eq!(authenticated["configprops"]["state"], "forbidden");
        assert!(!authenticated.to_string().contains(&password));
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn caps_response_size_and_can_cancel_an_inflight_read() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = reqwest::Url::parse(&format!("http://{}/beans", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_BYTES + 1).as_bytes())
                .await
                .unwrap();
        });
        assert_eq!(
            fetch(&runtime_client().unwrap(), url, "beans", "", "", &CancellationToken::new()).await["state"],
            "tooLarge"
        );
        server.await.unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = reqwest::Url::parse(&format!("http://{}/beans", listener.local_addr().unwrap())).unwrap();
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n{").await.unwrap();
            trigger.cancel();
        });
        assert_eq!(fetch(&runtime_client().unwrap(), url, "beans", "", "", &cancel).await["state"], "cancelled");
        server.await.unwrap();
    }
}

#[cfg(test)]
mod source_metadata_tests {
    use super::*;

    #[test]
    fn sanitizes_external_and_programmatic_sources_without_losing_local_file_origins() {
        let marker = "SYNTHETIC_SOURCE_AUTH_MUST_NOT_CROSS_IPC";
        let local =
            "Config resource 'file [/tmp/runtime/application.properties]' via location 'optional:file:/tmp/runtime/'";
        let classpath = "Config resource 'class path resource [application.yml]' via location 'optional:classpath:/'";
        let legacy = "applicationConfig: [classpath:/application.properties]";
        let names = [
            format!("configserver:https://user:{marker}@config.example.test/app/path?token={marker}#{marker}"),
            format!("programmatic value={marker}"),
            local.into(),
            classpath.into(),
            legacy.into(),
            "systemEnvironment".into(),
            format!(
                "Config resource 'class path resource [application.yml]' via location 'https://user:{marker}@example.test/?token={marker}'"
            ),
        ];
        let data = json!({"propertySources": names.iter().map(|name| json!({"name":name,"properties":{"demo.key":{"value":marker}}})).collect::<Vec<_>>()});
        let projected = project("env", &data);
        assert!(!projected.to_string().contains(marker));
        assert_eq!(projected["rows"][0]["detail"], "configserver:https://config.example.test");
        assert_eq!(projected["rows"][1]["detail"], "#2");
        assert_eq!(projected["rows"][2]["detail"], local);
        assert_eq!(projected["rows"][3]["detail"], classpath);
        assert_eq!(projected["rows"][4]["detail"], legacy);
        assert_eq!(projected["rows"][5]["detail"], "systemEnvironment");
        assert_eq!(projected["rows"][6]["detail"], "class path resource [application.yml]");
    }

    #[test]
    fn rejects_free_form_non_key_metadata_in_conditions_and_configprops() {
        let marker = "SYNTHETIC_SOURCE_AUTH_MUST_NOT_CROSS_IPC";
        let conditions = project(
            "conditions",
            &json!({"contexts":{"app":{"positiveMatches":{"Config":[{"condition":format!("user:{marker}@host?token={marker}"),"message":marker}]},"negativeMatches":{}}}}),
        );
        let properties = project(
            "configprops",
            &json!({"contexts":{"app":{"beans":{format!("bean value={marker}"):{"prefix":format!("https://user:{marker}@host?token={marker}"),"properties":{"safe-key":marker},"inputs":{"safe-key":{"value":marker,"origin":marker}}}}}}}),
        );
        let condition_source = project(
            "conditions",
            &json!({"contexts":{"app":{"positiveMatches":{format!("https://user:{marker}@host?token={marker}"):[{"condition":"OnBeanCondition"}]}}}}),
        );
        assert!(!condition_source.to_string().contains(marker));
        assert!(!conditions.to_string().contains(marker));
        assert!(!properties.to_string().contains(marker));
        assert_eq!(properties["rows"][0]["detail"], "#1");
        assert_eq!(properties["rows"][0]["name"], "safe-key");
    }
}
