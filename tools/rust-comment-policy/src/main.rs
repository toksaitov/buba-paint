use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use proc_macro2::LineColumn;
use rustc_lexer::{TokenKind, tokenize};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, File, ImplItemFn, ItemFn, ItemImpl, TraitItemFn, Type, TypePath};
use walkdir::WalkDir;

const DEFAULT_ROOTS: &[&str] = &[
    "bots/paint/src",
    "bots/paint/tests",
    "agent/src",
    "agent/tests",
    "dashboard/server/src",
    "dashboard/server/tests",
    "tools/rust-comment-policy/src",
];

/// Stores the violation totals for one `Rust` source file.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct FileCounts {
    missing_docs: usize,
    non_doc_comments: usize,
}

/// Stores the metadata needed to audit or document one function item.
#[derive(Debug, Clone)]
struct FunctionInfo {
    has_docs: bool,
    is_test: bool,
    start: LineColumn,
    name: String,
    owner: Option<String>,
}

/// Collects functions while tracking the current impl owner.
#[derive(Debug, Default)]
struct FunctionCollector {
    functions: Vec<FunctionInfo>,
    impl_stack: Vec<Option<String>>,
}

impl<'ast> Visit<'ast> for FunctionCollector {
    /// Visits a free function and stores its audit metadata.
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.push_function(&node.attrs, &node.sig.ident.to_string(), node.span().start(), None);
        syn::visit::visit_item_fn(self, node);
    }

    /// Visits an impl method and stores its audit metadata.
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let owner = self.impl_stack.last().cloned().flatten();
        self.push_function(
            &node.attrs,
            &node.sig.ident.to_string(),
            node.span().start(),
            owner,
        );
        syn::visit::visit_impl_item_fn(self, node);
    }

    /// Visits a trait method and stores its audit metadata.
    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        self.push_function(&node.attrs, &node.sig.ident.to_string(), node.span().start(), None);
        syn::visit::visit_trait_item_fn(self, node);
    }

    /// Tracks the current impl owner while visiting methods.
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.impl_stack.push(type_name(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        let _ = self.impl_stack.pop();
    }
}

impl FunctionCollector {
    /// Records one function with its rustdoc and test attributes.
    fn push_function(
        &mut self,
        attrs: &[Attribute],
        name: &str,
        start: LineColumn,
        owner: Option<String>,
    ) {
        self.functions.push(FunctionInfo {
            has_docs: has_rustdoc(attrs),
            is_test: has_test_attr(attrs),
            start,
            name: name.to_string(),
            owner,
        });
    }
}

/// Runs the selected command against the configured `Rust` roots.
fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = parse_command(args.first().map(String::as_str))?;
    let cwd = env::current_dir().context("reading current directory")?;
    let roots = parse_roots(&args);

    match command {
        Command::Check => check(&cwd, &roots),
        Command::Report => report(&cwd, &roots),
        Command::Fix => fix(&cwd, &roots),
    }
}

/// Describes the supported CLI commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Check,
    Report,
    Fix,
}

/// Parses the command name from CLI arguments.
fn parse_command(command: Option<&str>) -> Result<Command> {
    match command.unwrap_or("check") {
        "check" => Ok(Command::Check),
        "report" => Ok(Command::Report),
        "fix" => Ok(Command::Fix),
        other => bail!("unknown command: {other}"),
    }
}

/// Parses optional root arguments after the command name.
fn parse_roots(args: &[String]) -> Vec<PathBuf> {
    let start = usize::from(matches!(args.first().map(String::as_str), Some("check" | "report" | "fix")));
    let roots = &args[start..];
    if roots.is_empty() {
        DEFAULT_ROOTS.iter().map(PathBuf::from).collect()
    } else {
        roots.iter().map(PathBuf::from).collect()
    }
}

/// Enforces a zero-violation `Rust` comment policy.
fn check(cwd: &Path, roots: &[PathBuf]) -> Result<()> {
    let summary = analyze_workspace(cwd, roots)?;
    if summary.is_empty() {
        println!("rust comment policy summary: files_with_backlog=0, missing_docs=0, non_doc_comments=0");
        return Ok(());
    }

    print_summary("rust comment policy violations", &summary);
    bail!("rust comment policy check failed");
}

/// Prints the current violation report without failing.
fn report(cwd: &Path, roots: &[PathBuf]) -> Result<()> {
    let summary = analyze_workspace(cwd, roots)?;
    print_summary("current violations", &summary);
    Ok(())
}

/// Applies the automatic `Rust` cleanup to every scanned file.
fn fix(cwd: &Path, roots: &[PathBuf]) -> Result<()> {
    let files = collect_rust_files(cwd, roots)?;
    let mut updated = 0usize;

    for path in files {
        if fix_file(cwd, &path)? {
            updated += 1;
        }
    }

    println!("rust comment policy fix: updated_files={updated}");
    Ok(())
}

