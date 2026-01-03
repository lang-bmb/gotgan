//! BMBX Bundle Format - AI-Native Package Bundle
//!
//! Unified bundle containing:
//! - contracts.json: All contracts in JSON representation
//! - symbols.json: AI-discoverable symbol index
//! - types.json: Type signatures for semantic analysis
//! - src/: Source code
//! - bin/: Compiled binaries (multi-target)

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::GotganError;

// ============================================
// Contract JSON Schema (contracts.json)
// ============================================

/// Complete contract information for a package
#[derive(Debug, Serialize, Deserialize)]
pub struct ContractsJson {
    pub package: String,
    pub version: String,
    pub functions: Vec<FunctionContract>,
    pub types: Vec<TypeContract>,
    pub generated_at: String,
}

/// Contract information for a single function
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionContract {
    pub name: String,
    pub signature: String,
    pub visibility: String,
    #[serde(default)]
    pub preconditions: Vec<ContractClause>,
    #[serde(default)]
    pub postconditions: Vec<ContractClause>,
    #[serde(default)]
    pub named_contracts: Vec<NamedContractClause>,
    #[serde(default)]
    pub verified: Option<bool>,
    #[serde(default)]
    pub verification_time_ms: Option<u64>,
    #[serde(default)]
    pub attributes: Vec<String>,
}

/// A single contract clause (pre/post condition)
#[derive(Debug, Serialize, Deserialize)]
pub struct ContractClause {
    pub expr: String,
    #[serde(default)]
    pub message: Option<String>,
}

/// Named contract from where {} block
#[derive(Debug, Serialize, Deserialize)]
pub struct NamedContractClause {
    pub name: String,
    pub expr: String,
    pub kind: String, // "pre" | "post" | "invariant"
}

/// Refinement type contract
#[derive(Debug, Serialize, Deserialize)]
pub struct TypeContract {
    pub name: String,
    pub base: String,
    pub refinement: String,
    #[serde(default)]
    pub verified: Option<bool>,
}

// ============================================
// Symbol Index Schema (symbols.json)
// ============================================

/// AI-discoverable symbol index
#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolsJson {
    pub package: String,
    pub version: String,
    pub exports: Vec<SymbolExport>,
    #[serde(default)]
    pub semantic_tags: Vec<String>,
    #[serde(default)]
    pub ai_hints: AiHints,
}

/// Exported symbol information
#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolExport {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    #[serde(default)]
    pub description_inferred: Option<String>,
    #[serde(default)]
    pub complexity: Option<String>,
    #[serde(default)]
    pub pure: Option<bool>,
    #[serde(default)]
    pub contracts: Vec<String>,
    #[serde(default)]
    pub source_file: Option<String>,
    #[serde(default)]
    pub line_number: Option<u32>,
}

/// Symbol kind enumeration
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Type,
    Const,
}

/// AI hints for package discovery
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AiHints {
    #[serde(default)]
    pub use_when: Vec<String>,
    #[serde(default)]
    pub alternatives: Vec<String>,
    #[serde(default)]
    pub common_patterns: Vec<String>,
}

// ============================================
// Type Signatures Schema (types.json)
// ============================================

/// Type signature information for semantic analysis
#[derive(Debug, Serialize, Deserialize)]
pub struct TypesJson {
    pub package: String,
    pub version: String,
    pub functions: Vec<FunctionType>,
    pub types: Vec<TypeDefinition>,
    pub enums: Vec<EnumDefinition>,
    pub structs: Vec<StructDefinition>,
}

/// Function type signature
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionType {
    pub name: String,
    pub params: Vec<ParamType>,
    pub return_type: String,
    #[serde(default)]
    pub generic_params: Vec<String>,
}

/// Parameter type information
#[derive(Debug, Serialize, Deserialize)]
pub struct ParamType {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub refinement: Option<String>,
}

