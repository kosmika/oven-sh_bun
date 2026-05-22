//! Types and crate aliases extracted from `bundle_v2` so `Chunk.rs` /
//! `LinkerContext.rs` / `ParseTask.rs` / `Graph.rs` can compile against real
//! surfaces.
//!
//! These are pure value types with no T6 deps. `bundle_v2.rs` re-exports the
//! whole set from here (its draft duplicates were collapsed in DEDUP D059);
//! nothing here owns behavior that belongs elsewhere.

#![warn(unused_must_use)]

use bun_core::strings;
// `Ref` is re-exported (pub use) below for `crate::Ref`; the local `use` here
// is intentionally folded into that to avoid duplicate-import errors.

use crate::{Index, IndexInt, options};

pub use bun_core as bun_str;
/// `bun_output` is a thin re-export crate over `bun_core` that isn't a
/// workspace member yet; alias `bun_core` (which exports `declare_scope!` /
/// `scoped_log!` at its root) so `bun_output::declare_scope!(…)` resolves.
pub use bun_core as bun_output;
pub use bun_resolver::fs as bun_fs;
pub use bun_resolver::node_fallbacks as bun_node_fallbacks;
pub mod perf {
    pub use bun_perf::{Ctx, PerfEvent};

    #[inline]
    pub(crate) fn trace(_name: &'static str) -> Ctx {
        bun_perf::trace(PerfEvent::_Stub)
    }
}

pub mod bun_css {
    // `bun_css` is an UNCONDITIONAL dep (`bun_js_parser` already pulls it in
    // for `BundledAst.css`'s field type). Glob-re-export always.
    pub use ::bun_css::css_modules::Config as CssModuleConfig;
    pub use ::bun_css::*;

    /// `LayerName` for `Chunk::Layers`. The real `bun_css::css_parser::LayerName`
    /// (its `'bump` lifetime is already laundered to `'static` in
    /// `rules/layer.rs`, so no thread needed here).
    pub use ::bun_css::css_parser::LayerName;
}

// ──────────────────────────────────────────────────────────────────────────
// Value types extracted from `bundle_v2.zig`.
// ──────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
pub struct PartRange {
    pub source_index: Index,
    pub part_index_begin: u32,
    pub part_index_end: u32,
}

/// `bundle_v2.zig:StableRef` — `packed struct(u96)`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct StableRef {
    pub stable_source_index: IndexInt,
    pub r#ref: Ref,
}

