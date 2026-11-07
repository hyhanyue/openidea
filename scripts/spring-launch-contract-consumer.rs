#[cfg(test)]
mod f_launch_contract {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn production_payloads_and_consumer_share_the_frozen_combination_samples() {
        let contract: Value = serde_json::from_str(include_str!("../f-launch-contract.json")).unwrap();
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        for name in contract["sequence"].as_array().unwrap() {
            let fixture = contract["fixtures"].as_array().unwrap().iter().find(|f| f["name"] == *name).unwrap();
            for (path, text) in fixture["projectFiles"].as_object().unwrap() {
                host.add_file(path, text.as_str().unwrap());
            }
            host.set_setting("springBeans.launchContext", &fixture["producerExpected"].to_string());
            let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
            let expected = &fixture["consumerExpected"];
            if let Some(profiles) = expected["profiles"].as_array() {
                let expected: Vec<_> = profiles.iter().map(|v| v.as_str().unwrap()).collect();
                assert_eq!(snapshot.profile_order, expected, "sample {name}");
            }
            if fixture["producerExpected"].is_null() {
                assert!(snapshot.launch.is_none(), "deselection must not retain the last config");
                assert!(!snapshot.profile_order.iter().any(|p| p == "dev" || p == "prod"));
            }
            if let Some(properties) = expected["properties"].as_object() {
                for (key, fact) in properties {
                    let resolved = snapshot.resolve(key);
                    match fact["state"].as_str().unwrap() {
                        "known" => assert_eq!(
                            resolved.state,
                            State::Value(fact["value"].as_str().unwrap().into()),
                            "sample {name}: {key}"
                        ),
                        "unknown" => assert!(
                            matches!(resolved.state, State::Unknown(_)),
                            "sample {name}: unknown must not become missing/false or a lower file value"
                        ),
                        _ => panic!("unsupported contract state"),
                    }
                    if let Some(source) = fact["source"].as_str() {
                        let candidate = snapshot.candidates(key).last().expect("source retained").origin.to_lowercase();
                        let needle = match source {
                            "commandLine" => "command",
                            "systemProperties" => "system",
                            "environment" => "environment",
                            _ => source,
                        };
                        assert!(candidate.contains(needle), "sample {name}: source {source} was not retained");
                    }
                }
            }
        }
    }
}
