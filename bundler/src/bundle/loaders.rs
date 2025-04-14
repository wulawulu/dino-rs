use super::modules::ModulePath;
use super::modules::ModuleSource;
use super::transpilers::TypeScript;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use colored::*;
use lazy_static::lazy_static;
use path_absolutize::*;
use regex::Regex;
use sha::sha1::Sha1;
use sha::utils::Digest;
use sha::utils::DigestExt;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use url::Url;

/// Defines the interface of a module loader.
pub trait ModuleLoader {
    fn load(&self, specifier: &str) -> Result<ModuleSource>;
    fn resolve(&self, base: Option<&str>, specifier: &str) -> Result<ModulePath>;
}

static EXTENSIONS: &[&str] = &["js", "ts", "json"];

#[derive(Default)]
pub struct FsModuleLoader;

impl FsModuleLoader {
    /// Transforms PathBuf into String.
    fn transform(&self, path: PathBuf) -> String {
        path.into_os_string().into_string().unwrap()
    }

    /// Checks if path is a JSON file.
    fn is_json_import(&self, path: &Path) -> bool {
        match path.extension() {
            Some(value) => value == "json",
            None => false,
        }
    }

    /// Wraps JSON data into an ES module (using v8's built in objects).
    fn wrap_json(&self, source: &str) -> String {
        format!("export default JSON.parse(`{source}`);")
    }

    /// Loads contents from a file.
    fn load_source(&self, path: &Path) -> Result<ModuleSource> {
        let source = fs::read_to_string(path)?;
        let source = match self.is_json_import(path) {
            true => self.wrap_json(source.as_str()),
            false => source,
        };

        Ok(source)
    }

    /// Loads import as file.
    fn load_as_file(&self, path: &Path) -> Result<ModuleSource> {
        // 1. Check if path is already a valid file.
        if path.is_file() {
            return self.load_source(path);
        }

        // 2. Check if we need to add an extension.
        if path.extension().is_none() {
            for ext in EXTENSIONS {
                let path = &path.with_extension(ext);
                if path.is_file() {
                    return self.load_source(path);
                }
            }
        }

        // 3. Bail out with an error.
        bail!(format!("Module not found \"{}\"", path.display()));
    }

    /// Loads import as directory using the 'index.[ext]' convention.
    fn load_as_directory(&self, path: &Path) -> Result<ModuleSource> {
        for ext in EXTENSIONS {
            let path = &path.join(format!("index.{ext}"));
            if path.is_file() {
                return self.load_source(path);
            }
        }
        bail!(format!("Module not found \"{}\"", path.display()));
    }
}

impl ModuleLoader for FsModuleLoader {
    fn resolve(&self, base: Option<&str>, specifier: &str) -> Result<ModulePath> {
        // Windows platform full path regex.
        lazy_static! {
            static ref WINDOWS_REGEX: Regex = Regex::new(r"^[a-zA-Z]:\\").unwrap();
        }

        // Resolve absolute import.
        if specifier.starts_with('/') || WINDOWS_REGEX.is_match(specifier) {
            return Ok(self.transform(Path::new(specifier).absolutize()?.to_path_buf()));
        }

        // Resolve relative import.
        let cwd = &env::current_dir().unwrap();
        let base = base.map(|v| Path::new(v).parent().unwrap()).unwrap_or(cwd);

        if specifier.starts_with("./") || specifier.starts_with("../") {
            return Ok(self.transform(base.join(specifier).absolutize()?.to_path_buf()));
        }

        bail!(format!("Module not found \"{specifier}\""));
    }

    fn load(&self, specifier: &str) -> Result<ModuleSource> {
        // Load source.
        let path = Path::new(specifier);
        let maybe_source = self
            .load_as_file(path)
            .or_else(|_| self.load_as_directory(path));

        // Append default extension (if none specified).
        let path = match path.extension() {
            Some(_) => path.into(),
            None => path.with_extension("js"),
        };

        let source = match maybe_source {
            Ok(source) => source,
            Err(_) => bail!(format!("Module not found \"{}\"", path.display())),
        };

        let path_extension = path.extension().unwrap().to_str().unwrap();
        let fname = path.to_str();

        // Use a preprocessor if necessary.
        match path_extension {
            "ts" => TypeScript::compile(fname, &source).map_err(|e| anyhow!(e.to_string())),
            _ => Ok(source),
        }
    }
}

lazy_static! {
    // Use local cache directory in development.
    pub static ref CACHE_DIR: PathBuf = if cfg!(debug_assertions) {
        PathBuf::from(".cache")
    } else {
        dirs::home_dir().unwrap().join(".dune/cache")
    };
}

#[derive(Default)]
/// Loader supporting URL imports.
pub struct UrlModuleLoader {
    // Ignores the cache and re-downloads the dependency.
    pub skip_cache: bool,
}

impl ModuleLoader for UrlModuleLoader {
    fn resolve(&self, base: Option<&str>, specifier: &str) -> Result<ModulePath> {
        // 1. Check if specifier is a valid URL.
        if let Ok(url) = Url::parse(specifier) {
            return Ok(url.into());
        }

        // 2. Check if the requester is a valid URL.
        if let Some(base) = base {
            if let Ok(base) = Url::parse(base) {
                let options = Url::options();
                let url = options.base_url(Some(&base));
                let url = url.parse(specifier)?;

                return Ok(url.as_str().to_string());
            }
        }

        // Possibly unreachable error.
        bail!("Base is not a valid URL");
    }

    fn load(&self, specifier: &str) -> Result<ModuleSource> {
        // Create the cache directory.
        if fs::create_dir_all(CACHE_DIR.as_path()).is_err() {
            bail!("Failed to create module caching directory");
        }

        // Hash URL using sha1.
        let hash = Sha1::default().digest(specifier.as_bytes()).to_hex();
        let module_path = CACHE_DIR.join(hash);

        if !self.skip_cache {
            // Check cache, and load file.
            if module_path.is_file() {
                let source = fs::read_to_string(&module_path).unwrap();
                return Ok(source);
            }
        }

        println!("{} {}", "Downloading".green(), specifier);

        // Download file and, save it to cache.
        let source = match ureq::get(specifier).call()?.into_string() {
            Ok(source) => source,
            Err(_) => bail!(format!("Module not found \"{specifier}\"")),
        };

        // Use a preprocessor if necessary.
        let source = if specifier.ends_with(".ts") {
            TypeScript::compile(Some(specifier), &source)?
        } else {
            source
        };

        fs::write(&module_path, &source)?;

        Ok(source)
    }
}
