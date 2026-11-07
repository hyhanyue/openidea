use super::*;
use openidea_plugin::text::{offset, position};

fn source(entries: &[(&str, &str)]) -> Sources {
    entries.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect()
}

fn at(s: &Sources, path: &str, word: &str) -> Target {
    let text = &s[path];
    let byte = text.find(&format!("\"{word}\"")).map(|i| i + 1).unwrap_or_else(|| text.find(word).unwrap());
    target(s, path, position(text, byte)).expect("semantic target")
}

fn apply(s: &Sources, p: &Plan) -> Sources {
    let mut out = s.clone();
    for (path, edits) in &p.files {
        let text = &s[path];
        let mut changes: Vec<_> = edits
            .iter()
            .map(|e| (offset(text, e.range.start).unwrap(), offset(text, e.range.end).unwrap(), &e.text))
            .collect();
        changes.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (from, to, insert) in changes {
            out.get_mut(path).unwrap().replace_range(from..to, insert);
        }
    }
    out
}

#[test]
fn bean_name_is_not_java_symbol_or_plain_text() {
    let s = source(&[
        (
            "Config.java",
            "import org.springframework.context.annotation.Bean; class Config { @Bean(\"port\") Object makePort() { return null; } }",
        ),
        (
            "Use.java",
            "import org.springframework.beans.factory.annotation.Qualifier; class Use { /*😀*/ @Qualifier(\"port\") Object p; String text = \"port\"; // @Qualifier(\"port\")\n}",
        ),
        (
            "beans.xml",
            "<beans xmlns='http://www.springframework.org/schema/beans'><bean id='consumer'><property name='port' ref='port'/></bean><!-- ref='port' --></beans>",
        ),
    ]);
    let plan = plan(&s, &at(&s, "Config.java", "port"), "channel").unwrap();
    assert!(plan.java.is_none());
    let after = apply(&s, &plan);
    assert!(after["Config.java"].contains("@Bean(\"channel\") Object makePort"));
    assert!(
        after["Use.java"].contains("@Qualifier(\"channel\") Object p; String text = \"port\"; // @Qualifier(\"port\")")
    );
    assert!(after["beans.xml"].contains("name='port' ref='channel'"));
    assert!(after["beans.xml"].contains("<!-- ref='port' -->"));
    let e = &plan.files["Use.java"][0];
    let before = s["Use.java"].find("\"port\"").unwrap() + 1;
    assert_eq!(e.range.start.character as usize, s["Use.java"][..before].encode_utf16().count());
}

#[test]
fn qualifier_tags_and_duplicate_definitions_are_unknown() {
    let s = source(&[(
        "A.java",
        "import org.springframework.context.annotation.Bean; import org.springframework.beans.factory.annotation.Qualifier; class A { @Bean(\"port\") @Qualifier(\"port\") Object a() { return null; } }",
    )]);
    let pos = position(&s["A.java"], s["A.java"].find("\"port\"").unwrap() + 1);
    assert!(target(&s, "A.java", pos).is_none());
    assert!(recognized(&s, "A.java", pos));
    let s = source(&[(
        "A.java",
        "import org.springframework.context.annotation.Bean; class A { @Bean(\"port\") Object a() {return null;} @Bean(\"port\") Object b() {return null;} }",
    )]);
    assert!(target(&s, "A.java", position(&s["A.java"], s["A.java"].find("port").unwrap())).is_none());
}

#[test]
fn custom_annotations_constants_and_other_xml_namespaces_are_untouched() {
    let s = source(&[
        (
            "A.java",
            "import com.acme.Bean; import com.acme.Value; class A { @Bean(\"port\") Object x; @Value(\"${demo.key}\") String y; }",
        ),
        (
            "B.java",
            "import org.springframework.context.annotation.Bean; class B { @Bean(\"port\" + \"Extra\") Object x() { return null; } }",
        ),
        ("other.xml", "<beans><bean id='port'/></beans>"),
    ]);
    for (path, text) in &s {
        assert!(target(&s, path, position(text, text.find("port").unwrap())).is_none());
    }
}

#[test]
fn implicit_bean_uses_java_lsp_and_spring_decapitalization() {
    let s = source(&[
        ("URLPort.java", "import org.springframework.stereotype.Service; @Service public class URLPort {}"),
        (
            "Use.java",
            "import org.springframework.beans.factory.annotation.Qualifier; class Use { @Qualifier(\"URLPort\") Object p; }",
        ),
    ]);
    let p = plan(&s, &at(&s, "URLPort.java", "URLPort"), "HttpPort").unwrap();
    assert_eq!(p.java.as_ref().unwrap().old_name, "URLPort");
    assert!(apply(&s, &p)["Use.java"].contains("@Qualifier(\"httpPort\")"));
    assert!(!p.files.contains_key("URLPort.java"));
}

#[test]
fn implicit_bean_name_reference_materializes_explicit_name_without_java_rename() {
    for declaration in [
        "import org.springframework.stereotype.Service; @Service class Port {}",
        "import org.springframework.stereotype.Service; @Service(/*keep*/) class Port {}",
        "import org.springframework.context.annotation.Bean; class Config { @Bean Object port() { return null; } }",
    ] {
        let s = source(&[
            ("Port.java", declaration),
            (
                "Use.java",
                "import org.springframework.beans.factory.annotation.Qualifier; class Use { @Qualifier(\"port\") Object p; }",
            ),
        ]);
        let p = plan(&s, &at(&s, "Use.java", "port"), "channel").unwrap();
        assert!(p.java.is_none());
        let after = apply(&s, &p);
        assert!(after["Port.java"].contains("(\"channel\""));
        assert!(after["Use.java"].contains("@Qualifier(\"channel\")"));
        assert!(after["Port.java"].contains("class Port") || after["Port.java"].contains("Object port()"));
        if declaration.contains("/*keep*/") {
            assert!(after["Port.java"].contains("/*keep*/"));
        }
    }
}

