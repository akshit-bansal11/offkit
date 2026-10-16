//! The tool registry (`tools.json` at the repo root), compiled into the binary. It mirrors
//! `src/config/tools.ts`: the website switches to this file in M7; until then
//! `src/lib/tools.test.ts` guards drift between the two.

use serde::Deserialize;

const TOOLS_JSON: &str = include_str!("../../tools.json");

#[derive(Debug, Deserialize)]
pub struct Category {
    pub name: String,
    // `accent` is unused here: the app keeps one UI accent (see ui/theme.slint).
}

#[derive(Debug, Deserialize)]
pub struct Tool {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
}

#[derive(Debug, Deserialize)]
pub struct Registry {
    pub categories: Vec<Category>,
    pub tools: Vec<Tool>,
}

impl Registry {
    pub fn load() -> anyhow::Result<Self> {
        Ok(serde_json::from_str(TOOLS_JSON)?)
    }

    pub fn tool(&self, slug: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.slug == slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn loads_37_tools_in_9_categories_with_unique_slugs() {
        let r = Registry::load().unwrap();
        assert_eq!(r.categories.len(), 9);
        assert_eq!(r.tools.len(), 37);
        let slugs: HashSet<_> = r.tools.iter().map(|t| &t.slug).collect();
        assert_eq!(slugs.len(), 37);
        let cats: HashSet<_> = r.categories.iter().map(|c| c.name.as_str()).collect();
        assert!(r.tools.iter().all(|t| cats.contains(t.category.as_str())));
    }

    #[test]
    fn every_tool_icon_is_mapped() {
        let r = Registry::load().unwrap();
        let mapping = include_str!("../ui/icons/icons.slint");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/icons");
        for t in &r.tools {
            assert!(
                dir.join(format!("{}.svg", t.icon)).exists(),
                "missing {}.svg",
                t.icon
            );
            assert!(
                mapping.contains(&format!("name == \"{}\"", t.icon)),
                "icon {} not mapped in icons.slint",
                t.icon
            );
        }
    }
}
