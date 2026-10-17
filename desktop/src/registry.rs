//! The tool registry (`tools.json` at the repo root), compiled into the binary. It mirrors
//! `src/config/tools.ts`: the website switches to this file in M7; until then
//! `src/lib/tools.test.ts` guards drift between the two.

use serde::Deserialize;

const TOOLS_JSON: &str = include_str!("../../tools.json");

/// Where a tool runs. A tool can be web-only, desktop-only or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Web,
    Desktop,
}

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
    pub platforms: Vec<Platform>,
}

#[derive(Debug, Deserialize)]
pub struct Registry {
    #[serde(rename = "siteUrl")]
    pub site_url: String,
    pub categories: Vec<Category>,
    pub tools: Vec<Tool>,
}

impl Tool {
    /// A tool that does not list "desktop" has no view here; the app sends the user to the site.
    pub fn on_desktop(&self) -> bool {
        self.platforms.contains(&Platform::Desktop)
    }
}

/// The page for a web tool: `{site_url}/{slug}`. Only an https site URL is ever opened.
pub fn web_url(site_url: &str, slug: &str) -> Option<String> {
    site_url
        .starts_with("https://")
        .then(|| format!("{}/{slug}", site_url.trim_end_matches('/')))
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

    #[test]
    fn every_tool_lists_a_platform() {
        let r = Registry::load().unwrap();
        for t in &r.tools {
            assert!(!t.platforms.is_empty(), "{} has no platforms", t.slug);
        }
    }

    #[test]
    fn parses_platforms_and_the_site_url() {
        let r = Registry::load().unwrap();
        assert!(r.site_url.starts_with("https://"));
        assert_eq!(
            r.tool("base64").unwrap().platforms,
            [Platform::Web, Platform::Desktop]
        );

        let web_only: Tool = serde_json::from_str(
            r#"{"slug":"x","name":"X","description":"","category":"","icon":"","platforms":["web"]}"#,
        )
        .unwrap();
        assert!(!web_only.on_desktop());
        assert!(r.tool("base64").unwrap().on_desktop());
    }

    #[test]
    fn web_url_joins_the_slug_and_refuses_anything_but_https() {
        assert_eq!(
            web_url("https://example.com", "base64").as_deref(),
            Some("https://example.com/base64")
        );
        assert_eq!(
            web_url("https://example.com/", "base64").as_deref(),
            Some("https://example.com/base64")
        );
        assert_eq!(web_url("http://example.com", "base64"), None);
        assert_eq!(web_url("file:///etc", "base64"), None);
    }
}
