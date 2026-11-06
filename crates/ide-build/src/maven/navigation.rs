//! Source locations for definitions in a POM, using the same parent and profile
//! inputs as the build model, with the editor's current text for open POMs.

use super::{
    Element, Inherited, MAX_PARENT_DEPTH, RawDep, RawPom, Resolver, default_local_repo, interpolate, merge_by,
    read_dep, read_properties, read_raw, xml,
};
use crate::pom_edit::{element_name_range, offset_at, property_name_range};
use ide_core::{Pos, Range};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub enum PomDefinition {
    Unresolved,
    Target { path: PathBuf, range: Range, text: String },
}

enum Reference {
    Property(String),
    Dependency(RawDep),
}

struct Source {
    path: PathBuf,
    text: String,
    xml: Element,
    raw: RawPom,
}

impl Source {
    fn new(path: PathBuf, text: String, xml: Element) -> Option<Self> {
        (xml.name == "project").then(|| {
            let raw = read_raw(&xml);
            Self { path, text, xml, raw }
        })
    }

    fn sections(&self, resolver: &Resolver) -> Vec<&Element> {
        let profiles = self.xml.list("profiles", "profile");
        std::iter::once(&self.xml).chain(resolver.active(&self.raw.profiles).map(|(i, _)| profiles[i])).collect()
    }

    fn parent(&self, resolver: &Resolver, read: &impl Fn(&Path) -> Option<String>) -> Option<Self> {
        let parent = self.raw.parent.as_ref()?;
        let load = |path: PathBuf| {
            let path = dunce::canonicalize(path).ok()?;
            if !path.is_file() {
                return None;
            }
            let text = read(&path)?;
            let root = xml::parse(&text)?;
            Self::new(path, text, root)
        };
        let matches = |pom: &Source| {
            let version = pom.raw.version.as_deref().or_else(|| pom.raw.parent.as_ref().map(|p| p.version.as_str()));
            parent.matches_ga(&pom.raw) && (parent.version.is_empty() || version == Some(&parent.version))
        };
        if let Some(pom) = parent.relative_candidate(self.path.parent()?).and_then(load).filter(matches) {
            return Some(pom);
        }
        let path = resolver.repo_pom(&parent.group_id, &parent.artifact_id, &parent.version)?;
        load(path).filter(matches)
    }
}

fn property_at(text: &str, offset: usize) -> Option<(&str, std::ops::Range<usize>)> {
    let line_start = text[..offset].rfind('\n').map_or(0, |n| n + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |n| offset + n);
    let (start, _) =
        text[line_start..line_end].match_indices("${").take_while(|(n, _)| line_start + n <= offset).last()?;
    let start = line_start + start;
    let key_start = start + 2;
    let end = key_start + text[key_start..line_end].find('}')?;
    let key = &text[key_start..end];
    (offset <= end && !key.is_empty() && !key.contains(|c: char| c.is_whitespace() || "{}<>$".contains(c)))
        .then_some((key, start..end + 1))
}

fn dependency_at(section: &Element, offset: usize) -> Option<&Element> {
    section.list("dependencies", "dependency").into_iter().find(|d| {
        let at = |e: &Element| (e.start..e.end).contains(&offset);
        at(d)
            && ((d.start..d.inner_start).contains(&offset)
                || ["groupId", "artifactId", "type", "classifier"].into_iter().any(|tag| d.child(tag).is_some_and(at)))
    })
}

fn effective_key(dep: &RawDep, props: &HashMap<String, String>) -> Option<String> {
    let key = interpolate(&dep.key(), props);
    (!key.contains("${")).then_some(key)
}

