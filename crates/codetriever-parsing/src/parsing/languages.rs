//! Language-specific configurations for code parsing
//!
//! This module centralizes all language-specific parsing configurations,
//! including Tree-sitter language definitions, query patterns, and parsing rules.

use lazy_static::lazy_static;
use std::collections::HashMap;
use tree_sitter::Language;

/// Configuration for a specific programming language
#[derive(Debug, Clone)]
pub struct LanguageConfig {
    /// The language identifier (e.g., "rust", "python")
    pub id: &'static str,
    /// File extensions associated with this language
    pub extensions: &'static [&'static str],
    /// Tree-sitter language parser
    pub tree_sitter_language: Option<Language>,
    /// Tree-sitter query for extracting code elements
    pub tree_sitter_query: Option<&'static str>,
    /// Keywords that indicate function definitions
    pub function_keywords: &'static [&'static str],
    /// Keywords that indicate class/type definitions
    pub class_keywords: &'static [&'static str],
    /// Whether the language uses braces for blocks
    pub uses_braces: bool,
    /// Whether the language uses indentation for blocks (like Python)
    pub uses_indentation: bool,
}

impl LanguageConfig {
    /// Creates a new language configuration
    pub const fn new(id: &'static str) -> Self {
        Self {
            id,
            extensions: &[],
            tree_sitter_language: None,
            tree_sitter_query: None,
            function_keywords: &[],
            class_keywords: &[],
            uses_braces: true,
            uses_indentation: false,
        }
    }

    /// Builder method to set extensions
    pub const fn with_extensions(mut self, extensions: &'static [&'static str]) -> Self {
        self.extensions = extensions;
        self
    }

    /// Builder method to set tree-sitter language
    pub fn with_tree_sitter(mut self, language: Language, query: &'static str) -> Self {
        self.tree_sitter_language = Some(language);
        self.tree_sitter_query = Some(query);
        self
    }

    /// Builder method to set function keywords
    pub const fn with_function_keywords(mut self, keywords: &'static [&'static str]) -> Self {
        self.function_keywords = keywords;
        self
    }

    /// Builder method to set class keywords
    pub const fn with_class_keywords(mut self, keywords: &'static [&'static str]) -> Self {
        self.class_keywords = keywords;
        self
    }

    /// Builder method to set block style
    pub const fn with_block_style(mut self, uses_braces: bool, uses_indentation: bool) -> Self {
        self.uses_braces = uses_braces;
        self.uses_indentation = uses_indentation;
        self
    }
}