impl StableRef {
    pub fn is_less_than(_: (), a: StableRef, b: StableRef) -> bool {
        let (a_idx, b_idx) = (a.stable_source_index, b.stable_source_index);
        a_idx < b_idx || (a_idx == b_idx && { a.r#ref }.inner_index() < { b.r#ref }.inner_index())
    }
}

impl PartialEq for StableRef {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        let (a_idx, a_ref) = (self.stable_source_index, self.r#ref);
        let (b_idx, b_ref) = (other.stable_source_index, other.r#ref);
        a_idx == b_idx && a_ref == b_ref
    }
}
impl Eq for StableRef {}
impl Ord for StableRef {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let (a_idx, a_ref) = (self.stable_source_index, self.r#ref);
        let (b_idx, b_ref) = (other.stable_source_index, other.r#ref);
        (a_idx, a_ref.inner_index()).cmp(&(b_idx, b_ref.inner_index()))
    }
}
impl PartialOrd for StableRef {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// `bundle_v2.zig:ImportTracker`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportTracker {
    pub source_index: Index,
    pub name_loc: bun_ast::Loc,
    pub import_ref: Ref,
}

/// `bundle_v2.zig:CrossChunkImport.Item`.
#[derive(Default, Clone)]
pub struct CrossChunkImportItem {
    pub export_alias: Box<[u8]>,
    pub r#ref: Ref,
}
pub type CrossChunkImportItemList = Vec<CrossChunkImportItem>;
/// `bundle_v2.zig:CrossChunkImport`.
#[derive(Default)]
pub struct CrossChunkImport {
    pub chunk_index: IndexInt,
    /// Borrowed view into `ImportsFromOtherChunks` — Zig's `BabyList` has no
    /// destructor, so dropping `CrossChunkImport` must not free this buffer.
    pub sorted_import_items: core::mem::ManuallyDrop<CrossChunkImportItemList>,
}
/// `Chunk.zig:ImportsFromOtherChunks`.
pub mod cross_chunk_import {
    pub(crate) type ItemList = super::CrossChunkImportItemList;
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeclInfoKind {
    Declared,
    Lexical,
}
#[derive(Clone)]
pub struct DeclInfo {
    pub name: Box<[u8]>,
    pub kind: DeclInfoKind,
}

/// `bundle_v2.zig:CompileResult`.
pub enum CompileResult {
    Javascript {
        source_index: IndexInt,
        result: bun_js_printer::PrintResult,
        /// Top-level declarations collected from converted statements during
        /// parallel printing. Used by postProcessJSChunk to populate
        /// ModuleInfo without re-scanning the original (unconverted) AST.
        decls: Box<[DeclInfo]>,
    },
    Css {
        result: Result<Box<[u8]>, bun_core::Error>,
        source_index: IndexInt,
        source_map: Option<bun_sourcemap::Chunk>,
    },
    Html {
        source_index: IndexInt,
        code: Box<[u8]>,
        /// Offsets are used for DevServer to inject resources without re-bundling.
        script_injection_offset: u32,
    },
}

impl CompileResult {
    pub fn source_index(&self) -> IndexInt {
        match self {
            CompileResult::Javascript { source_index, .. }
            | CompileResult::Css { source_index, .. }
            | CompileResult::Html { source_index, .. } => *source_index,
        }
    }

    /// bundle_v2.zig:4215-4230.
    pub fn code(&self) -> &[u8] {
        match self {
            CompileResult::Javascript { result, .. } => match result {
                bun_js_printer::PrintResult::Result(r) => &r.code,
                bun_js_printer::PrintResult::Err(_) => b"",
            },
            CompileResult::Css { result, .. } => match result {
                Ok(v) => v,
                Err(_) => b"",
            },
            CompileResult::Html { code, .. } => code,
        }
    }

    /// Consume `self` and yield the owned code buffer. Used when the
    /// `StringJoiner` must outlive the `CompileResult` local that produced it
    /// (Zig `j.push(code, allocator)` ownership-transfer semantics).
    pub fn into_code(self) -> Box<[u8]> {
        match self {
            CompileResult::Javascript { result, .. } => match result {
                bun_js_printer::PrintResult::Result(r) => r.code,
                bun_js_printer::PrintResult::Err(_) => Box::default(),
            },
            CompileResult::Css { result, .. } => result.unwrap_or_default(),
            CompileResult::Html { code, .. } => code,
        }
    }

    /// bundle_v2.zig:4232-4241.
    pub fn source_map_chunk(&self) -> Option<&bun_sourcemap::Chunk> {
        match self {
            CompileResult::Javascript { result, .. } => match result {
                bun_js_printer::PrintResult::Result(r) => r.source_map.as_ref(),
                bun_js_printer::PrintResult::Err(_) => None,
            },
            CompileResult::Css { source_map, .. } => source_map.as_ref(),
            CompileResult::Html { .. } => None,
        }
    }
}

impl Clone for CompileResult {
    fn clone(&self) -> Self {
        match self {
            CompileResult::Javascript {
                source_index,
                result,
                decls,
            } => CompileResult::Javascript {
                source_index: *source_index,
                result: match result {
                    bun_js_printer::PrintResult::Result(r) => {
                        bun_js_printer::PrintResult::Result(bun_js_printer::PrintResultSuccess {
                            code: r.code.clone(),
                            source_map: r.source_map.clone(),
                        })
                    }
                    bun_js_printer::PrintResult::Err(e) => bun_js_printer::PrintResult::Err(*e),
                },
                decls: decls.clone(),
            },
            CompileResult::Css {
                result,
                source_index,
                source_map,
            } => CompileResult::Css {
                result: result.clone(),
                source_index: *source_index,
                source_map: source_map.clone(),
            },
            CompileResult::Html {
                source_index,
                code,
                script_injection_offset,
            } => CompileResult::Html {
                source_index: *source_index,
                code: code.clone(),
                script_injection_offset: *script_injection_offset,
            },
        }
    }
}

// PORT NOTE: `Default` so `CompileResult::Javascript { .., ..Default::default() }`
// FRU sites in `postProcessJSChunk.rs` compile. Returns the `Javascript`
// variant (the only one those FRU sites construct).
impl Default for CompileResult {
    fn default() -> Self {
        CompileResult::Javascript {
            source_index: 0,
            result: bun_js_printer::PrintResult::Result(bun_js_printer::PrintResultSuccess {
                code: Box::new([]),
                source_map: None,
            }),
            decls: Box::new([]),
        }
    }
}

pub fn generic_path_with_pretty_initialized(
    path: &bun_paths::fs::Path<'static>,
    target: options::Target,
    top_level_dir: &[u8],
    _bump: &bun_alloc::Arena,
) -> Result<bun_paths::fs::Path<'static>, bun_core::Error> {
    use bun_fs::PathResolverExt as _;
    use bun_io::Write as _;

    let mut buf = bun_paths::path_buffer_pool::get();

    let is_node = path.namespace == b"node";
    if is_node
        && (strings::has_prefix(path.text, bun_node_fallbacks::IMPORT_PATH)
            || !bun_paths::is_absolute(path.text))
    {
        return Ok(*path);
    }

    // "file" namespace should use the relative file path for its display name.
    // the "node" namespace is also put through this code path so that the
    // "node:" prefix is not emitted.
    if path.is_file() || is_node {
        let mut buf2 = bun_paths::path_buffer_pool::get();
        // TODO(port): in Zig buf2 aliases buf when target != ssr.
        let rel = bun_paths::resolve_path::relative_platform_buf::<
            bun_paths::resolve_path::platform::Loose,
            false,
        >(&mut **buf2, top_level_dir, path.text);
        // D090: `bun_paths::fs::Path<'static>` and `bun_fs::Path` are the same type;
        // covariance lets `path_clone` widen to `Path<'_>` for the temp `pretty`.
        let mut path_clone: bun_fs::Path<'_> = *path;
        // stack-allocated temporary is not leaked because dupeAlloc on the path will
        // move .pretty into the heap. that function also fixes some slash issues.
        if target == options::Target::BakeServerComponentsSsr {
            // the SSR graph needs different pretty names or else HMR mode will
            // confuse the two modules.
            let mut fbs = bun_io::FixedBufferStream::new_mut(&mut buf.0[..]);
            let _ = fbs.write_all(b"ssr:");
            let _ = fbs.write_all(rel);
            let written = fbs.pos;
            path_clone.pretty = &buf.0[..written];
        } else {
            path_clone.pretty = rel;
        }
        path_clone.dupe_alloc_fix_pretty()
    } else {
        // in non-file namespaces, standard filesystem rules do not apply.
        let mut path_clone: bun_fs::Path<'_> = *path;
        let mut fbs = bun_io::FixedBufferStream::new_mut(&mut buf.0[..]);
        // PORT NOTE: raw byte writes (not `write!` over `bstr::BStr`) — see
        // the `ssr:` branch above; namespace/text may carry non-UTF-8 bytes.
        if target == options::Target::BakeServerComponentsSsr {
            let _ = fbs.write_all(b"ssr:");
        }
        // make sure that a namespace including a colon wont collide with anything
        let _ = write_escaped_namespace(&mut fbs, path_clone.namespace);
        let _ = fbs.write_all(b":");
        let _ = fbs.write_all(path_clone.text);
        let written = fbs.pos;
        path_clone.pretty = &buf.0[..written];
        path_clone.dupe_alloc_fix_pretty()
    }
}

fn write_escaped_namespace<W: bun_io::Write + ?Sized>(w: &mut W, slice: &[u8]) -> bun_io::Result {
    let mut rest = slice;
    while let Some(i) = strings::index_of_char(rest, b':') {
        w.write_all(&rest[..i as usize])?;
        w.write_all(b"::")?;
        rest = &rest[i as usize + 1..];
    }
    w.write_all(rest)
}

/// `bundle_v2.zig:CompileResultForSourceMap`.

pub struct CompileResultForSourceMap {
    pub source_map_chunk: bun_sourcemap::Chunk,
    pub generated_offset: bun_sourcemap::LineColumnOffset,
    pub source_index: u32,
}

bun_collections::multi_array_columns! {
    pub trait CompileResultForSourceMapColumns for CompileResultForSourceMap {
        source_map_chunk: bun_sourcemap::Chunk,
        generated_offset: bun_sourcemap::LineColumnOffset,
        source_index: u32,
    }
}

/// `bundle_v2.zig:ContentHasher` — `std.hash.XxHash64` (seed 0). xxhash64
/// outperforms wyhash above ~1KB.
#[derive(Default)]
pub struct ContentHasher {
    pub hasher: bun_hash::XxHash64Streaming,
}
// `bun.Output.scoped(.ContentHasher, .hidden)` (bundle_v2.zig:4258). The static
// (value namespace) deliberately puns the struct name (type namespace) — brace
// structs only occupy the type namespace, so the two coexist.
bun_core::declare_scope!(ContentHasher, hidden);
impl ContentHasher {
    pub(crate) fn write(&mut self, bytes: &[u8]) {
        bun_core::scoped_log!(
            ContentHasher,
            "HASH_UPDATE {}:\n{}\n----------\n",
            bytes.len(),
            bstr::BStr::new(bytes)
        );
        self.hasher.update(&(bytes.len() as u64).to_ne_bytes());
        self.hasher.update(bytes);
    }
    pub(crate) fn run(bytes: &[u8]) -> u64 {
        let mut h = ContentHasher::default();
        h.write(bytes);
        h.digest()
    }
    /// `bundle_v2.zig:ContentHasher.writeInts` — `std.mem.sliceAsBytes(i)`.
    pub(crate) fn write_ints(&mut self, i: &[u32]) {
        bun_core::scoped_log!(ContentHasher, "HASH_UPDATE: {:?}\n", i);
        self.hasher.update(bytemuck::cast_slice::<u32, u8>(i));
    }
    pub(crate) fn digest(&self) -> u64 {
        self.hasher.digest()
    }
}

pub use bun_core::cheap_prefix_normalizer;

/// `bundle_v2.zig:targetFromHashbang`.
pub fn target_from_hashbang(buffer: &[u8]) -> Option<options::Target> {
    const HB: &[u8] = b"#!/usr/bin/env bun";
    if buffer.len() > HB.len() && buffer.starts_with(HB) {
        match buffer[HB.len()] {
            b'\n' | b' ' => return Some(options::Target::Bun),
            _ => {}
        }
    }
    None
}

/// `js_ast::renamer` — re-exported here so `Chunk.rs` can name it without
/// pulling `bun_js_printer` into its `use` set (the original draft used a
/// non-existent `bun_renamer` crate).
pub mod bun_renamer {
    pub use bun_js_printer::renamer::*;
    #[derive(Default)]
    pub enum ChunkRenamer {
        #[default]
        None,
        Number(Box<bun_js_printer::renamer::NumberRenamer>),
        Minify(Box<bun_js_printer::renamer::MinifyRenamer>),
    }

    impl ChunkRenamer {
        pub(crate) fn name_for_symbol(&mut self, ref_: bun_ast::Ref) -> &[u8] {
            match self {
                ChunkRenamer::None => unreachable!("ChunkRenamer not initialized"),
                ChunkRenamer::Number(r) => r.name_for_symbol(ref_),
                ChunkRenamer::Minify(r) => r.name_for_symbol(ref_),
            }
        }
        pub(crate) fn as_renamer(&mut self) -> bun_js_printer::renamer::Renamer<'_, '_> {
            match self {
                ChunkRenamer::None => unreachable!("ChunkRenamer not initialized"),
                ChunkRenamer::Number(r) => bun_js_printer::renamer::Renamer::NumberRenamer(r),
                ChunkRenamer::Minify(r) => bun_js_printer::renamer::Renamer::MinifyRenamer(r),
            }
        }
    }
}

pub mod html_import_manifest {
    use crate::Graph::Graph;
    use crate::HTMLImportManifest as real;
    use crate::{LinkerGraph, chunk::Chunk};

    pub use real::{EscapedJson, HTMLImportManifest};

    #[inline]
    pub(crate) fn format_escaped_json<'a>(
        index: u32,
        graph: &'a Graph,
        chunks: &'a [Chunk],
        linker_graph: &'a LinkerGraph,
    ) -> real::EscapedJson<'a> {
        real::HTMLImportManifest {
            index,
            graph,
            chunks,
            linker_graph,
        }
        .format_escaped_json()
    }

    pub fn write_escaped_json(
        index: u32,
        graph: &Graph,
        linker_graph: &LinkerGraph<'_>,
        chunks: &[Chunk],
        w: &mut &mut [u8],
    ) -> Result<(), core::fmt::Error> {
        let taken = core::mem::take(w);
        let mut fbs = bun_io::FixedBufferStream::new_mut(taken);
        real::write_escaped_json(index, graph, linker_graph, chunks, &mut fbs)
            .map_err(|_| core::fmt::Error)?;
        let bun_io::FixedBufferStream { buffer, pos } = fbs;
        *w = &mut buffer[pos..];
        Ok(())
    }
}

pub use crate::HTMLScanner as html_scanner;

/// `LinkerGraph.zig:JSMeta` / `WrapKind` / `ExportData` — minimal surface so
/// `LinkerContext.rs` field types resolve while `LinkerGraph.rs` is gated.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum WrapKind {
    #[default]
    None = 0,
    Cjs,
    Esm,
}

pub use crate::options_impl::PathTemplate;
pub(crate) use bun_ast::UseDirective;

/// `bundle_v2.zig:MangledProps`.
pub use bun_js_printer::MangledProps;

/// `bun.logger` — alias used by the original drafts as `crate::bun_ast::Source`.

/// `js_ast.BundledAst` (the bundler-facing AST view).
pub(crate) type JSAst<'a> = crate::BundledAst<'a>;
pub(crate) use bun_ast::{Part, Ref};

/// `bundle_v2.zig:EntryPoint` — both a struct and (via the sibling module
/// below) a namespace for `Kind`. Rust keeps types and modules in separate
/// namespaces, so `use crate::EntryPoint` imports both.
pub mod entry_point {
    use bun_collections::MultiArrayList;
    use bun_core::PathString;