/// Type alias definition
#[derive(Debug, Serialize, Deserialize)]
pub struct TypeDefinition {
    pub name: String,
    pub definition: String,
    #[serde(default)]
    pub refinement: Option<String>,
}

/// Enum definition
#[derive(Debug, Serialize, Deserialize)]
pub struct EnumDefinition {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

/// Enum variant
#[derive(Debug, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<String>,
}

/// Struct definition
#[derive(Debug, Serialize, Deserialize)]
pub struct StructDefinition {
    pub name: String,
    pub fields: Vec<StructField>,
}

/// Struct field
#[derive(Debug, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

// ============================================
// BMBX Bundle Operations
// ============================================

/// BMBX bundle structure
#[derive(Debug)]
pub struct BmbxBundle {
    pub manifest_path: PathBuf,
    pub package_name: String,
    pub version: String,
    pub contracts: ContractsJson,
    pub symbols: SymbolsJson,
    pub types: TypesJson,
}

impl BmbxBundle {
    /// Create a new BMBX bundle from a project directory
    pub fn from_project(project_path: &Path) -> Result<Self, GotganError> {
        let manifest_path = project_path.join("gotgan.toml");
        if !manifest_path.exists() {
            return Err(GotganError::Bundle(format!(
                "No gotgan.toml found at {:?}",
                project_path
            )));
        }

        // Parse gotgan.toml
        let manifest_content = fs::read_to_string(&manifest_path)?;
        let manifest: toml::Value = toml::from_str(&manifest_content)?;

        let package_name = manifest
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown")
            .to_string();

        let version = manifest
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .unwrap_or("0.1.0")
            .to_string();

        // Extract contracts, symbols, and types from source files
        let contracts = Self::extract_contracts(project_path, &package_name, &version)?;
        let symbols = Self::extract_symbols(project_path, &package_name, &version, &contracts)?;
        let types = Self::extract_types(project_path, &package_name, &version)?;

        Ok(Self {
            manifest_path,
            package_name,
            version,
            contracts,
            symbols,
            types,
        })
    }

    /// Extract contracts from BMB source files
    fn extract_contracts(
        project_path: &Path,
        package_name: &str,
        version: &str,
    ) -> Result<ContractsJson, GotganError> {
        let src_dir = project_path.join("src");
        let mut functions = Vec::new();
        let mut types = Vec::new();

        if src_dir.exists() {
            Self::extract_contracts_recursive(&src_dir, &mut functions, &mut types)?;
        }

        // Also check for lib.bmb or main.bmb directly
        for entry_file in &["lib.bmb", "main.bmb", "src/lib.bmb", "src/main.bmb"] {
            let path = project_path.join(entry_file);
            if path.exists() {
                Self::extract_from_file(&path, &mut functions, &mut types)?;
            }
        }

        Ok(ContractsJson {
            package: package_name.to_string(),
            version: version.to_string(),
            functions,
            types,
            generated_at: chrono_lite_now(),
        })
    }