lazy_static! {
    /// Registry of all supported language configurations
    pub static ref LANGUAGE_REGISTRY: HashMap<&'static str, LanguageConfig> = {
        let mut registry = HashMap::new();

        // Rust configuration
        registry.insert(
            "rust",
            LanguageConfig::new("rust")
                .with_extensions(&["rs"])
                .with_tree_sitter(
                    tree_sitter_rust::LANGUAGE.into(),
                    r#"
                    (function_item) @function
                    (impl_item) @impl
                    (struct_item) @struct
                    (enum_item) @enum
                    (trait_item) @trait
                    (mod_item) @module
                    "#,
                )
                .with_function_keywords(&["fn ", "pub fn", "pub(crate) fn", "async fn"])
                .with_class_keywords(&["struct ", "enum ", "trait ", "impl "])
                .with_block_style(true, false),
        );

        // Python configuration
        registry.insert(
            "python",
            LanguageConfig::new("python")
                .with_extensions(&["py", "pyi"])
                .with_tree_sitter(
                    tree_sitter_python::LANGUAGE.into(),
                    r#"
                    (function_definition) @function
                    (class_definition) @class
                    "#,
                )
                .with_function_keywords(&["def ", "async def "])
                .with_class_keywords(&["class "])
                .with_block_style(false, true),
        );

        // JavaScript configuration
        registry.insert(
            "javascript",
            LanguageConfig::new("javascript")
                .with_extensions(&["js", "mjs", "cjs"])
                .with_tree_sitter(
                    tree_sitter_javascript::LANGUAGE.into(),
                    r#"
                    (function_declaration) @function
                    (arrow_function) @arrow_function
                    (class_declaration) @class
                    (method_definition) @method
                    "#,
                )
                .with_function_keywords(&["function ", "async function ", "const ", "let ", "var "])
                .with_class_keywords(&["class "])
                .with_block_style(true, false),
        );

        // TypeScript configuration
        registry.insert(
            "typescript",
            LanguageConfig::new("typescript")
                .with_extensions(&["ts", "mts", "cts"])
                .with_tree_sitter(
                    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                    r#"
                    (function_declaration) @function
                    (arrow_function) @arrow_function
                    (class_declaration) @class
                    (method_definition) @method
                    (interface_declaration) @interface
                    "#,
                )
                .with_function_keywords(&["function ", "async function ", "const ", "let ", "var "])
                .with_class_keywords(&["class ", "interface ", "type ", "enum "])
                .with_block_style(true, false),
        );

        // TypeScript JSX configuration
        registry.insert(
            "tsx",
            LanguageConfig::new("tsx")
                .with_extensions(&["tsx"])
                .with_tree_sitter(
                    tree_sitter_typescript::LANGUAGE_TSX.into(),
                    r#"
                    (function_declaration) @function
                    (arrow_function) @arrow_function
                    (class_declaration) @class
                    (method_definition) @method
                    "#,
                )
                .with_function_keywords(&["function ", "async function ", "const ", "let ", "var "])
                .with_class_keywords(&["class ", "interface "])
                .with_block_style(true, false),
        );

        // Go configuration
        registry.insert(
            "go",
            LanguageConfig::new("go")
                .with_extensions(&["go"])
                .with_tree_sitter(
                    tree_sitter_go::LANGUAGE.into(),
                    r#"
                    (function_declaration) @function
                    (method_declaration) @method
                    (type_declaration) @type
                    "#,
                )
                .with_function_keywords(&["func "])
                .with_class_keywords(&["type ", "struct ", "interface "])
                .with_block_style(true, false),
        );

        // Java configuration
        registry.insert(
            "java",
            LanguageConfig::new("java")
                .with_extensions(&["java"])
                .with_tree_sitter(
                    tree_sitter_java::LANGUAGE.into(),
                    r#"
                    (class_declaration) @class
                    (interface_declaration) @interface
                    (method_declaration) @method
                    "#,
                )
                .with_function_keywords(&["public ", "private ", "protected ", "static "])
                .with_class_keywords(&["class ", "interface ", "enum "])
                .with_block_style(true, false),
        );

        // C configuration
        registry.insert(
            "c",
            LanguageConfig::new("c")
                .with_extensions(&["c", "h"])
                .with_tree_sitter(
                    tree_sitter_c::LANGUAGE.into(),
                    r#"
                    (function_definition) @function
                    (struct_specifier) @struct
                    "#,
                )
                .with_function_keywords(&["int ", "void ", "char ", "float ", "double ", "static "])
                .with_class_keywords(&["struct ", "typedef ", "enum ", "union "])
                .with_block_style(true, false),
        );

        // C++ configuration
        registry.insert(
            "cpp",
            LanguageConfig::new("cpp")
                .with_extensions(&["cpp", "cxx", "cc", "c++", "hpp", "hxx", "hh", "h++"])
                .with_tree_sitter(
                    tree_sitter_cpp::LANGUAGE.into(),
                    r#"
                    (function_definition) @function
                    (struct_specifier) @struct
                    (class_specifier) @class
                    "#,
                )
                .with_function_keywords(&["void ", "int ", "bool ", "auto ", "template "])
                .with_class_keywords(&["class ", "struct ", "namespace ", "template "])
                .with_block_style(true, false),
        );

        // C# configuration
        registry.insert(
            "csharp",
            LanguageConfig::new("csharp")
                .with_extensions(&["cs", "csx"])
                .with_tree_sitter(
                    tree_sitter_c_sharp::LANGUAGE.into(),
                    r#"
                    (class_declaration) @class
                    (interface_declaration) @interface
                    (method_declaration) @method
                    (property_declaration) @property
                    "#,
                )
                .with_function_keywords(&["public ", "private ", "protected ", "internal ", "static ", "async ", "override ", "virtual "])
                .with_class_keywords(&["class ", "interface ", "struct ", "enum ", "record "])
                .with_block_style(true, false),
        );

        // Bash configuration
        registry.insert(
            "bash",
            LanguageConfig::new("bash")
                .with_extensions(&["sh", "bash", "zsh", "fish", "ksh"])
                .with_tree_sitter(
                    tree_sitter_bash::LANGUAGE.into(),
                    r#"
                    (function_definition) @function
                    (command) @command
                    (if_statement) @conditional
                    (for_statement) @loop
                    (while_statement) @loop
                    (case_statement) @conditional
                    "#,
                )
                .with_function_keywords(&["function ", "() {"])
                .with_class_keywords(&[])
                .with_block_style(true, false),
        );

        // HTML configuration
        registry.insert(
            "html",
            LanguageConfig::new("html")
                .with_extensions(&["html", "htm", "xhtml"])
                .with_tree_sitter(
                    tree_sitter_html::LANGUAGE.into(),
                    r#"
                    (element) @element
                    (script_element) @script
                    (style_element) @style
                    "#,
                )
                .with_function_keywords(&[])
                .with_class_keywords(&[])
                .with_block_style(false, false),
        );

        // PowerShell configuration
        registry.insert(
            "powershell",
            LanguageConfig::new("powershell")
                .with_extensions(&["ps1", "psm1", "psd1", "ps1xml", "pssc", "psc1", "cdxml"])
                .with_tree_sitter(
                    tree_sitter_powershell::LANGUAGE.into(),
                    r#"
                    (function_statement) @function
                    (class_statement) @class
                    (enum_statement) @enum
                    (workflow_statement) @workflow
                    "#,
                )
                .with_function_keywords(&["function ", "filter ", "workflow "])
                .with_class_keywords(&["class ", "enum "])
                .with_block_style(true, false),
        );

        // SQL configuration
        registry.insert(
            "sql",
            LanguageConfig::new("sql")
                .with_extensions(&["sql", "ddl", "dml", "dcl", "pgsql", "plsql", "tsql", "psql"])
                .with_tree_sitter(
                    tree_sitter_sequel::LANGUAGE.into(),
                    r#"
                    (function_definition) @function
                    (procedure_definition) @procedure
                    (table_definition) @table
                    (view_definition) @view
                    (index_definition) @index
                    "#,
                )
                .with_function_keywords(&["CREATE FUNCTION", "CREATE OR REPLACE FUNCTION", "CREATE PROCEDURE", "CREATE OR REPLACE PROCEDURE"])
                .with_class_keywords(&["CREATE TABLE", "CREATE VIEW", "CREATE INDEX", "CREATE TRIGGER"])
                .with_block_style(false, false),
        );

        // JSON configuration
        registry.insert(
            "json",
            LanguageConfig::new("json")
                .with_extensions(&["json", "jsonc"])
                .with_tree_sitter(
                    tree_sitter_json::LANGUAGE.into(),
                    r#"
                    (object) @object
                    (array) @array
                    "#,
                )
                .with_function_keywords(&[])
                .with_class_keywords(&[])
                .with_block_style(true, false),
        );

        // XML configuration
        registry.insert(
            "xml",
            LanguageConfig::new("xml")
                .with_extensions(&["xml", "xsd", "xsl", "svg"])
                .with_tree_sitter(
                    tree_sitter_xml::LANGUAGE_XML.into(),
                    r#"
                    (element) @element
                    "#,
                )
                .with_function_keywords(&[])
                .with_class_keywords(&[])
                .with_block_style(false, false),
        );

        // YAML configuration
        registry.insert(
            "yaml",
            LanguageConfig::new("yaml")
                .with_extensions(&["yaml", "yml"])
                .with_tree_sitter(
                    tree_sitter_yaml::LANGUAGE.into(),
                    r#"
                    (document) @document
                    (block_mapping) @mapping
                    (block_sequence) @sequence
                    (flow_mapping) @flow_mapping
                    (flow_sequence) @flow_sequence
                    "#,
                )
                .with_function_keywords(&[])
                .with_class_keywords(&[])
                .with_block_style(false, true),
        );

        registry
    };

    /// Map of file extensions to language IDs
    pub static ref EXTENSION_MAP: HashMap<&'static str, &'static str> = {
        let mut map = HashMap::new();

        for (lang_id, config) in LANGUAGE_REGISTRY.iter() {
            for ext in config.extensions {
                map.insert(*ext, *lang_id);
            }
        }

        map
    };
}