    #[derive(Default)]
    pub struct EntryPoint {
        pub output_path: PathString,
        /// This is the source index of the entry point. This file must have a
        /// valid entry point kind (i.e. not "none").
        pub source_index: crate::IndexInt,
        /// Manually specified output paths are ignored when computing the
        /// default "outbase" directory.
        pub output_path_was_auto_generated: bool,
    }

    pub type List = MultiArrayList<EntryPoint>;

    bun_collections::multi_array_columns! {
        pub trait EntryPointColumns for EntryPoint {
            output_path: PathString,
            source_index: crate::IndexInt,
            output_path_was_auto_generated: bool,
        }
    }

    impl EntryPoint {
        pub type Kind = Kind;
    }

    #[repr(u8)]
    #[derive(Clone, Copy, PartialEq, Eq, Default)]
    pub enum Kind {
        #[default]
        None,
        UserSpecified,
        DynamicImport,
        Html,
    }
    impl Kind {
        #[inline]
        pub fn is_entry_point(self) -> bool {
            self != Self::None
        }
        #[inline]
        pub fn is_user_specified_entry_point(self) -> bool {
            self == Self::UserSpecified
        }
        #[inline]
        pub fn is_server_entry_point(self) -> bool {
            self == Self::UserSpecified
        }
        /// bundle_v2.zig:4021-4026.
        #[inline]
        pub fn output_kind(self) -> crate::options::OutputKind {
            match self {
                Self::UserSpecified => crate::options::OutputKind::EntryPoint,
                _ => crate::options::OutputKind::Chunk,
            }
        }
    }
}

/// `bundle_v2.zig:ImportData` / `ExportData` / `JSMeta` — see the gated
/// `bundle_v2.rs` draft body for full doc-comments.
pub mod js_meta {
    use bun_alloc::{AstAlloc, AstVec};
    use bun_ast::{Dependency, Ref};
    use bun_collections::array_hash_map::StringContext;
    use bun_collections::{ArrayHashMap, AutoContext, StringArrayHashMap};