    /// Recursively extract contracts from a directory
    fn extract_contracts_recursive(
        dir: &Path,
        functions: &mut Vec<FunctionContract>,
        types: &mut Vec<TypeContract>,
    ) -> Result<(), GotganError> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                Self::extract_contracts_recursive(&path, functions, types)?;
            } else if path.extension().map_or(false, |e| e == "bmb") {
                Self::extract_from_file(&path, functions, types)?;
            }
        }

        Ok(())
    }

    /// Extract contracts from a single BMB file using simple parsing
    fn extract_from_file(
        path: &Path,
        functions: &mut Vec<FunctionContract>,
        types: &mut Vec<TypeContract>,
    ) -> Result<(), GotganError> {
        let content = fs::read_to_string(path)?;
        let source_file = path.file_name()
            .map(|s| s.to_string_lossy().to_string());

        // Simple line-by-line parsing for function definitions with contracts
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Parse function definitions
            if line.starts_with("pub fn ") || line.starts_with("fn ") {
                let visibility = if line.starts_with("pub ") { "pub" } else { "private" };

                // Extract function info
                if let Some(func) = Self::parse_function_with_contracts(&lines, i, visibility, source_file.as_deref()) {
                    functions.push(func);
                }
            }

            // Parse type definitions with refinements
            if line.starts_with("type ") && line.contains("where") {
                if let Some(type_contract) = Self::parse_type_refinement(line) {
                    types.push(type_contract);
                }
            }

            i += 1;
        }

        Ok(())
    }

    /// Parse a function definition with its contracts
    fn parse_function_with_contracts(
        lines: &[&str],
        start_idx: usize,
        visibility: &str,
        source_file: Option<&str>,
    ) -> Option<FunctionContract> {
        let first_line = lines[start_idx].trim();

        // Extract function name
        let fn_start = first_line.find("fn ")? + 3;
        let fn_name_end = first_line[fn_start..].find('(')?;
        let name = first_line[fn_start..fn_start + fn_name_end].trim().to_string();

        // Extract signature (from fn to return type)
        let mut signature = String::new();
        let mut current_line = first_line.to_string();

        // Find the complete signature including return type
        if let Some(arrow_pos) = current_line.find("->") {
            // Extract params and return type
            if let Some(paren_start) = current_line.find('(') {
                signature = current_line[paren_start..].to_string();
                // Clean up - find end of return type
                if let Some(ret_end) = signature.find(|c| c == '\n' || c == '=') {
                    signature.truncate(ret_end);
                }
            }
        } else {
            // Multi-line signature, look for ->
            let mut sig_parts = vec![current_line.clone()];
            let mut j = start_idx + 1;
            while j < lines.len() && !lines[j].contains("->") && !lines[j].contains("=") {
                sig_parts.push(lines[j].trim().to_string());
                j += 1;
            }
            if j < lines.len() && lines[j].contains("->") {
                sig_parts.push(lines[j].trim().to_string());
            }
            signature = sig_parts.join(" ");
            if let Some(paren_start) = signature.find('(') {
                signature = signature[paren_start..].to_string();
            }
        }

        // Collect preconditions and postconditions
        let mut preconditions = Vec::new();
        let mut postconditions = Vec::new();
        let mut attributes = Vec::new();

        // Look for pre/post conditions in subsequent lines
        let mut j = start_idx + 1;
        while j < lines.len() {
            let line = lines[j].trim();

            if line.starts_with("pre ") {
                let expr = line[4..].trim().to_string();
                preconditions.push(ContractClause {
                    expr,
                    message: None,
                });
            } else if line.starts_with("post ") {
                let expr = line[5..].trim().to_string();
                postconditions.push(ContractClause {
                    expr,
                    message: None,
                });
            } else if line.starts_with('@') {
                // Extract attribute
                let attr = line[1..].split_whitespace().next().unwrap_or("");
                attributes.push(format!("@{}", attr));
            } else if line.starts_with('=') || line.starts_with('{') || line.is_empty() {
                break;
            } else if line.starts_with("fn ") || line.starts_with("pub fn ") {
                break;
            }

            j += 1;
        }

        // Clean signature
        signature = signature.trim().trim_end_matches(';').trim().to_string();

        Some(FunctionContract {
            name: name.clone(),
            signature: format!("fn {}{}", name, signature),
            visibility: visibility.to_string(),
            preconditions,
            postconditions,
            named_contracts: Vec::new(),
            verified: None,
            verification_time_ms: None,
            attributes,
        })
    }

    /// Parse a type refinement
    fn parse_type_refinement(line: &str) -> Option<TypeContract> {
        // type NonZero = i64 where self != 0;
        let type_start = line.find("type ")? + 5;
        let eq_pos = line.find('=')?;
        let name = line[type_start..eq_pos].trim().to_string();

        let where_pos = line.find("where")?;
        let base = line[eq_pos + 1..where_pos].trim().to_string();

        let refinement = line[where_pos + 5..].trim().trim_end_matches(';').trim().to_string();

        Some(TypeContract {
            name,
            base,
            refinement,
            verified: None,
        })
    }

    /// Extract symbols for AI discovery
    fn extract_symbols(
        project_path: &Path,
        package_name: &str,
        version: &str,
        contracts: &ContractsJson,
    ) -> Result<SymbolsJson, GotganError> {
        let mut exports = Vec::new();

        // Convert function contracts to symbol exports
        for func in &contracts.functions {
            if func.visibility == "pub" {
                let mut contract_strs = Vec::new();
                for pre in &func.preconditions {
                    contract_strs.push(format!("pre: {}", pre.expr));
                }
                for post in &func.postconditions {
                    contract_strs.push(format!("post: {}", post.expr));
                }

                // Infer description from function name
                let description = infer_description(&func.name);

                // Check if function is pure (no side effects)
                let is_pure = !func.signature.contains("&mut")
                    && func.attributes.iter().any(|a| a.contains("pure"))
                    || func.preconditions.iter().all(|_| true); // Heuristic

                exports.push(SymbolExport {
                    name: func.name.clone(),
                    kind: SymbolKind::Function,
                    signature: func.signature.clone(),
                    description_inferred: Some(description),
                    complexity: None,
                    pure: Some(is_pure),
                    contracts: contract_strs,
                    source_file: None,
                    line_number: None,
                });
            }
        }

        // Infer semantic tags from function names
        let semantic_tags = infer_semantic_tags(&exports);

        Ok(SymbolsJson {
            package: package_name.to_string(),
            version: version.to_string(),
            exports,
            semantic_tags,
            ai_hints: AiHints::default(),
        })
    }

    /// Extract type information
    fn extract_types(
        project_path: &Path,
        package_name: &str,
        version: &str,
    ) -> Result<TypesJson, GotganError> {
        // Basic implementation - would be enhanced with full AST parsing
        Ok(TypesJson {
            package: package_name.to_string(),
            version: version.to_string(),
            functions: Vec::new(),
            types: Vec::new(),
            enums: Vec::new(),
            structs: Vec::new(),
        })
    }

    /// Write bundle to output directory
    pub fn write_to(&self, output_dir: &Path) -> Result<(), GotganError> {
        fs::create_dir_all(output_dir)?;

        // Write contracts.json
        let contracts_path = output_dir.join("contracts.json");
        let contracts_json = serde_json::to_string_pretty(&self.contracts)?;
        fs::write(&contracts_path, contracts_json)?;

        // Write symbols.json
        let symbols_path = output_dir.join("symbols.json");
        let symbols_json = serde_json::to_string_pretty(&self.symbols)?;
        fs::write(&symbols_path, symbols_json)?;

        // Write types.json
        let types_path = output_dir.join("types.json");
        let types_json = serde_json::to_string_pretty(&self.types)?;
        fs::write(&types_path, types_json)?;

        println!("✓ Generated contracts.json ({} functions, {} types)",
            self.contracts.functions.len(),
            self.contracts.types.len()
        );
        println!("✓ Generated symbols.json ({} exports)",
            self.symbols.exports.len()
        );
        println!("✓ Generated types.json");

        Ok(())
    }
}