#[test]
fn binding_fields_and_accessors_are_recognized_as_unsupported() {
    let s = source(&[(
        "Settings.java",
        "import org.springframework.boot.context.properties.ConfigurationProperties; @ConfigurationProperties(\"app\") class Settings { String label; String getLabel() { return label; } }",
    )]);
    for word in ["label;", "getLabel"] {
        let pos = position(&s["Settings.java"], s["Settings.java"].find(word).unwrap());
        assert!(recognized(&s, "Settings.java", pos));
        assert!(target(&s, "Settings.java", pos).is_none());
        assert!(refusal(&s, "Settings.java", pos).contains("accessor"));
    }
}

#[test]
fn config_leaf_keeps_comments_values_defaults_and_unrelated_files() {
    let s = source(&[
        (
            "application.properties",
            "# demo.timeout=comment\ndemo.timeout=42\nlabel=demo.timeout\nother=${demo.timeout:99}\n",
        ),
        ("application-dev.yml", "demo:\n  timeout: 43 # timeout\ntext: demo.timeout\n"),
        (
            "Use.java",
            "import org.springframework.beans.factory.annotation.Value; class Use { @Value(\"${demo.timeout:88}\") int timeout; String s = \"${demo.timeout}\"; }",
        ),
        ("notes.properties", "demo.timeout=unrelated"),
    ]);
    let p = plan(&s, &at(&s, "application.properties", "demo.timeout=42"), "demo.delay").unwrap();
    let out = apply(&s, &p);
    assert_eq!(
        out["application.properties"],
        "# demo.timeout=comment\ndemo.delay=42\nlabel=demo.timeout\nother=${demo.delay:99}\n"
    );
    assert!(out["application-dev.yml"].contains("  delay: 43 # timeout"));
    assert!(out["Use.java"].contains("@Value(\"${demo.delay:88}\") int timeout; String s = \"${demo.timeout}\""));
    assert_eq!(out["notes.properties"], s["notes.properties"]);
}

#[test]
fn configuration_prefix_and_leaf_safety() {
    let s = source(&[
        (
            "Settings.java",
            "import org.springframework.boot.context.properties.ConfigurationProperties; @ConfigurationProperties(prefix=\"demo\") class Settings { String label; }",
        ),
        ("application.yml", "demo:\n  label: value\n  timeout: 3\nother:\n  demo: unrelated\n"),
        ("application-dev.properties", "demo.label=dev\ndemo.timeout=9\nref=${demo.label}\n"),
    ]);
    let p = plan(&s, &at(&s, "Settings.java", "demo"), "app").unwrap();
    let out = apply(&s, &p);
    assert_eq!(out["application.yml"], "app:\n  label: value\n  timeout: 3\nother:\n  demo: unrelated\n");
    assert!(out["application-dev.properties"].contains("app.label=dev\napp.timeout=9\nref=${app.label}"));
    assert_eq!(p.files["application.yml"].len(), 1);
    assert!(plan(&s, &at(&s, "application-dev.properties", "demo.label"), "demo.title").is_err());
    assert!(plan(&s, &at(&s, "Settings.java", "demo"), "other.app").is_err());
}

#[test]
fn mapper_is_namespaced_and_sql_identifiers_are_separate() {
    let s = source(&[
        ("UserMapper.java", "package app; public interface UserMapper { int find(); }"),
        (
            "UserMapper.xml",
            "<mapper namespace='app.UserMapper'><select id='find'>select find from t</select><resultMap id='find' type='X'><association property='x' select='find'/></resultMap><sql id='find'>find</sql><!-- <select id='find'/> --></mapper>",
        ),
        (
            "OtherMapper.xml",
            "<mapper namespace='app.OtherMapper'><select id='find'>select 2</select><resultMap id='x' type='X'><association property='x' select='app.UserMapper.find'/></resultMap></mapper>",
        ),
    ]);
    let p = plan(&s, &at(&s, "UserMapper.xml", "find"), "lookup").unwrap();
    assert_eq!(p.java.as_ref().unwrap().path, "UserMapper.java");
    let out = apply(&s, &p);
    assert!(out["UserMapper.xml"].contains("id='lookup'>select find from t"));
    assert!(out["UserMapper.xml"].contains("resultMap id='find'"));
    assert!(out["UserMapper.xml"].contains("select='lookup'"));
    assert!(out["UserMapper.xml"].contains("<sql id='find'>find</sql>"));
    assert!(out["OtherMapper.xml"].contains("<select id='find'>"));
    assert!(out["OtherMapper.xml"].contains("select='app.UserMapper.lookup'"));
}

#[test]
fn mapper_overloads_variants_and_collisions_refuse_every_edit() {
    let s = source(&[
        ("M.java", "package a; interface M { int find(); int find(int id); }"),
        ("M.xml", "<mapper namespace='a.M'><select id='find'>select 1</select></mapper>"),
    ]);
    assert!(plan(&s, &at(&s, "M.xml", "find"), "lookup").is_err());
    let s = source(&[
        ("M.java", "package a; interface M { int find(); int lookup(); }"),
        (
            "M.xml",
            "<mapper namespace='a.M'><select id='find'>select 1</select><select id='lookup'>select 2</select></mapper>",
        ),
    ]);
    assert!(plan(&s, &at(&s, "M.xml", "find"), "lookup").is_err());
}