/// Gets a language configuration by ID
pub fn get_language_config(language_id: &str) -> Option<&'static LanguageConfig> {
    LANGUAGE_REGISTRY.get(language_id)
}

/// Gets a language ID from a file extension
pub fn get_language_from_extension(extension: &str) -> Option<&'static str> {
    EXTENSION_MAP.get(extension).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_registry_initialization() {
        assert!(!LANGUAGE_REGISTRY.is_empty());
        assert!(LANGUAGE_REGISTRY.contains_key("rust"));
        assert!(LANGUAGE_REGISTRY.contains_key("python"));
        assert!(LANGUAGE_REGISTRY.contains_key("javascript"));
    }

    #[test]
    fn test_extension_mapping() {
        assert_eq!(get_language_from_extension("rs"), Some("rust"));
        assert_eq!(get_language_from_extension("py"), Some("python"));
        assert_eq!(get_language_from_extension("js"), Some("javascript"));
        assert_eq!(get_language_from_extension("ts"), Some("typescript"));
        assert_eq!(get_language_from_extension("go"), Some("go"));
    }

    #[test]
    fn test_language_config_properties() {
        let rust_config = get_language_config("rust").unwrap();
        assert_eq!(rust_config.id, "rust");
        assert!(rust_config.uses_braces);
        assert!(!rust_config.uses_indentation);
        assert!(rust_config.tree_sitter_language.is_some());
        assert!(rust_config.tree_sitter_query.is_some());

        let python_config = get_language_config("python").unwrap();
        assert_eq!(python_config.id, "python");
        assert!(!python_config.uses_braces);
        assert!(python_config.uses_indentation);
    }

    #[test]
    fn test_function_keywords() {
        let rust_config = get_language_config("rust").unwrap();
        assert!(rust_config.function_keywords.contains(&"fn "));
        assert!(rust_config.function_keywords.contains(&"async fn"));

        let python_config = get_language_config("python").unwrap();
        assert!(python_config.function_keywords.contains(&"def "));
        assert!(python_config.function_keywords.contains(&"async def "));
    }

    #[test]
    fn test_class_keywords() {
        let rust_config = get_language_config("rust").unwrap();
        assert!(rust_config.class_keywords.contains(&"struct "));
        assert!(rust_config.class_keywords.contains(&"impl "));

        let java_config = get_language_config("java").unwrap();
        assert!(java_config.class_keywords.contains(&"class "));
        assert!(java_config.class_keywords.contains(&"interface "));
    }

    #[test]
    fn test_tree_sitter_queries() {
        let rust_config = get_language_config("rust").unwrap();
        let query = rust_config.tree_sitter_query.unwrap();
        assert!(query.contains("function_item"));
        assert!(query.contains("impl_item"));
        assert!(query.contains("struct_item"));
    }

    #[test]
    fn test_bash_configuration() {
        let bash_config = get_language_config("bash").unwrap();
        assert_eq!(bash_config.id, "bash");
        assert!(bash_config.extensions.contains(&"sh"));
        assert!(bash_config.extensions.contains(&"bash"));
        assert!(bash_config.tree_sitter_language.is_some());
    }

    #[test]
    fn test_html_configuration() {
        let html_config = get_language_config("html").unwrap();
        assert_eq!(html_config.id, "html");
        assert!(html_config.extensions.contains(&"html"));
        assert!(html_config.extensions.contains(&"htm"));
        assert!(!html_config.uses_braces);
        assert!(!html_config.uses_indentation);
    }

    #[test]
    fn test_powershell_configuration() {
        let ps_config = get_language_config("powershell").unwrap();
        assert_eq!(ps_config.id, "powershell");
        assert!(ps_config.extensions.contains(&"ps1"));
        assert!(ps_config.extensions.contains(&"psm1"));
        assert!(ps_config.extensions.contains(&"psd1"));
        assert!(ps_config.tree_sitter_language.is_some());
        assert!(ps_config.function_keywords.contains(&"function "));
        assert!(ps_config.class_keywords.contains(&"class "));
        assert!(ps_config.uses_braces);
        assert!(!ps_config.uses_indentation);
    }

    #[test]
    fn test_sql_configuration() {
        let sql_config = get_language_config("sql").unwrap();
        assert_eq!(sql_config.id, "sql");
        assert!(sql_config.extensions.contains(&"sql"));
        assert!(sql_config.extensions.contains(&"pgsql"));
        assert!(sql_config.extensions.contains(&"tsql"));
        assert!(sql_config.tree_sitter_language.is_some());
        assert!(sql_config.function_keywords.contains(&"CREATE FUNCTION"));
        assert!(sql_config.class_keywords.contains(&"CREATE TABLE"));
        assert!(!sql_config.uses_braces);
        assert!(!sql_config.uses_indentation);
    }

    #[test]
    fn test_go_configuration() {
        let go_config = get_language_config("go").unwrap();
        assert_eq!(go_config.id, "go");
        assert!(go_config.extensions.contains(&"go"));
        assert!(go_config.tree_sitter_language.is_some());
        assert!(go_config.function_keywords.contains(&"func "));
        assert!(go_config.class_keywords.contains(&"type "));
        assert!(go_config.class_keywords.contains(&"struct "));
        assert!(go_config.uses_braces);
        assert!(!go_config.uses_indentation);
    }

    #[test]
    fn test_typescript_configuration() {
        let ts_config = get_language_config("typescript").unwrap();
        assert_eq!(ts_config.id, "typescript");
        assert!(ts_config.extensions.contains(&"ts"));
        assert!(ts_config.extensions.contains(&"mts"));
        assert!(ts_config.tree_sitter_language.is_some());
        assert!(ts_config.function_keywords.contains(&"function "));
        assert!(ts_config.class_keywords.contains(&"interface "));
        assert!(ts_config.class_keywords.contains(&"type "));
        assert!(ts_config.uses_braces);
        assert!(!ts_config.uses_indentation);
    }

    #[test]
    fn test_tsx_configuration() {
        let tsx_config = get_language_config("tsx").unwrap();
        assert_eq!(tsx_config.id, "tsx");
        assert!(tsx_config.extensions.contains(&"tsx"));
        assert!(tsx_config.tree_sitter_language.is_some());
        assert!(tsx_config.uses_braces);
        assert!(!tsx_config.uses_indentation);
    }

    #[test]
    fn test_csharp_configuration() {
        let cs_config = get_language_config("csharp").unwrap();
        assert_eq!(cs_config.id, "csharp");
        assert!(cs_config.extensions.contains(&"cs"));
        assert!(cs_config.extensions.contains(&"csx"));
        assert!(cs_config.tree_sitter_language.is_some());
        assert!(cs_config.function_keywords.contains(&"public "));
        assert!(cs_config.function_keywords.contains(&"async "));
        assert!(cs_config.class_keywords.contains(&"class "));
        assert!(cs_config.class_keywords.contains(&"record "));
        assert!(cs_config.uses_braces);
        assert!(!cs_config.uses_indentation);
    }

    #[test]
    fn test_cpp_configuration() {
        let cpp_config = get_language_config("cpp").unwrap();
        assert_eq!(cpp_config.id, "cpp");
        assert!(cpp_config.extensions.contains(&"cpp"));
        assert!(cpp_config.extensions.contains(&"hpp"));
        assert!(cpp_config.extensions.contains(&"cxx"));
        assert!(cpp_config.tree_sitter_language.is_some());
        assert!(cpp_config.function_keywords.contains(&"void "));
        assert!(cpp_config.class_keywords.contains(&"class "));
        assert!(cpp_config.class_keywords.contains(&"namespace "));
        assert!(cpp_config.uses_braces);
        assert!(!cpp_config.uses_indentation);
    }

    #[test]
    fn test_c_configuration() {
        let c_config = get_language_config("c").unwrap();
        assert_eq!(c_config.id, "c");
        assert!(c_config.extensions.contains(&"c"));
        assert!(c_config.extensions.contains(&"h"));
        assert!(c_config.tree_sitter_language.is_some());
        assert!(c_config.function_keywords.contains(&"int "));
        assert!(c_config.function_keywords.contains(&"void "));
        assert!(c_config.class_keywords.contains(&"struct "));
        assert!(c_config.class_keywords.contains(&"typedef "));
        assert!(c_config.uses_braces);
        assert!(!c_config.uses_indentation);
    }

    #[test]
    fn test_json_configuration() {
        let json_config = get_language_config("json").unwrap();
        assert_eq!(json_config.id, "json");
        assert!(json_config.extensions.contains(&"json"));
        assert!(json_config.extensions.contains(&"jsonc"));
        assert!(json_config.tree_sitter_language.is_some());
        assert!(json_config.function_keywords.is_empty());
        assert!(json_config.class_keywords.is_empty());
        assert!(json_config.uses_braces);
        assert!(!json_config.uses_indentation);
    }

    #[test]
    fn test_xml_configuration() {
        let xml_config = get_language_config("xml").unwrap();
        assert_eq!(xml_config.id, "xml");
        assert!(xml_config.extensions.contains(&"xml"));
        assert!(xml_config.extensions.contains(&"svg"));
        assert!(xml_config.tree_sitter_language.is_some());
        assert!(xml_config.function_keywords.is_empty());
        assert!(xml_config.class_keywords.is_empty());
        assert!(!xml_config.uses_braces);
        assert!(!xml_config.uses_indentation);
    }

    #[test]
    fn test_yaml_configuration() {
        let yaml_config = get_language_config("yaml").unwrap();
        assert_eq!(yaml_config.id, "yaml");
        assert!(yaml_config.extensions.contains(&"yaml"));
        assert!(yaml_config.extensions.contains(&"yml"));
        assert!(yaml_config.tree_sitter_language.is_some());
        assert!(yaml_config.function_keywords.is_empty());
        assert!(yaml_config.class_keywords.is_empty());
        assert!(!yaml_config.uses_braces);
        assert!(yaml_config.uses_indentation); // YAML uses indentation
    }

    #[test]
    fn test_all_languages_have_config() {
        // List of all languages we support
        let expected_languages = vec![
            "rust",
            "python",
            "javascript",
            "typescript",
            "tsx",
            "go",
            "java",
            "c",
            "cpp",
            "csharp",
            "bash",
            "html",
            "powershell",
            "sql",
            "json",
            "xml",
            "yaml",
        ];

        for lang in expected_languages {
            assert!(
                LANGUAGE_REGISTRY.contains_key(lang),
                "Missing configuration for language: {lang}"
            );

            let config = get_language_config(lang).unwrap();
            assert_eq!(config.id, lang);
            assert!(
                !config.extensions.is_empty(),
                "Language {lang} has no extensions"
            );
            assert!(
                config.tree_sitter_language.is_some(),
                "Language {lang} has no tree-sitter"
            );
            assert!(
                config.tree_sitter_query.is_some(),
                "Language {lang} has no query"
            );
        }
    }

    #[test]
    fn test_extension_uniqueness() {
        // Check that each extension maps to exactly one language
        type ExtensionMap<'a> = HashMap<&'a str, Vec<&'a str>>;
        let mut extension_count: ExtensionMap = HashMap::new();

        for (lang_id, config) in LANGUAGE_REGISTRY.iter() {
            for ext in config.extensions {
                extension_count.entry(ext).or_default().push(lang_id);
            }
        }

        // These extensions might legitimately map to multiple languages
        let allowed_duplicates = ["h", "hpp"]; // C and C++ headers

        for (ext, langs) in extension_count.iter() {
            if langs.len() > 1 && !allowed_duplicates.contains(ext) {
                panic!("Extension '{ext}' maps to multiple languages: {langs:?}");
            }
        }
    }

    #[test]
    fn test_block_style_consistency() {
        // Test that block styles make sense
        let python_config = get_language_config("python").unwrap();
        assert!(!python_config.uses_braces);
        assert!(python_config.uses_indentation);

        let rust_config = get_language_config("rust").unwrap();
        assert!(rust_config.uses_braces);
        assert!(!rust_config.uses_indentation);

        // let yaml_config = get_language_config("yaml").unwrap();
        // assert!(!yaml_config.uses_braces);
        // assert!(yaml_config.uses_indentation);

        // Languages that use neither (markup languages)
        let html_config = get_language_config("html").unwrap();
        assert!(!html_config.uses_braces);
        assert!(!html_config.uses_indentation);

        let xml_config = get_language_config("xml").unwrap();
        assert!(!xml_config.uses_braces);
        assert!(!xml_config.uses_indentation);
    }

    #[test]
    fn test_query_completeness() {
        // Test that queries contain expected patterns for each language
        let rust_config = get_language_config("rust").unwrap();
        let rust_query = rust_config.tree_sitter_query.unwrap();
        assert!(rust_query.contains("function_item"));
        assert!(rust_query.contains("impl_item"));
        assert!(rust_query.contains("struct_item"));
        assert!(rust_query.contains("trait_item"));
        assert!(rust_query.contains("mod_item"));

        let python_config = get_language_config("python").unwrap();
        let python_query = python_config.tree_sitter_query.unwrap();
        assert!(python_query.contains("function_definition"));
        assert!(python_query.contains("class_definition"));

        let js_config = get_language_config("javascript").unwrap();
        let js_query = js_config.tree_sitter_query.unwrap();
        assert!(js_query.contains("function_declaration"));
        assert!(js_query.contains("arrow_function"));
        assert!(js_query.contains("class_declaration"));
        assert!(js_query.contains("method_definition"));
    }

    #[test]
    fn test_extension_coverage() {
        // Test that common extensions are covered
        let common_extensions = vec![
            ("rs", "rust"),
            ("py", "python"),
            ("js", "javascript"),
            ("ts", "typescript"),
            ("tsx", "tsx"),
            ("go", "go"),
            ("java", "java"),
            ("c", "c"),
            ("cpp", "cpp"),
            ("cs", "csharp"),
            ("sh", "bash"),
            ("html", "html"),
            ("ps1", "powershell"),
            ("sql", "sql"),
            ("json", "json"),
            ("xml", "xml"),
            ("yaml", "yaml"),
            ("yml", "yaml"),
        ];

        for (ext, expected_lang) in common_extensions {
            let lang = get_language_from_extension(ext);
            assert_eq!(
                lang,
                Some(expected_lang),
                "Extension '{ext}' should map to '{expected_lang}'"
            );
        }
    }
}
