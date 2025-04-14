use std::{collections::HashMap, env, path::Path};

use anyhow::{Result, anyhow};
use lazy_static::lazy_static;
use regex::Regex;
use serde_json::Value;
use url::Url;

use super::loaders::{CoreModuleLoader, FsModuleLoader, ModuleLoader, UrlModuleLoader};

pub type ModulePath = String;
pub type ModuleSource = String;
/// A single import mapping (specifier, target).
type ImportMapEntry = (String, String);

/// Key-Value entries representing WICG import-maps.
#[derive(Debug, Clone)]
pub struct ImportMap {
    map: Vec<ImportMapEntry>,
}

lazy_static! {
    pub static ref CORE_MODULES: HashMap<&'static str, &'static str> = {
        let modules = vec![
            ("console", include_str!("./js/console.js")),
            ("events", include_str!("./js/events.js")),
            ("process", include_str!("./js/process.js")),
            ("timers", include_str!("./js/timers.js")),
            ("assert", include_str!("./js/assert.js")),
            ("util", include_str!("./js/util.js")),
            ("fs", include_str!("./js/fs.js")),
            ("perf_hooks", include_str!("./js/perf-hooks.js")),
            ("colors", include_str!("./js/colors.js")),
            ("dns", include_str!("./js/dns.js")),
            ("net", include_str!("./js/net.js")),
            ("test", include_str!("./js/test.js")),
            ("stream", include_str!("./js/stream.js")),
            ("http", include_str!("./js/http.js")),
            ("@web/abort", include_str!("./js/abort-controller.js")),
            ("@web/text_encoding", include_str!("./js/text-encoding.js")),
            ("@web/clone", include_str!("./js/structured-clone.js")),
            ("@web/fetch", include_str!("./js/fetch.js")),
        ];
        HashMap::from_iter(modules.into_iter())
    };
}

lazy_static! {
    // Windows absolute path regex validator.
    static ref WINDOWS_REGEX: Regex = Regex::new(r"^[a-zA-Z]:\\").unwrap();
    // URL regex validator (string begins with http:// or https://).
    static ref URL_REGEX: Regex = Regex::new(r"^(http|https)://").unwrap();
}

/// Loads an import using the appropriate loader.
pub fn load_import(specifier: &str, skip_cache: bool) -> Result<ModuleSource> {
    // Look the params and choose a loader.
    let loader: Box<dyn ModuleLoader> = match (
        CORE_MODULES.contains_key(specifier),
        WINDOWS_REGEX.is_match(specifier),
        Url::parse(specifier).is_ok(),
    ) {
        (true, _, _) => Box::new(CoreModuleLoader),
        (_, true, _) => Box::new(FsModuleLoader),
        (_, _, true) => Box::new(UrlModuleLoader { skip_cache }),
        _ => Box::new(FsModuleLoader),
    };

    // Load module.
    loader.load(specifier)
}

/// Resolves an import using the appropriate loader.
pub fn resolve_import(
    base: Option<&str>,
    specifier: &str,
    ignore_core_modules: bool,
    import_map: Option<ImportMap>,
) -> Result<ModulePath> {
    // Use import-maps if available.
    let specifier = match import_map {
        Some(map) => map.lookup(specifier).unwrap_or_else(|| specifier.into()),
        None => specifier.into(),
    };

    // Look the params and choose a loader.
    let loader: Box<dyn ModuleLoader> = {
        let is_core_module_import = CORE_MODULES.contains_key(specifier.as_str());
        let is_url_import = URL_REGEX.is_match(&specifier)
            || match base {
                Some(base) => URL_REGEX.is_match(base),
                None => false,
            };

        match (is_core_module_import, is_url_import) {
            (true, _) if !ignore_core_modules => Box::new(CoreModuleLoader),
            (_, true) => Box::<UrlModuleLoader>::default(),
            _ => Box::new(FsModuleLoader),
        }
    };

    // Resolve module.
    loader.resolve(base, &specifier)
}

impl ImportMap {
    /// Creates an ImportMap from JSON text.
    pub fn parse_from_json(text: &str) -> Result<ImportMap> {
        // Parse JSON string into serde value.
        let json: Value = serde_json::from_str(text)?;
        let imports = json["imports"].to_owned();

        if imports.is_null() || !imports.is_object() {
            return Err(anyhow!("Import map's 'imports' must be an object"));
        }

        let map: HashMap<String, String> = serde_json::from_value(imports)?;
        let mut map: Vec<ImportMapEntry> = Vec::from_iter(map);

        // Note: We're sorting the imports because we need to support "Packages"
        // via trailing slashes, so the lengthier mapping should always be selected.
        //
        // https://github.com/WICG/import-maps#packages-via-trailing-slashes

        map.sort_by(|a, b| b.0.cmp(&a.0));

        Ok(ImportMap { map })
    }

    /// Tries to match a specifier against an import-map entry.
    pub fn lookup(&self, specifier: &str) -> Option<String> {
        // Find a mapping if exists.
        let (base, mut target) = match self.map.iter().find(|(k, _)| specifier.starts_with(k)) {
            Some(mapping) => mapping.to_owned(),
            None => return None,
        };

        // The following code treats "./" as an alias for the CWD.
        if target.starts_with("./") {
            let cwd = env::current_dir().unwrap().to_string_lossy().to_string();
            target = target.replacen('.', &cwd, 1);
        }

        // Note: The reason we need this additional check below with the specifier's
        // extension (if exists) is to be able to support extension-less imports.
        //
        // https://github.com/WICG/import-maps#extension-less-imports

        match Path::new(specifier).extension() {
            Some(ext) => match Path::new(specifier) == Path::new(&base).with_extension(ext) {
                false => Some(specifier.replacen(&base, &target, 1)),
                _ => None,
            },
            None => Some(specifier.replacen(&base, &target, 1)),
        }
    }
}