/// Walks the configured roots and returns every `Rust` source file.
fn collect_rust_files(cwd: &Path, roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in roots {
        let full_root = cwd.join(root);
        if !full_root.exists() {
            continue;
        }
        for entry in WalkDir::new(full_root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if entry.file_type().is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Analyzes every `Rust` file and returns the non-empty violation summary.
fn analyze_workspace(cwd: &Path, roots: &[PathBuf]) -> Result<BTreeMap<String, FileCounts>> {
    let mut summary = BTreeMap::new();
    for path in collect_rust_files(cwd, roots)? {
        let counts = analyze_file(cwd, &path)?;
        if counts != FileCounts::default() {
            let rel_path = relative_path(cwd, &path);
            summary.insert(rel_path, counts);
        }
    }
    Ok(summary)
}

/// Analyzes one `Rust` file for missing rustdoc and non-doc comments.
fn analyze_file(cwd: &Path, path: &Path) -> Result<FileCounts> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.strip_prefix(cwd).unwrap_or(path).display()))?;
    let parsed: File = syn::parse_file(&source)
        .with_context(|| format!("parsing {}", path.strip_prefix(cwd).unwrap_or(path).display()))?;
    let mut collector = FunctionCollector::default();
    collector.visit_file(&parsed);

    Ok(FileCounts {
        missing_docs: collector.functions.iter().filter(|function| !function.has_docs).count(),
        non_doc_comments: non_doc_comment_ranges(&source).len(),
    })
}

/// Applies rustdoc insertion and non-doc comment stripping to one file.
fn fix_file(cwd: &Path, path: &Path) -> Result<bool> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.strip_prefix(cwd).unwrap_or(path).display()))?;
    let parsed: File = syn::parse_file(&source)
        .with_context(|| format!("parsing {}", path.strip_prefix(cwd).unwrap_or(path).display()))?;
    let mut collector = FunctionCollector::default();
    collector.visit_file(&parsed);

    let line_starts = line_start_offsets(&source);
    let mut updated = source.clone();
    let mut insertions = collector
        .functions
        .iter()
        .filter(|function| !function.has_docs)
        .map(|function| -> Result<(usize, String)> {
            let offset = offset_for(&line_starts, function.start)
                .with_context(|| format!("locating function {}", function.name))?;
            let insertion_start = line_start(&updated, offset);
            Ok((
                insertion_start,
                build_doc_comment(&updated, offset, function),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    insertions.sort_by_key(|(offset, _)| *offset);

    for (offset, doc) in insertions.into_iter().rev() {
        updated.insert_str(offset, &doc);
    }

    updated = strip_non_doc_comments(&updated);
    updated = collapse_blank_lines(&updated);

    if updated == source {
        return Ok(false);
    }

    fs::write(path, updated)
        .with_context(|| format!("writing {}", path.strip_prefix(cwd).unwrap_or(path).display()))?;
    Ok(true)
}

/// Returns whether any attribute on the function is rustdoc.
fn has_rustdoc(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("doc"))
}

/// Returns whether any attribute marks the function as a test.
fn has_test_attr(attrs: &[Attribute]) -> bool {
    attrs.iter()
        .any(|attr| attr.path().segments.last().is_some_and(|segment| segment.ident == "test"))
}

/// Resolves the simple type name for an impl block owner.
fn type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(TypePath { path, .. }) => path.segments.last().map(|segment| segment.ident.to_string()),
        _ => None,
    }
}

/// Converts line and column information into a byte offset.
fn offset_for(line_starts: &[usize], line_column: LineColumn) -> Option<usize> {
    let line_index = line_column.line.checked_sub(1)?;
    line_starts
        .get(line_index)
        .map(|line_start| line_start + line_column.column)
}

/// Returns the starting byte offset of every source line.
fn line_start_offsets(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, ch) in source.char_indices() {
        if ch == '\n' {
            starts.push(idx + 1);
        }
    }
    starts
}

/// Builds the rustdoc block that should precede one function.
fn build_doc_comment(source: &str, offset: usize, function: &FunctionInfo) -> String {
    let indent = leading_indent(source, offset);
    format!("{indent}/// {}\n", doc_sentence(function))
}

/// Returns the indentation prefix for the line that contains the offset.
fn leading_indent(source: &str, offset: usize) -> String {
    let line_start = line_start(source, offset);
    source[line_start..offset]
        .chars()
        .take_while(|ch| ch.is_ascii_whitespace())
        .collect()
}

/// Returns the starting byte offset for the line that contains the offset.
fn line_start(source: &str, offset: usize) -> usize {
    source[..offset].rfind('\n').map_or(0, |index| index + 1)
}

/// Generates one concise rustdoc sentence for the function.
fn doc_sentence(function: &FunctionInfo) -> String {
    if function.is_test {
        return format!("Verifies that {}.", humanize_test_name(&function.name));
    }

    match function.name.as_str() {
        "new" => function
            .owner
            .as_deref()
            .map_or_else(|| "Creates a new instance.".to_string(), |owner| format!("Creates a new `{owner}`.")),
        "default" => function
            .owner
            .as_deref()
            .map_or_else(|| "Builds the default value.".to_string(), |owner| format!("Builds the default `{owner}`.")),
        _ => prefixed_sentence(&function.name),
    }
}

