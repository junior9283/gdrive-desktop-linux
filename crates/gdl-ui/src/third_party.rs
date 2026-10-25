//! Os softwares de terceiros que vão no app, para o "Sobre": as dependências (diretas e
//! indiretas, sem as de teste) dos programas do workspace, a partir do `cargo metadata`. O
//! `build.rs` também usa este arquivo, para gerar a lista no build.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde_json::{Value, json};

/// Pacotes do workspace que não vão no app (só os testes usam).
const TEST_ONLY: &[&str] = &["gdl-fake-drive"];

/// `[{ name, version, license }]`, em ordem de nome, a partir da saída do
/// `cargo metadata --format-version 1`. `license` vazio quando o pacote só traz um arquivo de
/// licença.
pub fn licenses(metadata: &Value) -> Value {
    let packages: HashMap<&str, &Value> = list(&metadata["packages"])
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();
    let members: HashSet<&str> = list(&metadata["workspace_members"])
        .filter_map(Value::as_str)
        .collect();
    let deps: HashMap<&str, Vec<&str>> = list(&metadata["resolve"]["nodes"])
        .filter_map(|node| {
            let shipped = list(&node["deps"])
                .filter(|dep| list(&dep["dep_kinds"]).any(|k| k["kind"].as_str() != Some("dev")))
                .filter_map(|dep| dep["pkg"].as_str())
                .collect();
            Some((node["id"].as_str()?, shipped))
        })
        .collect();
    let name = |id: &str| packages.get(id).and_then(|p| p["name"].as_str());
    let mut pending: Vec<&str> = members
        .iter()
        .copied()
        .filter(|id| name(id).is_some_and(|n| !TEST_ONLY.contains(&n)))
        .collect();
    let mut seen: HashSet<&str> = pending.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for &dep in deps.get(id).into_iter().flatten() {
            if seen.insert(dep) {
                pending.push(dep);
            }
        }
    }
    let entries: BTreeSet<(&str, &str, &str)> = seen
        .iter()
        .filter(|id| !members.contains(*id))
        .filter_map(|id| {
            let p = packages.get(id)?;
            Some((
                p["name"].as_str()?,
                p["version"].as_str()?,
                p["license"].as_str().unwrap_or_default(),
            ))
        })
        .collect();
    entries
        .into_iter()
        .map(|(name, version, license)| json!({ "name": name, "version": version, "license": license }))
        .collect()
}

fn list(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(id: &str, license: Option<&str>) -> Value {
        json!({ "id": id, "name": id, "version": "1.0.0", "license": license })
    }

    fn dep(pkg: &str, kind: Option<&str>) -> Value {
        json!({ "pkg": pkg, "dep_kinds": [{ "kind": kind }] })
    }

    #[test]
    fn walks_shipped_dependencies_only() {
        let metadata = json!({
            "packages": [
                package("app", None),
                package("gdl-fake-drive", None),
                package("serde", Some("MIT OR Apache-2.0")),
                package("indireta", Some("MIT")),
                package("de-teste", Some("MIT")),
                package("do-fake", Some("MIT")),
                package("so-arquivo", None),
            ],
            "workspace_members": ["app", "gdl-fake-drive"],
            "resolve": { "nodes": [
                { "id": "app", "deps": [
                    dep("serde", None), dep("de-teste", Some("dev")), dep("so-arquivo", Some("build")),
                ] },
                { "id": "serde", "deps": [dep("indireta", None)] },
                { "id": "gdl-fake-drive", "deps": [dep("do-fake", None)] },
            ] },
        });

        let names: Vec<(String, String)> = licenses(&metadata)
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["name"].as_str().unwrap().into(),
                    e["license"].as_str().unwrap().into(),
                )
            })
            .collect();

        assert_eq!(
            names,
            [
                ("indireta".into(), "MIT".into()),
                ("serde".into(), "MIT OR Apache-2.0".into()),
                ("so-arquivo".into(), String::new()),
            ]
        );
    }
}