    use crate::{ImportTracker, Index, WrapKind};

    pub struct ImportData {
        pub re_exports: AstVec<Dependency>,
        pub data: ImportTracker,
    }
    impl Default for ImportData {
        fn default() -> Self {
            Self {
                re_exports: AstAlloc::vec(),
                data: ImportTracker::default(),
            }
        }
    }
    /// Alias used by `LinkerGraph::generate_symbol_import_and_use`.
    pub(crate) type ImportToBind = ImportData;

    pub struct ExportData {
        pub potentially_ambiguous_export_star_refs: AstVec<ImportData>,
        pub data: ImportTracker,
    }
    impl Default for ExportData {
        fn default() -> Self {
            Self {
                potentially_ambiguous_export_star_refs: AstAlloc::vec(),
                data: ImportTracker::default(),
            }
        }
    }
    /// Alias used by `LinkerGraph::load`.
    pub(crate) type ResolvedExport = ExportData;

    pub type RefImportData = ArrayHashMap<Ref, ImportData, AutoContext, AstAlloc>;
    pub type ResolvedExports = StringArrayHashMap<ExportData, StringContext, AstAlloc>;
    pub type ProbablyTypescriptType = ArrayHashMap<Ref, (), AutoContext, AstAlloc>;
    pub type SortedAndFilteredExportAliases = AstVec<Box<[u8], AstAlloc>>;
    pub type CjsExportCopies = AstVec<Ref>;
    pub type TopLevelSymbolToParts = bun_ast::ast_result::TopLevelSymbolToParts;