/// Generates a sentence for a non-test function based on its verb prefix.
fn prefixed_sentence(name: &str) -> String {
    const PREFIXES: &[(&str, &str)] = &[
        ("parse_", "Parses"),
        ("build_", "Builds"),
        ("run_", "Runs"),
        ("load_", "Loads"),
        ("save_", "Saves"),
        ("apply_", "Applies"),
        ("create_", "Creates"),
        ("make_", "Builds"),
        ("collect_", "Collects"),
        ("resolve_", "Resolves"),
        ("write_", "Writes"),
        ("read_", "Reads"),
        ("open_", "Opens"),
        ("close_", "Closes"),
        ("update_", "Updates"),
        ("record_", "Records"),
        ("note_", "Records"),
        ("set_", "Sets"),
        ("get_", "Returns"),
        ("find_", "Finds"),
        ("log_", "Logs"),
        ("send_", "Sends"),
        ("spawn_", "Spawns"),
        ("capture_", "Captures"),
        ("calculate_", "Calculates"),
        ("merge_", "Merges"),
        ("copy_", "Copies"),
        ("upgrade_", "Upgrades"),
        ("verify_", "Verifies"),
        ("should_", "Returns whether"),
        ("can_", "Returns whether"),
        ("is_", "Returns whether"),
        ("has_", "Returns whether"),
    ];

    for (prefix, verb) in PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix) {
            let phrase = humanize_words(rest);
            return format!("{verb} {phrase}.");
        }
    }

    sentence_case(&humanize_words(name))
}

/// Converts a test name into readable prose.
fn humanize_test_name(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("test_") {
        return humanize_words(rest);
    }
    humanize_words(name)
}

/// Converts a snake_case identifier into plain words.
fn humanize_words(name: &str) -> String {
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Uppercases the first character of the sentence and ensures punctuation.
fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return "Documents the function.".to_string();
    };
    let mut sentence = first.to_uppercase().collect::<String>();
    sentence.push_str(chars.as_str());
    if sentence.ends_with('.') {
        sentence
    } else {
        format!("{sentence}.")
    }
}

/// Returns the byte ranges for every non-doc comment in the source.
fn non_doc_comment_ranges(source: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0usize;

    for token in tokenize(source) {
        let end = offset + token.len;
        if matches!(token.kind, TokenKind::LineComment | TokenKind::BlockComment { .. }) {
            let slice = &source[offset..end];
            if !is_doc_comment(slice) {
                ranges.push((offset, end));
            }
        }
        offset = end;
    }

    ranges
}

/// Returns whether the comment token is a `Rust` doc comment.
fn is_doc_comment(comment: &str) -> bool {
    comment.starts_with("///")
        || comment.starts_with("//!")
        || comment.starts_with("/**")
        || comment.starts_with("/*!")
}

/// Removes every non-doc `Rust` comment from the source.
fn strip_non_doc_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut last = 0usize;

    for (start, end) in non_doc_comment_ranges(source) {
        output.push_str(&source[last..start]);
        last = end;
    }

    output.push_str(&source[last..]);
    output
}

/// Collapses repeated blank lines and trims trailing whitespace from lines.
fn collapse_blank_lines(source: &str) -> String {
    let had_trailing_newline = source.ends_with('\n');
    let mut result = String::new();
    let mut blank_run = 0usize;

    for line in source.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run == 1 {
                result.push('\n');
            }
            continue;
        }

        blank_run = 0;
        result.push_str(trimmed);
        result.push('\n');
    }

    if !had_trailing_newline && result.ends_with('\n') {
        let _ = result.pop();
    }

    result
}

/// Renders the summary in a stable, human-readable format.
fn print_summary(label: &str, summary: &BTreeMap<String, FileCounts>) {
    println!("{label}:");
    for (path, counts) in summary {
        println!(
            "  {path}: missing_docs={}, non_doc_comments={}",
            counts.missing_docs, counts.non_doc_comments
        );
    }
    let (file_count, missing_docs, non_doc_comments) = summary_totals(summary);
    println!(
        "rust comment policy summary: files_with_backlog={file_count}, missing_docs={missing_docs}, non_doc_comments={non_doc_comments}"
    );
}

/// Aggregates the workspace totals from the per-file summary.
fn summary_totals(summary: &BTreeMap<String, FileCounts>) -> (usize, usize, usize) {
    summary.values().fold((0, 0, 0), |(files, docs, comments), counts| {
        (
            files + 1,
            docs + counts.missing_docs,
            comments + counts.non_doc_comments,
        )
    })
}

/// Converts a path into a normalized workspace-relative string.
fn relative_path(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