// ============================================
// Helper Functions
// ============================================

/// Simple timestamp generation without chrono dependency
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

/// Infer description from function name using common patterns
fn infer_description(name: &str) -> String {
    let words: Vec<&str> = name.split('_').collect();

    // Common verb patterns
    let description = match words.first() {
        Some(&"is") | Some(&"has") | Some(&"can") => {
            format!("Check if {}", words[1..].join(" "))
        }
        Some(&"get") => {
            format!("Get {}", words[1..].join(" "))
        }
        Some(&"set") => {
            format!("Set {}", words[1..].join(" "))
        }
        Some(&"to") => {
            format!("Convert to {}", words[1..].join(" "))
        }
        Some(&"from") => {
            format!("Create from {}", words[1..].join(" "))
        }
        Some(&"parse") => {
            format!("Parse {}", words[1..].join(" "))
        }
        Some(&"find") => {
            format!("Find {}", words[1..].join(" "))
        }
        Some(&"count") => {
            format!("Count {}", words[1..].join(" "))
        }
        Some(&"read") => {
            format!("Read {}", words[1..].join(" "))
        }
        Some(&"write") => {
            format!("Write {}", words[1..].join(" "))
        }
        Some(&"skip") => {
            format!("Skip {}", words[1..].join(" "))
        }
        Some(&"extract") => {
            format!("Extract {}", words[1..].join(" "))
        }
        Some(&"strip") => {
            format!("Strip {}", words[1..].join(" "))
        }
        _ => words.join(" "),
    };

    description
}

