//! Conservative static checks: only project-defined types, after the initial index pass.
use crate::World;
use openidea_plugin::{Diagnostic, Severity};

pub(super) fn problems(path: &str, world: &World, file: &spring_text::java::JavaFile) -> Vec<Diagnostic> {
    if !world.complete {
        return Vec::new();
    }
    if ["Service", "Component", "Repository", "Controller", "RestController", "Configuration", "Autowired", "Qualifier"]
        .iter()
        .any(|a| !file.from_spring(a))
    {
        return Vec::new();
    }
    let Some(model) = world.files.get(path) else { return Vec::new() };
    let mut out = Vec::new();
    for inj in &model.injections {
        let Some(owner) = inj.owner.and_then(|i| model.beans.get(i)) else { continue };
        if inj.multiple || inj.optional || inj.ty.is_empty() || owner.conditional || !owner.profiles.is_empty() {
            continue;
        }
        // Never diagnose library/auto-configuration types or ambiguous simple type names.
        let types: Vec<_> = world.files.values().flat_map(|m| &m.types).filter(|t| t.name == inj.ty).collect();
        if types.len() != 1 {
            continue;
        }
        let suffix = format!(".{}", inj.ty);
        if file.imports.iter().any(|i| !i.is_static && i.path.ends_with(&suffix) && i.path != types[0].qualified) {
            continue;
        }
        let possible: Vec<_> = world.beans().filter(|(_, b)| !b.is_abstract && world.is_a(&b.ty, &inj.ty)).collect();
        if possible.iter().any(|(_, b)| b.conditional || !b.profiles.is_empty()) {
            continue;
        }
        let found = world.candidates(inj);
        let (code, message) = match found.len() {
            0 => (
                "missing-bean",
                format!(
                    "No project Bean matches '{}'{}; check its registration or qualifier. Library and programmatic registrations are not indexed.",
                    inj.ty,
                    inj.qualifier.as_ref().map_or(String::new(), |q| format!(" with qualifier '{q}'"))
                ),
            ),
            1 => continue,
            _ => (
                "ambiguous-bean",
                format!(
                    "Multiple project Beans match '{}': {}. Select one with @Qualifier or @Primary.",
                    inj.ty,
                    found.iter().map(|(_, b)| b.name.as_str()).collect::<Vec<_>>().join(", ")
                ),
            ),
        };
        out.push(Diagnostic { code: Some(code.into()), ..Diagnostic::new(inj.range, Severity::Warning, message) });
    }
    out
}
