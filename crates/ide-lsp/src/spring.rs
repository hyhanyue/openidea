//! Spring Tools' Java model requests are answered by the existing JDT LS.

use crate::LspManager;
use crate::server::Server;
use ide_core::{CancellationToken, ServerId};
use ide_jsonrpc::codes;
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(crate) const SERVER: &str = "spring-boot";
const TIMEOUT: Duration = Duration::from_secs(30);

type Reply = Result<Value, (i64, String)>;

struct Listener {
    server: Weak<Server>,
    generation: u64,
}

#[derive(Default)]
pub(crate) struct Bridge {
    listeners: Mutex<HashMap<(ServerId, u64, String), Listener>>,
}

fn java_command(method: &str) -> Option<&'static str> {
    Some(match method {
        "sts/javaType" => "sts.java.type",
        "sts/javadocHoverLink" => "sts.java.javadocHoverLink",
        "sts/javaLocation" => "sts.java.location",
        "sts/javadoc" => "sts.java.javadoc",
        "sts/javaSearchTypes" => "sts.java.search.types",
        "sts/javaSearchPackages" => "sts.java.search.packages",
        "sts/javaSubTypes" => "sts.java.hierarchy.subtypes",
        "sts/javaSuperTypes" => "sts.java.hierarchy.supertypes",
        "sts/javaCodeComplete" => "sts.java.code.completions",
        "sts/project/gav" => "sts.project.gav",
        "sts/addClasspathListener" => "sts.java.addClasspathListener",
        "sts/removeClasspathListener" => "sts.java.removeClasspathListener",
        _ => return None,
    })
}

fn invalid(message: &str) -> (i64, String) {
    (codes::INVALID_PARAMS, message.into())
}

impl Bridge {
    /// Called on JSON-RPC's blocking handler pool, never its reader loop.
    pub(crate) fn request(
        &self,
        mgr: &LspManager,
        source: &Arc<Server>,
        generation: u64,
        method: &str,
        params: Value,
    ) -> Reply {
        if source.name != SERVER {
            return Err((codes::METHOD_NOT_FOUND, format!("unsupported method {method}")));
        }
        let Some(command) = java_command(method) else {
            return Err((codes::METHOD_NOT_FOUND, format!("unsupported Spring request {method}")));
        };
        if !params.is_object() {
            return Err(invalid("Spring request parameters must be an object"));
        }
        let callback = if method.ends_with("ClasspathListener") {
            Some(
                params["callbackCommandId"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .ok_or_else(|| invalid("Missing or invalid callbackCommandId"))?
                    .to_owned(),
            )
        } else {
            None
        };
        mgr.runtime.block_on(async {
            let cancel = CancellationToken::new();
            let (java, run) =
                mgr.jdtls(&source.root, &cancel).await.map_err(|e| (codes::INTERNAL_ERROR, e.to_string()))?;
            if !run.caps.read().commands().iter().any(|c| c == command) {
                return Err((
                    codes::METHOD_NOT_FOUND,
                    "Spring Tools JDT extension is missing or incompatible; add its bundles and restart Java".into(),
                ));
            }
            let key = callback.as_ref().map(|id| (java.id, run.generation, id.clone()));
            if method == "sts/addClasspathListener"
                && let Some(key) = &key
            {
                let mut listeners = self.listeners.lock();
                listeners.retain(|_, l| l.server.upgrade().is_some_and(|s| s.initialized_gen(l.generation).is_some()));
                if let Some(existing) = listeners.get(key)
                    && existing.server.upgrade().is_some_and(|s| s.id != source.id)
                {
                    return Err(invalid("Classpath callback is already registered by another Spring server"));
                }
                if listeners.len() >= 128 && !listeners.contains_key(key) {
                    return Err(invalid("Too many active Spring classpath listeners"));
                }
                listeners.insert(key.clone(), Listener { server: Arc::downgrade(source), generation });
            }
            let arguments = callback.map_or_else(|| vec![params], |id| vec![json!(id)]);
            let result = tokio::time::timeout(
                TIMEOUT,
                run.peer.request::<_, Value>(
                    "workspace/executeCommand",
                    json!({ "command": command, "arguments": arguments }),
                    Some(cancel.clone()),
                ),
            )
            .await;
            let result = match result {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(e)) => Err((codes::INTERNAL_ERROR, e.to_string())),
                Err(_) => {
                    cancel.cancel();
                    Err((codes::INTERNAL_ERROR, "Spring Java request timed out".into()))
                }
            };
            if (method == "sts/removeClasspathListener" || result.is_err())
                && let Some(key) = key
            {
                self.listeners.lock().remove(&key);
            }
            result
        })
    }

    pub(crate) fn callback(&self, mgr: &LspManager, source: &Server, generation: u64, params: Value) -> Reply {
        if source.name != "jdtls" {
            return Err((codes::METHOD_NOT_FOUND, "Only JDT LS can send Spring classpath callbacks".into()));
        }
        let command = params["command"].as_str().ok_or_else(|| invalid("Missing callback command"))?;
        let target = self
            .listeners
            .lock()
            .get(&(source.id, generation, command.into()))
            .and_then(|l| l.server.upgrade().and_then(|s| s.initialized_gen(l.generation)).map(|r| r.peer));
        let Some(peer) = target else {
            return Err((codes::METHOD_NOT_FOUND, format!("No active Spring listener for {command}")));
        };
        mgr.runtime.block_on(async {
            tokio::time::timeout(TIMEOUT, peer.request::<_, Value>("workspace/executeCommand", params, None))
                .await
                .map_err(|_| (codes::INTERNAL_ERROR, "Spring classpath callback timed out".into()))?
                .map_err(|e| (codes::INTERNAL_ERROR, e.to_string()))
        })
    }
}

pub(crate) fn document_language(server: &str, language: &str) -> String {
    match (server, language) {
        (SERVER, "properties") => "spring-boot-properties".into(),
        (SERVER, "yaml") => "spring-boot-properties-yaml".into(),
        _ => language.into(),
    }
}

pub(crate) fn configure_document(server: &str, params: &mut Value) {
    if server == SERVER
        && let Some(language) = params.pointer("/textDocument/languageId").and_then(Value::as_str)
    {
        params["textDocument"]["languageId"] = json!(document_language(server, language));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_requests_are_an_explicit_allowlist() {
        assert_eq!(java_command("sts/javaLocation"), Some("sts.java.location"));
        assert_eq!(java_command("sts/addClasspathListener"), Some("sts.java.addClasspathListener"));
        assert_eq!(java_command("sts/arbitraryCommand"), None);
        assert_eq!(java_command("workspace/executeCommand"), None);
    }

    #[test]
    fn only_spring_gets_its_configuration_language_ids() {
        assert_eq!(document_language(SERVER, "properties"), "spring-boot-properties");
        assert_eq!(document_language(SERVER, "yaml"), "spring-boot-properties-yaml");
        assert_eq!(document_language(SERVER, "java"), "java");
        assert_eq!(document_language("yaml-language-server", "yaml"), "yaml");
    }
}
