//! Annotation misuse in Java code: `@Profile` expressions and `@Async` return
//! types. Quiet for foreign annotations and non-literal arguments.

use crate::checks::spring;
use crate::model::is_repository;
use openidea_plugin::Severity;
use spring_text::finding::Finding;
use spring_text::java::{Annotation, JavaFile, TypeDecl, TypeRef};

const CACHE_ANNOTATIONS: [&str; 3] = ["Cacheable", "CachePut", "CacheEvict"];
const CACHE_NAME_ARGS: [&str; 3] = ["value", "cacheNames", "cacheResolver"];

fn non_future_return(file: &JavaFile, returned: &TypeRef) -> bool {
    if returned.array {
        return true;
    }
    let package = match returned.name.as_str() {
        "boolean" | "byte" | "short" | "char" | "int" | "long" | "float" | "double" => return true,
        "String" | "Byte" | "Short" | "Character" | "Integer" | "Long" | "Float" | "Double" | "Boolean" | "Object"
        | "Void" => "java.lang",
        "List" | "Set" | "Map" | "Optional" => "java.util",
        "CompletionStage" => "java.util.concurrent",
        _ => return false,
    };
    let expected = format!("{package}.{}", returned.name);
    let written = returned.text.split('<').next().unwrap_or_default().split_whitespace().collect::<String>();
    if written.contains('.') {
        return written == expected;
    }
    if file.types.iter().any(|ty| ty.name == returned.name) {
        return false;
    }
    if let Some(import) = file.imports.iter().find(|i| !i.is_static && i.path.ends_with(&format!(".{}", returned.name)))
    {
        return import.path == expected;
    }
    if file.imports.iter().any(|i| !i.is_static && i.path.ends_with(".*") && !i.path.starts_with("java.")) {
        return false;
    }
    returned.name != "CompletionStage" || file.imports.iter().any(|i| i.path == "java.util.concurrent.*")
}

#[derive(Clone, Copy, PartialEq)]
enum Token {
    Name,
    Not,
    And,
    Or,
    Open,
    Close,
}

fn tokens(value: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut in_name = false;
    for c in value.chars() {
        let token = match c {
            '!' => Token::Not,
            '&' => Token::And,
            '|' => Token::Or,
            '(' => Token::Open,
            ')' => Token::Close,
            c => {
                if !c.is_whitespace() && !in_name {
                    out.push(Token::Name);
                    in_name = true;
                }
                continue;
            }
        };
        in_name = false;
        out.push(token);
    }
    out
}

/// What is wrong with a profile expression such as `a & !b`, if anything.
/// Repeated or trailing operators and juxtaposed operands stay quiet: Spring's
/// own parser may accept them.
pub fn profile_expression_problem(value: &str) -> Option<&'static str> {
    let tokens = tokens(value);
    if tokens.is_empty() {
        return Some("A profile name can't be empty: Spring rejects the value when the context is built.");
    }
    let mut at = 0;
    if let Err(problem) = expression(&tokens, &mut at) {
        return problem;
    }
    (tokens.get(at) == Some(&Token::Close)).then_some(STRAY_CLOSE)
}

const STRAY_CLOSE: &str = "A profile expression has a closing parenthesis without an opening one.";

/// `Err(None)` is a shape Spring may still accept.
type Parsed = Result<(), Option<&'static str>>;

fn expression(tokens: &[Token], at: &mut usize) -> Parsed {
    term(tokens, at)?;
    let mut operator = None;
    while let Some(&op) = tokens.get(*at).filter(|t| matches!(t, Token::And | Token::Or)) {
        if operator.is_some_and(|first| first != op) {
            return Err(Some(
                "A profile expression can't mix & and | at one level: wrap one of them in parentheses, as in (a & b) | c.",
            ));
        }
        operator = Some(op);
        *at += 1;
        term(tokens, at)?;
    }
    Ok(())
}

fn term(tokens: &[Token], at: &mut usize) -> Parsed {
    let token = tokens.get(*at).copied();
    *at += 1;
    match token {
        Some(Token::Name) => Ok(()),
        Some(Token::Not) => {
            if !matches!(tokens.get(*at), Some(Token::Name | Token::Not | Token::Open)) {
                return Err(Some(
                    "A ! in a profile expression needs a profile name or a parenthesised expression after it.",
                ));
            }
            term(tokens, at)
        }
        Some(Token::Open) => {
            if tokens.get(*at) == Some(&Token::Close) {
                return Err(Some("A profile expression has empty parentheses: put a profile name inside."));
            }
            expression(tokens, at)?;
            match tokens.get(*at) {
                Some(Token::Close) => {
                    *at += 1;
                    Ok(())
                }
                None => Err(Some("A profile expression has an opening parenthesis without a closing one.")),
                Some(_) => Err(None),
            }
        }
        _ => Err(None),
    }
}

fn profile_values(file: &JavaFile, annotations: &[Annotation], out: &mut Vec<Finding>) {
    let Some(profile) = spring(file, annotations, "Profile") else { return };
    let Some(value) = profile.arg("value") else { return };
    if value.text.contains('+') {
        return;
    }
    for literal in &value.strings {
        if literal.value.contains("${") || literal.value.contains("#{") {
            continue;
        }
        let Some(problem) = profile_expression_problem(&literal.value) else { continue };
        out.push(Finding::new(literal.at, literal.end, "profile-expression", problem).with_severity(Severity::Error));
    }
}

pub fn check_profiles(file: &JavaFile, ty: &TypeDecl, out: &mut Vec<Finding>) {
    profile_values(file, &ty.annotations, out);
    for method in ty.methods() {
        profile_values(file, &method.annotations, out);
    }
}