/// Infer semantic tags from symbol names
fn infer_semantic_tags(exports: &[SymbolExport]) -> Vec<String> {
    let mut tags = std::collections::HashSet::new();

    for export in exports {
        let name = &export.name.to_lowercase();

        // Category-based tags
        if name.contains("string") || name.contains("str") || name.contains("char") {
            tags.insert("string".to_string());
        }
        if name.contains("int") || name.contains("num") || name.contains("digit") {
            tags.insert("numeric".to_string());
        }
        if name.contains("parse") {
            tags.insert("parsing".to_string());
        }
        if name.contains("find") || name.contains("search") || name.contains("index") {
            tags.insert("search".to_string());
        }
        if name.contains("convert") || name.contains("to_") || name.contains("from_") {
            tags.insert("conversion".to_string());
        }
        if name.contains("is_") || name.contains("has_") {
            tags.insert("predicate".to_string());
        }
        if name.contains("ws") || name.contains("whitespace") || name.contains("trim") {
            tags.insert("whitespace".to_string());
        }
        if name.contains("field") || name.contains("extract") {
            tags.insert("extraction".to_string());
        }
    }

    tags.into_iter().collect()
}

// ============================================
// Bundle Command Entry Point
// ============================================

/// Run the bundle command
pub fn run_bundle(
    output: Option<PathBuf>,
    single_file: bool,
) -> Result<(), GotganError> {
    let current_dir = std::env::current_dir()?;

    println!("Generating BMBX bundle...");

    // Create bundle from current project
    let bundle = BmbxBundle::from_project(&current_dir)?;

    // Determine output directory
    let output_dir = output.unwrap_or_else(|| current_dir.join("target").join("bmbx"));

    // Write bundle files
    bundle.write_to(&output_dir)?;

    println!("\n✓ BMBX bundle generated at {:?}", output_dir);
    println!("  Package: {} v{}", bundle.package_name, bundle.version);

    if single_file {
        println!("\nNote: --single-file bundling not yet implemented");
    }

    Ok(())
}