    #[derive(Clone, Copy, Default)]
    pub struct Flags {
        pub is_async_or_has_async_dependency: bool,
        pub needs_exports_variable: bool,
        pub force_include_exports_for_entry_point: bool,
        pub needs_export_symbol_from_runtime: bool,
        pub did_wrap_dependencies: bool,
        pub needs_synthetic_default_export: bool,
        pub wrap: WrapKind,
    }
    /// `JSMeta.Wrap` alias used by `linker_context/` submodules.
    pub use crate::WrapKind as Wrap;

    pub struct JSMeta {
        pub probably_typescript_type: ProbablyTypescriptType,
        pub imports_to_bind: RefImportData,
        pub resolved_exports: ResolvedExports,
        pub resolved_export_star: ExportData,
        pub sorted_and_filtered_export_aliases: SortedAndFilteredExportAliases,
        pub top_level_symbol_to_parts_overlay: TopLevelSymbolToParts,
        pub cjs_export_copies: CjsExportCopies,
        pub wrapper_part_index: Index,
        pub entry_point_part_index: Index,
        pub flags: Flags,
    }

    impl Default for JSMeta {
        fn default() -> Self {
            Self {
                probably_typescript_type: ProbablyTypescriptType::default(),
                imports_to_bind: RefImportData::default(),
                resolved_exports: ResolvedExports::default(),
                resolved_export_star: ExportData::default(),
                sorted_and_filtered_export_aliases: AstAlloc::vec(),
                top_level_symbol_to_parts_overlay: TopLevelSymbolToParts::default(),
                cjs_export_copies: AstAlloc::vec(),
                wrapper_part_index: Index::default(),
                entry_point_part_index: Index::default(),
                flags: Flags::default(),
            }
        }
    }