pub fn check_async(file: &JavaFile, ty: &TypeDecl, out: &mut Vec<Finding>) {
    let class_async = spring(file, &ty.annotations, "Async").is_some();
    for method in ty.methods() {
        let method_async = spring(file, &method.annotations, "Async").is_some();
        if !method_async
            && (!class_async || method.modifiers.iter().any(|m| ["private", "static", "final"].contains(&m.as_str())))
        {
            continue;
        }
        let Some(returned) = &method.ty else { continue };
        if !non_future_return(file, returned) {
            continue;
        }
        out.push(Finding::new(
            returned.at,
            returned.at + returned.name.len(),
            "async-return-type",
            format!("An @Async method should return void or a Future-compatible type: this {} return type cannot deliver the asynchronous result.", returned.name),
        ));
    }
}

fn lacks_cache_name(annotation: &Annotation) -> bool {
    CACHE_NAME_ARGS.iter().all(|name| annotation.arg(name).is_none())
}

/// Cache annotations on `annotations`, as findings on the annotation itself.
fn cache_annotations<'a>(file: &JavaFile, annotations: &'a [Annotation]) -> Vec<&'a Annotation> {
    CACHE_ANNOTATIONS.iter().filter_map(|name| spring(file, annotations, name)).collect()
}

pub fn check_cache(file: &JavaFile, ty: &TypeDecl, out: &mut Vec<Finding>) {
    if ty.kind == "interface" {
        if is_repository(&ty.kind, ty.supers().map(|s| s.name.as_str())) {
            return;
        }
        let on_type = cache_annotations(file, &ty.annotations);
        let on_methods = ty.methods().flat_map(|m| cache_annotations(file, &m.annotations));
        for annotation in on_type.into_iter().chain(on_methods) {
            out.push(Finding::new(
                annotation.at,
                annotation.end,
                "cache-annotation-on-interface",
                "Cache annotations on an interface only take effect with interface-based (JDK) proxies; class-based proxies and AspectJ mode miss them. Annotate the concrete class instead.",
            ).with_severity(Severity::Hint));
        }
        return;
    }

    // A supertype may carry the @CacheConfig: Spring looks it up through the hierarchy.
    let names_supplied_by_class =
        spring(file, &ty.annotations, "CacheConfig").is_some() || ty.supers().next().is_some();
    for method in ty.methods() {
        if spring(file, &method.annotations, "Caching").is_some() {
            continue;
        }
        let cache_annotations = cache_annotations(file, &method.annotations);
        if !names_supplied_by_class {
            for annotation in cache_annotations.iter().filter(|a| lacks_cache_name(a)) {
                out.push(Finding::new(
                    annotation.at,
                    annotation.end,
                    "cache-names-missing",
                    format!("@{} names no cache: set value or cacheNames, or put @CacheConfig(cacheNames = ...) on the class, or Spring fails when the method runs.", annotation.name),
                ));
            }
        }
        if let Some(put) = cache_annotations.iter().find(|a| a.name == "CachePut") {
            let Some(cacheable) = cache_annotations.iter().find(|a| a.name == "Cacheable") else { continue };
            let second = if put.at > cacheable.at { put } else { cacheable };
            out.push(Finding::new(
                second.at,
                second.end,
                "cacheable-and-cache-put",
                "@Cacheable skips the method when the entry exists, while @CachePut always runs it: combining them on one method gives surprising results.",
            ));
        }
    }
}

#[cfg(test)]
mod async_contract_tests {
    use super::*;
    use spring_text::java;

    fn findings(text: &str) -> Vec<Finding> {
        let file = java::parse(text);
        let mut out = Vec::new();
        for ty in &file.types {
            check_async(&file, ty, &mut out);
        }
        out
    }

    #[test]
    fn type_level_async_checks_interceptable_methods() {
        let text = "import org.springframework.scheduling.annotation.Async; @Async class Jobs { public String text(){return null;} public int count(){return 0;} public void run(){} public Future<String> future(){return null;} public CompletableFuture<String> complete(){return null;} }";
        let found = findings(text);
        assert_eq!(found.len(), 2);
        assert_eq!(&text[found[0].start..found[0].end], "String");
        assert_eq!(&text[found[1].start..found[1].end], "int");
    }

    #[test]
    fn class_defaults_do_not_treat_uninterceptable_helpers_as_async_calls() {
        let text = "@Async class Jobs { private String helper(){return null;} public final String local(){return null;} static int value(){return 0;} public Future<String> allowed(){return null;} }";
        assert!(findings(text).is_empty());
        assert!(findings("import other.Async; @Async class Jobs { public String text(){return null;} }").is_empty());
    }

    #[test]
    fn return_type_identity_and_unknown_hierarchies_remain_conservative() {
        assert!(findings("import other.String; class Jobs { @Async public String future(){return null;} }").is_empty());
        assert!(
            findings(
                "class String extends FutureTask<Object> {} class Jobs { @Async public String future(){return null;} }"
            )
            .is_empty()
        );
        assert!(findings("class Jobs { @Async public other.String future(){return null;} }").is_empty());
        assert!(findings("class Jobs { @Async public CustomFuture future(){return null;} }").is_empty());
        assert_eq!(
            findings("import other.String; class Jobs { @Async public java.lang.String text(){return null;} }").len(),
            1
        );
    }

    #[test]
    fn arrays_and_explicit_completion_stage_cannot_carry_async_results() {
        assert_eq!(findings("class Jobs { @Async public Future<String>[] values(){return null;} }").len(), 1);
        assert_eq!(findings("import java.util.concurrent.CompletionStage; class Jobs { @Async public CompletionStage<String> value(){return null;} }").len(), 1);
        assert_eq!(
            findings("class Jobs { @Async public java.util.concurrent.CompletionStage<String> value(){return null;} }")
                .len(),
            1
        );
    }
}