/// Explore package symbols
pub fn run_explore(
    package_path: Option<PathBuf>,
    show_symbols: bool,
    show_contracts: bool,
    show_types: bool,
    json_output: bool,
    filter: Option<String>,
) -> Result<(), GotganError> {
    let project_path = package_path.unwrap_or_else(|| {
        std::env::current_dir().expect("Failed to get current directory")
    });

    // Create bundle to get extracted info
    let bundle = BmbxBundle::from_project(&project_path)?;

    // JSON output mode
    if json_output {
        if show_contracts {
            println!("{}", serde_json::to_string_pretty(&bundle.contracts)?);
        } else if show_types {
            println!("{}", serde_json::to_string_pretty(&bundle.types)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&bundle.symbols)?);
        }
        return Ok(());
    }

    println!("Package: {} v{}", bundle.package_name, bundle.version);
    println!();

    // Default to showing symbols if no specific flag is set
    let show_any = show_symbols || show_contracts || show_types;
    let should_show_symbols = show_symbols || !show_any;

    if should_show_symbols {
        println!("Exports ({} symbols):", bundle.symbols.exports.len());
        println!("{}", "-".repeat(60));

        for export in &bundle.symbols.exports {
            // Apply filter if provided
            if let Some(ref pattern) = filter {
                if !export.name.contains(pattern) {
                    continue;
                }
            }

            let kind_str = match export.kind {
                SymbolKind::Function => "fn",
                SymbolKind::Struct => "struct",
                SymbolKind::Enum => "enum",
                SymbolKind::Type => "type",
                SymbolKind::Const => "const",
            };

            println!("  {} {} : {}", kind_str, export.name, export.signature);

            if let Some(desc) = &export.description_inferred {
                println!("      → {}", desc);
            }

            if !export.contracts.is_empty() {
                for contract in &export.contracts {
                    println!("      [{}]", contract);
                }
            }
            println!();
        }
    }

    if show_contracts {
        println!("\nContracts ({} functions, {} types):",
            bundle.contracts.functions.len(),
            bundle.contracts.types.len()
        );
        println!("{}", "-".repeat(60));

        for func in &bundle.contracts.functions {
            // Apply filter if provided
            if let Some(ref pattern) = filter {
                if !func.name.contains(pattern) {
                    continue;
                }
            }

            println!("  {}", func.name);
            for pre in &func.preconditions {
                println!("    pre: {}", pre.expr);
            }
            for post in &func.postconditions {
                println!("    post: {}", post.expr);
            }
            println!();
        }

        for ty in &bundle.contracts.types {
            // Apply filter if provided
            if let Some(ref pattern) = filter {
                if !ty.name.contains(pattern) {
                    continue;
                }
            }
            println!("  type {} = {} where {}", ty.name, ty.base, ty.refinement);
        }
    }

    if show_types {
        println!("\nTypes ({} functions, {} types, {} enums, {} structs):",
            bundle.types.functions.len(),
            bundle.types.types.len(),
            bundle.types.enums.len(),
            bundle.types.structs.len()
        );
        println!("{}", "-".repeat(60));

        for func in &bundle.types.functions {
            if let Some(ref pattern) = filter {
                if !func.name.contains(pattern) {
                    continue;
                }
            }
            println!("  fn {}", func.name);
        }
    }

    if !bundle.symbols.semantic_tags.is_empty() {
        println!("\nSemantic Tags: {}", bundle.symbols.semantic_tags.join(", "));
    }

    Ok(())
}

// ============================================
// Contract Compatibility Checker (v0.11.5)
// ============================================

/// Contract compatibility result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractChange {
    /// No change in contracts
    Compatible,
    /// Weaker precondition (acceptable - more permissive)
    WeakenedPre { old: String, new: String },
    /// Stronger postcondition (acceptable - more guarantees)
    StrengthenedPost { old: String, new: String },
    /// Stronger precondition (BREAKING - more restrictive)
    StrengthenedPre { old: String, new: String },
    /// Weaker postcondition (BREAKING - less guarantees)
    WeakenedPost { old: String, new: String },
    /// Added precondition (potentially breaking)
    AddedPre { expr: String },
    /// Removed postcondition (potentially breaking)
    RemovedPost { expr: String },
    /// Added postcondition (acceptable - more guarantees)
    AddedPost { expr: String },
    /// Removed precondition (acceptable - more permissive)
    RemovedPre { expr: String },
}

impl ContractChange {
    pub fn is_breaking(&self) -> bool {
        matches!(
            self,
            ContractChange::StrengthenedPre { .. }
                | ContractChange::WeakenedPost { .. }
                | ContractChange::AddedPre { .. }
                | ContractChange::RemovedPost { .. }
        )
    }

