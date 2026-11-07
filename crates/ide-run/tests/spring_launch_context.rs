// Test the host's pure context boundary without building the native app / WASM runtime.
#[path = "../../ide-host/src/plugins/launch_context.rs"]
mod launch_context;

#[test]
fn adapter_command_is_namespaced_to_spring_beans() {
    for id in ["hyhanyue.spring-beans", "aritti.spring-beans", "hyhanyue.spring-beans-local"] {
        assert!(launch_context::is_plugin(id));
    }
    assert!(!launch_context::is_plugin("untrusted.spring-beans"));
    assert!(launch_context::parse("null").unwrap().is_empty());
    assert!(
        launch_context::parse(r#"{"version":1,"launch":null}"#).unwrap()["springBeans.launchContext"]["launch"]
            .is_null()
    );
    assert_eq!(launch_context::COMMAND, "openidea.setLaunchContext");
}

#[test]
fn accepts_the_exact_shared_producer_payloads_for_all_package_aliases() {
    let contract: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/spring-launch-context-contract.json")).unwrap();
    for fixture in contract["fixtures"].as_array().unwrap() {
        let request = serde_json::json!({"projectRoot":"/project","context":fixture["producerExpected"]});
        let parsed = launch_context::parse_request(&request.to_string(), std::path::Path::new("/project")).unwrap();
        if fixture["producerExpected"].is_null() {
            assert!(parsed.is_empty());
        } else {
            assert_eq!(parsed["springBeans.launchContext"], fixture["producerExpected"]);
        }
    }
}
