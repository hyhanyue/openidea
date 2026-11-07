//! Java generic qualifiers. Unknown signatures stay candidates and suppress
//! resolution warnings: a source index cannot infer erased/library types.
use crate::model::{FileModel, unwrap};
use openidea_plugin::Range;
use spring_text::java::{self, JavaFile, MemberKind, TypeRef};
use spring_text::range;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    types: Vec<Declaration>,
    beans: Vec<(Range, Type)>,
    injections: Vec<(Range, Type)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Declaration {
    name: String,
    parameters: Vec<String>,
    supers: Vec<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Type {
    name: String,
    args: Vec<Type>,
    array: usize,
    unknown: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    Yes,
    No,
    Unknown,
}

fn simple(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn parameters(text: &str, at: usize) -> Vec<String> {
    let tokens = java::tokens(&text[at..]);
    if tokens.get(1).is_none_or(|t| t.s != "<") {
        return Vec::new();
    }
    let end = java::closing(&tokens, 1);
    java::split_commas(&tokens[2..end.max(2)], true)
        .iter()
        .filter_map(|p| p.first().filter(|t| t.is_ident()).map(|t| t.s.to_owned()))
        .collect()
}

fn qualify(file: &JavaFile, ty: &TypeRef) -> (String, bool) {
    let head: String =
        java::tokens(&ty.text).into_iter().take_while(|t| t.s != "<" && t.s != "[").map(|t| t.s).collect();
    if head.contains('.') && !head.ends_with("...") {
        let relative = head.split('.').next().is_some_and(|part| part.chars().next().is_some_and(char::is_uppercase));
        return (head, relative);
    }
    if let Some(import) = file.imports.iter().find(|i| !i.is_static && simple(&i.path) == ty.name) {
        return (import.path.clone(), false);
    }
    if [
        "String",
        "Integer",
        "Long",
        "Double",
        "Float",
        "Short",
        "Byte",
        "Boolean",
        "Character",
        "Number",
        "Object",
        "Void",
    ]
    .contains(&ty.name.as_str())
    {
        return (format!("java.lang.{}", ty.name), false);
    }
    if file.imports.iter().any(|i| !i.is_static && i.path == "java.util.*")
        && ["List", "Map", "Set", "Collection", "Optional", "ArrayList", "HashMap", "HashSet"]
            .contains(&ty.name.as_str())
    {
        return (format!("java.util.{}", ty.name), false);
    }
    let uncertain = file
        .imports
        .iter()
        .any(|i| !i.is_static && i.path.ends_with(".*") && !i.path.starts_with("org.springframework."))
        && !file.types.iter().any(|t| t.name == ty.name);
    (if file.package.is_empty() { ty.name.clone() } else { format!("{}.{}", file.package, ty.name) }, uncertain)
}

pub(super) fn array_dimensions(ty: &TypeRef) -> usize {
    let tokens = java::tokens(&ty.text);
    let mut end = tokens.len();
    let mut dimensions = usize::from(ty.text.trim_end().ends_with("..."));
    while end >= 2 && tokens[end - 2].s == "[" && tokens[end - 1].s == "]" {
        dimensions += 1;
        end -= 2;
    }
    dimensions
}

fn member_variables(text: &str, member: &java::Member, class_variables: &[String]) -> Vec<String> {
    let mut variables = class_variables.to_vec();
    if member.kind == MemberKind::Field {
        return variables;
    }
    let end = member.ty.as_ref().map_or(member.at, |ty| ty.at);
    let tokens = java::tokens(&text[member.start..end]);
    if let Some(lt) = tokens.iter().position(|t| t.s == "<") {
        let end = java::closing(&tokens, lt);
        variables.extend(
            java::split_commas(&tokens[lt + 1..end.max(lt + 1)], true)
                .iter()
                .filter_map(|p| p.first().filter(|t| t.is_ident()).map(|t| t.s.to_owned())),
        );
    }
    variables
}

impl Type {
    fn read(file: &JavaFile, ty: &TypeRef, variables: &[String]) -> Self {
        Self::read_type(file, ty, variables, false)
    }

    fn read_type(file: &JavaFile, ty: &TypeRef, variables: &[String], argument: bool) -> Self {
        let variable = variables.contains(&ty.name);
        let (name, uncertain) = if variable { (ty.name.clone(), true) } else { qualify(file, ty) };
        Self {
            name,
            args: ty.args.iter().map(|a| Self::read_type(file, a, variables, true)).collect(),
            array: array_dimensions(ty),
            unknown: variable || argument && uncertain || ty.text.contains('?') || ty.name == "?",
        }
    }

    fn substitute(&self, bindings: &BTreeMap<&str, &Type>) -> Self {
        if let Some(bound) = bindings.get(self.name.as_str()) {
            let mut bound = (*bound).clone();
            bound.array += self.array;
            return bound;
        }
        Self { args: self.args.iter().map(|a| a.substitute(bindings)).collect(), ..self.clone() }
    }

    fn concrete(&self) -> bool {
        !self.unknown && self.args.iter().all(Self::concrete)
    }

    fn matches(&self, other: &Self) -> Match {
        if !self.concrete() || !other.concrete() {
            return Match::Unknown;
        }
        if self.name != other.name || self.array != other.array {
            return Match::No;
        }
        if self.args.len() != other.args.len() {
            return Match::Unknown;
        }
        let mut result = Match::Yes;
        for (wanted, actual) in self.args.iter().zip(&other.args) {
            match wanted.matches(actual) {
                Match::No => return Match::No,
                Match::Unknown => result = Match::Unknown,
                Match::Yes => {}
            }
        }
        result
    }

    fn label(&self) -> String {
        let mut out = simple(&self.name).to_owned();
        if !self.args.is_empty() {
            out += &format!("<{}>", self.args.iter().map(Self::label).collect::<Vec<_>>().join(", "));
        }
        out += &"[]".repeat(self.array);
        out
    }
}

impl Model {
    pub fn java(text: &str, file: &JavaFile, model: &FileModel) -> Self {
        let mut out = Self::default();
        for (i, decl) in file.types.iter().enumerate() {
            let variables = parameters(text, decl.at);
            out.types.push(Declaration {
                name: file.qualified(i),
                parameters: variables.clone(),
                supers: decl.supers().map(|s| Type::read(file, s, &variables)).collect(),
            });
            let class_range = range(text, decl.at, decl.at + decl.name.len());
            if model.beans.iter().any(|b| b.range == class_range) {
                out.beans.push((
                    class_range,
                    Type {
                        name: file.qualified(i),
                        args: variables
                            .iter()
                            .map(|v| Type { name: v.clone(), args: vec![], array: 0, unknown: true })
                            .collect(),
                        array: 0,
                        unknown: false,
                    },
                ));
            }
            for member in &decl.members {
                if member.kind == MemberKind::Method
                    && member.has(&["Bean"])
                    && let Some(ty) = &member.ty
                {
                    let at = range(text, member.at, member.at + member.name.len());
                    let variables = member_variables(text, member, &variables);
                    out.beans.push((at, Type::read(file, ty, &variables)));
                }
            }
            let mut record_injection = |ty: &TypeRef, variables: &[String]| {
                let target = unwrap(ty).0;
                let at = range(text, target.at, target.at + target.name.len());
                if model.injections.iter().any(|inj| inj.range == at) {
                    let mut signature = Type::read(file, target, variables);
                    // Arrays inject the element beans, just like List<T>.
                    signature.array = 0;
                    out.injections.push((at, signature));
                }
            };
            for component in &decl.components {
                record_injection(&component.ty, &variables);
            }
            for member in &decl.members {
                let variables = member_variables(text, member, &variables);
                for parameter in &member.params {
                    record_injection(&parameter.ty, &variables);
                }
                if member.kind == MemberKind::Field
                    && let Some(ty) = &member.ty
                {
                    record_injection(ty, &variables);
                }
            }
        }
        out
    }

    pub fn label(&self, at: Range) -> Option<String> {
        self.injections.iter().find(|(r, _)| *r == at).map(|(_, t)| t.label())
    }
}

pub struct Index<'a> {
    declarations: BTreeMap<&'a str, Vec<&'a Declaration>>,
    injections: BTreeMap<usize, &'a Type>,
    beans: BTreeMap<usize, &'a Type>,
}

impl<'a> Index<'a> {
    pub fn new(models: impl Iterator<Item = &'a FileModel>) -> Self {
        let mut out = Self { declarations: BTreeMap::new(), injections: BTreeMap::new(), beans: BTreeMap::new() };
        for model in models {
            for declaration in &model.generics.types {
                out.declarations.entry(simple(&declaration.name)).or_default().push(declaration);
            }
            for inj in &model.injections {
                if let Some((_, signature)) = model.generics.injections.iter().find(|(r, _)| *r == inj.range) {
                    out.injections.insert(std::ptr::from_ref(inj) as usize, signature);
                }
            }
            for bean in &model.beans {
                if let Some((_, signature)) = model.generics.beans.iter().find(|(r, _)| *r == bean.range) {
                    out.beans.insert(std::ptr::from_ref(bean) as usize, signature);
                }
            }
        }
        out
    }

    pub fn uncertain(&self, inj: &crate::Injection) -> bool {
        self.injections.get(&(std::ptr::from_ref(inj) as usize)).is_some_and(|t| !t.concrete())
    }

    pub fn matching(&self, inj: &crate::Injection, bean: &crate::Bean) -> Match {
        let Some(target) = self.injections.get(&(std::ptr::from_ref(inj) as usize)) else { return Match::Yes };
        if target.args.is_empty() && !target.unknown {
            return Match::Yes;
        }
        if !target.concrete() {
            return Match::Unknown;
        }
        let Some(ty) = self.beans.get(&(std::ptr::from_ref(bean) as usize)) else { return Match::Unknown };
        self.walk(ty, target, &mut BTreeSet::new(), 0)
    }

    fn reaches(&self, ty: &str, target: &str) -> bool {
        let mut todo = vec![ty];
        let mut seen = BTreeSet::new();
        while let Some(name) = todo.pop() {
            if simple(name) == simple(target) {
                return true;
            }
            if seen.insert(name) {
                todo.extend(
                    self.declarations
                        .get(simple(name))
                        .into_iter()
                        .flatten()
                        .flat_map(|d| &d.supers)
                        .map(|p| p.name.as_str()),
                );
            }
        }
        false
    }

    fn walk(&self, ty: &Type, target: &Type, seen: &mut BTreeSet<String>, depth: usize) -> Match {
        if depth >= 32 || !seen.insert(ty.name.clone()) {
            return Match::Unknown;
        }
        let result = if simple(&ty.name) == simple(&target.name) {
            if ty.args.len() != target.args.len() || !ty.concrete() {
                Match::Unknown
            } else {
                let mut wanted = target.clone();
                // The existing raw hierarchy uses simple names; argument identities remain qualified.
                wanted.name = ty.name.clone();
                wanted.matches(ty)
            }
        } else {
            let declarations = self.declarations.get(simple(&ty.name));
            if declarations.is_none_or(|d| d.len() != 1) {
                Match::Unknown
            } else {
                let declaration = declarations.unwrap()[0];
                let bindings: BTreeMap<_, _> =
                    declaration.parameters.iter().map(String::as_str).zip(&ty.args).collect();
                let mut result = Match::No;
                for parent in &declaration.supers {
                    if !self.reaches(&parent.name, &target.name) {
                        continue;
                    }
                    let parent = parent.substitute(&bindings);
                    match self.walk(&parent, target, seen, depth + 1) {
                        Match::Yes => {
                            result = Match::Yes;
                            break;
                        }
                        Match::Unknown => result = Match::Unknown,
                        Match::No => {}
                    }
                }
                result
            }
        };
        seen.remove(&ty.name);
        result
    }
}