    pub fn description(&self) -> String {
        match self {
            ContractChange::Compatible => "No change".to_string(),
            ContractChange::WeakenedPre { old, new } => {
                format!("Weakened pre: {} → {}", old, new)
            }
            ContractChange::StrengthenedPost { old, new } => {
                format!("Strengthened post: {} → {}", old, new)
            }
            ContractChange::StrengthenedPre { old, new } => {
                format!("⚠️ BREAKING: Strengthened pre: {} → {}", old, new)
            }
            ContractChange::WeakenedPost { old, new } => {
                format!("⚠️ BREAKING: Weakened post: {} → {}", old, new)
            }
            ContractChange::AddedPre { expr } => {
                format!("⚠️ BREAKING: Added precondition: {}", expr)
            }
            ContractChange::RemovedPost { expr } => {
                format!("⚠️ BREAKING: Removed postcondition: {}", expr)
            }
            ContractChange::AddedPost { expr } => {
                format!("Added postcondition: {}", expr)
            }
            ContractChange::RemovedPre { expr } => {
                format!("Removed precondition: {}", expr)
            }
        }
    }
}

/// Function compatibility analysis result
#[derive(Debug)]
pub struct FunctionCompatibility {
    pub name: String,
    pub changes: Vec<ContractChange>,
    pub is_breaking: bool,
}

/// Package compatibility analysis result
#[derive(Debug)]
pub struct CompatibilityReport {
    pub old_version: String,
    pub new_version: String,
    pub functions: Vec<FunctionCompatibility>,
    pub removed_functions: Vec<String>,
    pub added_functions: Vec<String>,
    pub breaking_changes_count: usize,
}

impl CompatibilityReport {
    pub fn has_breaking_changes(&self) -> bool {
        self.breaking_changes_count > 0 || !self.removed_functions.is_empty()
    }

    pub fn print_summary(&self) {
        println!("Contract Compatibility Report");
        println!("============================");
        println!("Version: {} → {}", self.old_version, self.new_version);
        println!();

        if self.has_breaking_changes() {
            println!("⚠️  BREAKING CHANGES DETECTED");
            println!();
        }

        if !self.removed_functions.is_empty() {
            println!("Removed Functions ({}):", self.removed_functions.len());
            for name in &self.removed_functions {
                println!("  ❌ {}", name);
            }
            println!();
        }

        if !self.added_functions.is_empty() {
            println!("Added Functions ({}):", self.added_functions.len());
            for name in &self.added_functions {
                println!("  ✓ {}", name);
            }
            println!();
        }

        let breaking_funcs: Vec<_> = self.functions.iter()
            .filter(|f| f.is_breaking)
            .collect();

        if !breaking_funcs.is_empty() {
            println!("Functions with Breaking Changes ({}):", breaking_funcs.len());
            for func in breaking_funcs {
                println!("  {}", func.name);
                for change in &func.changes {
                    if change.is_breaking() {
                        println!("    {}", change.description());
                    }
                }
            }
            println!();
        }

        let compatible_changes: Vec<_> = self.functions.iter()
            .filter(|f| !f.is_breaking && !f.changes.is_empty() && f.changes.iter().any(|c| *c != ContractChange::Compatible))
            .collect();

        if !compatible_changes.is_empty() {
            println!("Compatible Changes ({}):", compatible_changes.len());
            for func in compatible_changes {
                println!("  {}", func.name);
                for change in &func.changes {
                    if !change.is_breaking() && *change != ContractChange::Compatible {
                        println!("    {}", change.description());
                    }
                }
            }
        }

        println!();
        if self.has_breaking_changes() {
            println!("Summary: {} breaking change(s) detected", self.breaking_changes_count + self.removed_functions.len());
        } else {
            println!("Summary: All changes are backward compatible ✓");
        }
    }
}