fn managed_version(chain: &[Source], resolver: &Resolver, dep: &RawDep) -> Option<(usize, Range)> {
    let source = &chain[0];
    let mut inherited = Inherited::default();
    for pom in chain.iter().rev() {
        for section in pom.sections(resolver) {
            inherited.properties.extend(read_properties(section.child("properties")));
        }
    }
    inherited.group_id = chain.iter().find_map(|p| p.raw.group_id.clone());
    inherited.version = chain.iter().find_map(|p| p.raw.version.clone());
    let props = resolver.properties_for(&source.raw, &inherited, source.path.parent()?);
    let key = effective_key(dep, &props)?;
    let raw_key = dep.key();

    // Profile injection can replace the dependency at the caret with one
    // that already supplies a version.
    if source
        .sections(resolver)
        .into_iter()
        .rev()
        .find_map(|section| {
            section
                .list("dependencies", "dependency")
                .into_iter()
                .rev()
                .filter_map(read_dep)
                .find(|d| d.key() == raw_key)
        })
        .is_some_and(|d| d.version.is_some())
    {
        return None;
    }

    // Maven merges by raw key before interpolation. Active profiles replace
    // base entries, and the nearest POM replaces ancestors with the same key.
    let mut seen = HashSet::new();
    for (index, pom) in chain.iter().enumerate() {
        let mut managed = Vec::new();
        for section in pom.sections(resolver) {
            let entries: Vec<_> = section
                .list("dependencyManagement/dependencies", "dependency")
                .into_iter()
                .filter_map(|element| Some((read_dep(element)?, element)))
                .collect();
            merge_by(&mut managed, &entries, |(dep, _)| dep.key());
        }
        for (dep, element) in managed {
            if seen.insert(dep.key())
                && dep.version.is_some()
                && dep.scope.as_deref().map(|scope| interpolate(scope, &props)).as_deref() != Some("import")
                && effective_key(&dep, &props).as_ref() == Some(&key)
            {
                return Some((index, element_name_range(&pom.text, element.child("version")?)?));
            }
        }
    }
    None
}

/// `None` leaves navigation to other providers. A recognized Maven reference
/// without a definition follows the editor's normal declaration-not-found path.
pub fn pom_definition(
    path: PathBuf,
    text: String,
    pos: Pos,
    local_repo: Option<PathBuf>,
    profiles: &[String],
    read: impl Fn(&Path) -> Option<String>,
) -> Option<PomDefinition> {
    let offset = offset_at(&text, pos)?;
    let property = property_at(&text, offset);
    let mut in_text = false;
    let root = xml::parse_with_text(&text, |span| {
        in_text |= property.as_ref().is_some_and(|(_, token)| span.start <= token.start && span.end >= token.end);
    })?;
    let property = property
        .filter(|_| in_text && (root.inner_start..root.inner_end).contains(&offset))
        .map(|(name, _)| name.to_owned());
    let source = Source::new(path, text, root)?;
    let resolver = Resolver::new(local_repo.or_else(default_local_repo), profiles);
    let reference = match property {
        Some(name) => Reference::Property(name),
        None => {
            let entry = source.sections(&resolver).into_iter().find_map(|s| dependency_at(s, offset))?;
            let dep = read_dep(entry)?;
            if dep.version.is_some() {
                return None;
            }
            Reference::Dependency(dep)
        }
    };
    let mut chain = vec![source];
    while chain.len() <= MAX_PARENT_DEPTH {
        let Some(parent) = chain.last()?.parent(&resolver, &read) else { break };
        if chain.iter().any(|p| p.path == parent.path) {
            break;
        }
        chain.push(parent);
    }
    let target = match reference {
        Reference::Property(name) => chain.iter().enumerate().find_map(|(index, pom)| {
            pom.sections(&resolver)
                .into_iter()
                .rev()
                .find_map(|s| property_name_range(&pom.text, s, &name))
                .map(|range| (index, range))
        }),
        Reference::Dependency(dep) => managed_version(&chain, &resolver, &dep),
    };
    let Some((index, range)) = target else { return Some(PomDefinition::Unresolved) };
    let Source { path, text, .. } = chain.swap_remove(index);
    Some(PomDefinition::Target { path, range, text })
}
