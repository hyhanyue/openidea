use ide_core::{IdeError, IdeResult};
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpringRuntimeSnapshot {
    pub beans: SpringRuntimeData,
    pub conditions: SpringRuntimeData,
    pub env: SpringRuntimeData,
    pub configprops: SpringRuntimeData,
    pub mappings: SpringRuntimeData,
}

impl SpringRuntimeSnapshot {
    /// Validates only the sanitized internal projection. Errors never include its contents.
    pub(super) fn from_projection(value: serde_json::Value) -> IdeResult<Self> {
        serde_json::from_value(value).map_err(|_| IdeError::internal("Invalid Spring Runtime snapshot projection"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpringRuntimeData {
    pub state: SpringRuntimeState,
    pub rows: Vec<SpringRuntimeRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub limited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub status: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpringRuntimeRow {
    pub name: String,
    pub context: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub class_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub method_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub descriptor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub dependencies: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub methods: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub outcome: Option<SpringRuntimeOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub masked: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SpringRuntimeState {
    Ok,
    Unauthorized,
    Forbidden,
    Disabled,
    Redirect,
    HttpError,
    Timeout,
    Unreachable,
    TooLarge,
    InvalidResponse,
    Incompatible,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SpringRuntimeOutcome {
    Matched,
    NotMatched,
    Unconditional,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn projected() -> serde_json::Value {
        let data = json!({"state":"ok","rows":[{"name":"key","context":"app","detail":"source"}]});
        json!({"beans":data,"conditions":data,"env":data,"configprops":data,"mappings":data})
    }

    #[test]
    fn async_ipc_result_implements_specta_function_result() {
        fn accepts<T, Marker>(_: T)
        where
            T: specta::function::FunctionResult<Marker>,
        {
            let mut types = specta::Types::default();
            let _ = T::to_datatype(&mut types);
        }
        let cancel = ide_core::CancellationToken::new();
        accepts(crate::spring_runtime::snapshot("", "", "", &cancel));
    }

    #[test]
    fn optional_fields_default_and_are_omitted_on_the_wire() {
        let value = projected();
        let snapshot = SpringRuntimeSnapshot::from_projection(value.clone()).unwrap();
        assert_eq!(snapshot.env.rows[0].class_name, None);
        assert_eq!(snapshot.env.rows[0].outcome, None);
        assert_eq!(snapshot.env.status, None);
        assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    }

    #[test]
    fn preserves_existing_camel_case_state_outcome_and_row_contract() {
        let mut value = projected();
        value["mappings"] = json!({"state":"ok","limited":false,"status":200,"rows":[{
            "name":"GET /demo","context":"app","detail":"main","className":"com.example.Controller",
            "methodName":"get","descriptor":"()V","dependencies":["repo"],"methods":["GET"],
            "paths":["/demo"],"outcome":"notMatched","masked":false
        }]});
        let snapshot = SpringRuntimeSnapshot::from_projection(value.clone()).unwrap();
        assert_eq!(snapshot.mappings.state, SpringRuntimeState::Ok);
        assert_eq!(snapshot.mappings.rows[0].outcome, Some(SpringRuntimeOutcome::NotMatched));
        assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
        for state in [
            "ok",
            "unauthorized",
            "forbidden",
            "disabled",
            "redirect",
            "httpError",
            "timeout",
            "unreachable",
            "tooLarge",
            "invalidResponse",
            "incompatible",
            "cancelled",
        ] {
            let data = json!({"state":state,"rows":[]});
            let decoded: SpringRuntimeData = serde_json::from_value(data.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), data);
        }
        for outcome in ["matched", "notMatched", "unconditional"] {
            let decoded: SpringRuntimeOutcome = serde_json::from_value(json!(outcome)).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), outcome);
        }
    }

    #[test]
    fn rejects_schema_drift_without_exposing_projected_contents() {
        let marker = "SYNTHETIC_CONTRACT_CONTENT_MUST_NOT_APPEAR_IN_ERRORS";
        let mut invalid = Vec::new();
        let mut value = projected();
        value.as_object_mut().unwrap().remove("beans");
        invalid.push(value);
        for pointer in
            ["/env/state", "/env/status", "/env/rows/0/outcome", "/env/rows/0/masked", "/env/rows/0/dependencies"]
        {
            let mut value = projected();
            let (parent, field) = pointer.rsplit_once('/').unwrap();
            value.pointer_mut(parent).unwrap()[field] = json!(marker);
            invalid.push(value);
        }
        for pointer in ["", "/env", "/env/rows/0"] {
            let mut value = projected();
            value.pointer_mut(pointer).unwrap()[marker] = json!(marker);
            invalid.push(value);
        }
        for value in invalid {
            let error = SpringRuntimeSnapshot::from_projection(value).unwrap_err();
            assert_eq!(error.kind, ide_core::error::ErrorKind::Internal);
            assert_eq!(error.message, "Invalid Spring Runtime snapshot projection");
            assert!(!error.to_string().contains(marker));
        }
    }
}