/// Compare two contract JSONs and produce a compatibility report
pub fn compare_contracts(old: &ContractsJson, new: &ContractsJson) -> CompatibilityReport {
    let mut functions = Vec::new();
    let mut removed_functions = Vec::new();
    let mut added_functions = Vec::new();
    let mut breaking_count = 0;

    // Build lookup map for old functions
    let old_funcs: std::collections::HashMap<_, _> = old.functions.iter()
        .map(|f| (f.name.clone(), f))
        .collect();

    // Build lookup map for new functions
    let new_funcs: std::collections::HashMap<_, _> = new.functions.iter()
        .map(|f| (f.name.clone(), f))
        .collect();

    // Check for removed functions (breaking)
    for name in old_funcs.keys() {
        if !new_funcs.contains_key(name) {
            removed_functions.push(name.clone());
        }
    }

    // Check for added functions (not breaking)
    for name in new_funcs.keys() {
        if !old_funcs.contains_key(name) {
            added_functions.push(name.clone());
        }
    }

    // Compare contracts for functions that exist in both versions
    for (name, old_func) in &old_funcs {
        if let Some(new_func) = new_funcs.get(name) {
            let changes = compare_function_contracts(old_func, new_func);
            let is_breaking = changes.iter().any(|c| c.is_breaking());

            if is_breaking {
                breaking_count += 1;
            }

            functions.push(FunctionCompatibility {
                name: name.clone(),
                changes,
                is_breaking,
            });
        }
    }

    CompatibilityReport {
        old_version: old.version.clone(),
        new_version: new.version.clone(),
        functions,
        removed_functions,
        added_functions,
        breaking_changes_count: breaking_count,
    }
}

/// Compare contracts of two function versions
fn compare_function_contracts(old: &FunctionContract, new: &FunctionContract) -> Vec<ContractChange> {
    let mut changes = Vec::new();

    // Compare preconditions
    let old_pres: std::collections::HashSet<_> = old.preconditions.iter()
        .map(|c| c.expr.clone())
        .collect();
    let new_pres: std::collections::HashSet<_> = new.preconditions.iter()
        .map(|c| c.expr.clone())
        .collect();

    // Removed preconditions (acceptable - more permissive)
    for expr in old_pres.difference(&new_pres) {
        changes.push(ContractChange::RemovedPre { expr: expr.clone() });
    }

    // Added preconditions (breaking - more restrictive)
    for expr in new_pres.difference(&old_pres) {
        changes.push(ContractChange::AddedPre { expr: expr.clone() });
    }

    // Compare postconditions
    let old_posts: std::collections::HashSet<_> = old.postconditions.iter()
        .map(|c| c.expr.clone())
        .collect();
    let new_posts: std::collections::HashSet<_> = new.postconditions.iter()
        .map(|c| c.expr.clone())
        .collect();

    // Removed postconditions (breaking - less guarantees)
    for expr in old_posts.difference(&new_posts) {
        changes.push(ContractChange::RemovedPost { expr: expr.clone() });
    }

    // Added postconditions (acceptable - more guarantees)
    for expr in new_posts.difference(&old_posts) {
        changes.push(ContractChange::AddedPost { expr: expr.clone() });
    }

    if changes.is_empty() {
        changes.push(ContractChange::Compatible);
    }

    changes
}

/// Run contract compatibility check between two versions
pub fn run_compat_check(
    old_path: Option<PathBuf>,
    new_path: Option<PathBuf>,
) -> Result<(), GotganError> {
    let current_dir = std::env::current_dir()?;

    // Load old contracts
    let old_contracts = if let Some(path) = old_path {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        // Try to load from target/bmbx/contracts.json
        let default_path = current_dir.join("target").join("bmbx").join("contracts.json");
        if default_path.exists() {
            let content = fs::read_to_string(&default_path)?;
            serde_json::from_str(&content)?
        } else {
            return Err(GotganError::Bundle(
                "No old contracts.json specified and target/bmbx/contracts.json not found".to_string()
            ));
        }
    };

    // Load new contracts (from current project)
    let new_contracts = if let Some(path) = new_path {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        // Generate from current project
        let bundle = BmbxBundle::from_project(&current_dir)?;
        bundle.contracts
    };

    // Compare and generate report
    let report = compare_contracts(&old_contracts, &new_contracts);
    report.print_summary();

    if report.has_breaking_changes() {
        std::process::exit(1);
    }

    Ok(())
}