    bun_collections::multi_array_columns! {
        pub trait JSMetaColumns for JSMeta {
            probably_typescript_type: ProbablyTypescriptType,
            imports_to_bind: RefImportData,
            resolved_exports: ResolvedExports,
            resolved_export_star: ExportData,
            sorted_and_filtered_export_aliases: SortedAndFilteredExportAliases,
            top_level_symbol_to_parts_overlay: TopLevelSymbolToParts,
            cjs_export_copies: CjsExportCopies,
            wrapper_part_index: Index,
            entry_point_part_index: Index,
            flags: Flags,
        }
    }

    /// Inherent associated types so `JSMeta::Flags` / `JSMeta::Wrap` resolve
    /// (Zig nests them under the struct). A sibling `pub mod JSMeta` would
    /// collide with the struct re-export (E0255).
    impl JSMeta {
        pub type Flags = Flags;
        pub type Wrap = crate::WrapKind;
    }
}
pub use js_meta::{
    CjsExportCopies, ExportData, ImportData, JSMeta, JSMetaColumns, ProbablyTypescriptType,
    RefImportData, ResolvedExports, SortedAndFilteredExportAliases, TopLevelSymbolToParts,
};

/// Re-export of the SoA accessor trait so callers can
/// `use crate::ungate_support::EntryPointColumns as _;`.
pub use entry_point::EntryPointColumns;

pub use crate::linker_context_mod::EventLoop;

// crate-private aliases mirroring Zig's `Index.Int` / `Part.List` /
// `ImportRecord.List` nesting.
pub(crate) mod index {
    pub(crate) use bun_ast::IndexInt as Int;
}
pub(crate) mod part {
    pub(crate) use bun_ast::PartList as List;
}
pub(crate) mod import_record {
    pub(crate) use bun_ast::import_record::List;
}
