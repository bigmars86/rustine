//! Output tree builder: constructs a hierarchical node tree from runtime actions.
//!
//! Nodes are stored in a **flat arena** (`Vec<Node>`) during tree construction.
//! Each node references its children by `NodeId` (u32 index into the arena)
//! instead of owning a `Vec<Node>`.  This puts all nodes in **one contiguous
//! allocation**, dramatically reducing allocator fragmentation.
//!
//! After execution, [`OutputTree::compact_and_flatten()`] converts the arena
//! into a [`FlatTree`] (BFS-order `Vec<FlatNode>`) and drops the arena in one
//! shot — the OS can reclaim all tree-build memory immediately.

use smallvec::SmallVec;
use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, BuildHasherDefault, Hasher};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// NodeId — typed index into the node arena
// ---------------------------------------------------------------------------

/// Index into the node arena (`OutputTree.arena`).
pub type NodeId = u32;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Compute a 64-bit hash of the compound (name + attributes) key; the order of
/// the attributes does not matter.
///
/// A hash instead of a key `String` per call keeps peak RSS low on large trees.
/// `state` holds random keys, so inputs cannot be prepared to collide.
fn child_hash_key<K: AsRef<str>, V: AsRef<str>>(state: &RandomState, name: &str, attribs: &[(K, V)]) -> u64 {
    let attrs = attribs.iter().fold(0u64, |sum, (k, v)| {
        sum.wrapping_add(state.hash_one((k.as_ref(), v.as_ref())))
    });
    state.hash_one((name, attrs))
}

/// The same attributes in any order (keys are unique, see [`parse_path`]).
fn same_attrs(a: &[(Rc<str>, String)], b: &[(String, String)]) -> bool {
    a.len() == b.len()
        && b.iter()
            .all(|(k, v)| a.iter().any(|(k2, v2)| &**k2 == k.as_str() && v2 == v))
}

/// Intern a string in the given table, returning a shared `Rc<str>`.
/// Duplicate names share a single allocation.
fn intern(table: &mut HashSet<Rc<str>>, s: &str) -> Rc<str> {
    if let Some(existing) = table.get(s) {
        return Rc::clone(existing);
    }
    let rc: Rc<str> = Rc::from(s);
    table.insert(Rc::clone(&rc));
    rc
}

// ---------------------------------------------------------------------------
// NodeIndex — auxiliary O(1) child lookup for high-fanout nodes
// ---------------------------------------------------------------------------

/// Auxiliary child-index for interior nodes with many children.
///
/// Boxed behind `Option<Box<NodeIndex>>` in [`Node`] so that leaf nodes
/// and low-fanout nodes carry only a null pointer (8 bytes).
///
/// Only created when a node accumulates more than [`INDEX_THRESHOLD`]
/// children.  For nodes with few children, linear scan is faster and
/// avoids the HashMap overhead entirely.
#[derive(Debug, Clone)]
struct NodeIndex {
    /// Maps hash of compound key (name + creation attributes) → index in that
    /// node's `children: Vec<NodeId>` (NOT an arena NodeId) of the most
    /// recently added matching child.
    child_exact: HashMap<u64, usize, BuildHasherDefault<KeyHasher>>,
}

/// Uses a [`child_hash_key`] as its own hash instead of hashing it again.
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(b);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

/// Minimum number of children before a [`NodeIndex`] is created.
const INDEX_THRESHOLD: usize = 8;

impl NodeIndex {
    fn new() -> Self {
        Self {
            child_exact: HashMap::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Node — arena-allocated tree node
// ---------------------------------------------------------------------------

/// Single node in the output tree, stored in the arena.
///
/// Children are referenced by [`NodeId`] (u32 index into the arena).
/// Attributes are stored in [`OutputTree::attr_pool`] and referenced
/// by `(attr_start, attr_len)` — eliminating per-node `Vec` heap
/// allocations and reducing Node size from 80 to 64 bytes.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: Rc<str>,
    /// Start index into [`OutputTree::attr_pool`].
    pub attr_start: u32,
    /// Number of attribute pairs.
    pub attr_len: u32,
    /// Text content — boxed to keep Node small.
    pub text: Option<Box<String>>,
    /// Child node IDs (indices into the arena).
    children: Vec<NodeId>,
    /// `None` for leaf nodes and nodes with fewer than [`INDEX_THRESHOLD`] children.
    index: Option<Box<NodeIndex>>,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            name: Rc::from(""),
            attr_start: 0,
            attr_len: 0,
            text: None,
            children: Vec::new(),
            index: None,
        }
    }
}

impl Node {
    fn new(name: Rc<str>) -> Self {
        Self {
            name,
            attr_start: 0,
            attr_len: 0,
            text: None,
            children: Vec::new(),
            index: None,
        }
    }
}

// ---------------------------------------------------------------------------
// FlatTree: contiguous arena representation for serialization
// ---------------------------------------------------------------------------

/// A node in the flattened tree.
///
/// Stores children as `(start, len)` index ranges into the parent
/// [`FlatTree`]'s node `Vec` instead of owning a `Vec<Node>`.
/// Attributes are stored in [`FlatTree::attrs`] and referenced by
/// `(attr_start, attr_len)` — all 900K+ nodes share one contiguous
/// pool, eliminating hundreds of thousands of small `Vec` allocations.
///
/// Size: 40 bytes (down from 56 with inline `Vec`).
#[derive(Debug, Clone)]
pub struct FlatNode {
    pub name: Rc<str>,
    /// Start index of this node's attributes in [`FlatTree::attrs`].
    pub attr_start: u32,
    /// Number of attributes.
    pub attr_len: u32,
    pub text: Option<Box<String>>,
    /// Start index of this node's children in [`FlatTree::nodes`].
    pub children_start: u32,
    /// Number of children.
    pub children_len: u32,
}

/// Flattened tree: all nodes in a single contiguous `Vec<FlatNode>`.
///
/// Built from a compacted arena via BFS.  Children of each node form a
/// contiguous sub-slice, enabling efficient group iteration.
/// Attributes are pooled in a single `Vec` to avoid per-node allocations.
#[derive(Debug, Clone)]
pub struct FlatTree {
    pub nodes: Vec<FlatNode>,
    /// Pooled attribute pairs for all nodes — each `FlatNode` references
    /// a contiguous slice `[attr_start .. attr_start + attr_len]`.
    pub attrs: Vec<(Rc<str>, String)>,
}

impl FlatTree {
    /// Convert a compacted arena into a flat BFS-ordered tree.
    ///
    /// Consumes the arena `Vec<Node>` and attribute pool — after this
    /// returns, all arena memory is freed in one shot (no fragmentation).
    ///
    /// The attribute pool transfers directly from `OutputTree` to `FlatTree`
    /// without copying — zero-cost ownership transfer.
    pub fn from_arena(mut arena: Vec<Node>, attrs: Vec<(Rc<str>, String)>, root: NodeId) -> Self {
        let total = arena.len();
        let mut nodes: Vec<FlatNode> = Vec::with_capacity(total);
        let mut queue: std::collections::VecDeque<NodeId> = std::collections::VecDeque::new();
        queue.push_back(root);

        let mut next_child_start: u32 = 1;

        while let Some(nid) = queue.pop_front() {
            // Take the node out of the arena (replace with default).
            // This moves the owned data (name, text) into the FlatNode
            // without cloning — zero-copy transfer.  Attribute data stays
            // in the pool (only (start, len) are copied).
            let node = std::mem::take(&mut arena[nid as usize]);
            let children_len = node.children.len() as u32;
            let children_start = if children_len > 0 { next_child_start } else { 0 };
            next_child_start += children_len;
            for &child_id in &node.children {
                queue.push_back(child_id);
            }
            nodes.push(FlatNode {
                name: node.name,
                attr_start: node.attr_start,
                attr_len: node.attr_len,
                text: node.text,
                children_start,
                children_len,
            });
        }

        // Arena is dropped here — the single large Vec<Node> allocation
        // plus all small children Vec<NodeId> are freed cleanly.
        // The attr pool was already moved into FlatTree (zero-copy).
        FlatTree { nodes, attrs }
    }

    /// Access the root node (always at index 0).
    #[inline]
    pub fn root(&self) -> &FlatNode {
        &self.nodes[0]
    }

    /// Get the attributes of a node as a contiguous slice from the pool.
    #[inline]
    pub fn attrs_of(&self, node: &FlatNode) -> &[(Rc<str>, String)] {
        if node.attr_len == 0 {
            return &[];
        }
        let start = node.attr_start as usize;
        &self.attrs[start..start + node.attr_len as usize]
    }

    /// Get the children of a node as a contiguous slice.
    #[inline]
    pub fn children_of(&self, node: &FlatNode) -> &[FlatNode] {
        if node.children_len == 0 {
            return &[];
        }
        let start = node.children_start as usize;
        &self.nodes[start..start + node.children_len as usize]
    }

    /// Iterate over contiguous groups of children sharing the same tag name.
    ///
    /// Uses `Rc::ptr_eq` on interned names for O(1) group-boundary detection.
    #[inline]
    pub fn iter_child_groups<'a>(&'a self, node: &'a FlatNode) -> FlatChildGroupIter<'a> {
        FlatChildGroupIter {
            children: self.children_of(node),
            pos: 0,
        }
    }

    /// Total number of nodes in the flattened tree.
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Iterator over contiguous groups of [`FlatNode`] children sharing the
/// same tag name.
pub struct FlatChildGroupIter<'a> {
    children: &'a [FlatNode],
    pos: usize,
}

impl<'a> Iterator for FlatChildGroupIter<'a> {
    type Item = &'a [FlatNode];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.children.len() {
            return None;
        }
        let start = self.pos;
        let name_ptr = &self.children[start].name;
        self.pos += 1;
        while self.pos < self.children.len() && Rc::ptr_eq(&self.children[self.pos].name, name_ptr) {
            self.pos += 1;
        }
        Some(&self.children[start..self.pos])
    }
}

// ---------------------------------------------------------------------------
// URL percent-encode / -decode
// ---------------------------------------------------------------------------

/// Decode `%XX` sequences; `+` stays `+`.
pub fn percent_decode(input: &str) -> String {
    if !input.contains('%') {
        return input.to_string();
    }
    percent_decode_slow(input)
}

/// Percent-decode, taking ownership of the input String.
/// Returns it unchanged (zero-copy) when no decoding is needed.
pub fn percent_decode_owned(input: String) -> String {
    if !input.contains('%') {
        return input;
    }
    percent_decode_slow(&input)
}

fn percent_decode_slow(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

/// Percent-encode every byte except ASCII letters, digits, `-_.~` and space.
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    percent_encode_into(&mut out, input);
    out
}

/// [`percent_encode`] appending to `out`.
pub fn percent_encode_into(out: &mut String, input: &str) {
    let mut run = 0;
    for (i, b) in input.bytes().enumerate() {
        if !matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b' ') {
            if run < i {
                out.push_str(&input[run..i]);
            }
            out.push('%');
            out.push(char::from(HEX_UPPER[(b >> 4) as usize]));
            out.push(char::from(HEX_UPPER[(b & 0xF) as usize]));
            run = i + 1;
        }
    }
    if run < input.len() {
        out.push_str(&input[run..]);
    }
}

static HEX_UPPER: [u8; 16] = *b"0123456789ABCDEF";

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// PathSegment + parse_path
// ---------------------------------------------------------------------------

/// Simplified path segment representation: name plus optional attribute map string (raw)
#[derive(Debug, Clone)]
pub struct PathSegment {
    pub tag: String,
    pub attribs: Vec<(String, String)>,
}

/// Parse a slash-separated path string into [`PathSegment`]s.
///
/// Tag names and attribute keys are lowercased; of repeated attribute keys the
/// first one counts.
pub fn parse_path(path: &str) -> Vec<PathSegment> {
    parse_path_with_case(path, false)
}

/// Like [`parse_path`], but keeps the case of tag names and attribute keys
/// when `preserve_case` is true.
pub fn parse_path_with_case(path: &str, preserve_case: bool) -> Vec<PathSegment> {
    let parts = split_path_segments(path);
    let mut segments = Vec::with_capacity(parts.len());
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let mut tag = part;
        let mut attribs: Vec<(String, String)> = Vec::new();
        if let Some(qpos) = part.find('?') {
            tag = &part[..qpos];
            let query = &part[qpos + 1..];
            for pair in query.split('&') {
                if pair.is_empty() {
                    continue;
                }
                if let Some(epos) = pair.find('=') {
                    let key = normalize_name(pair[..epos].trim(), preserve_case);
                    let mut val = pair[epos + 1..].trim();
                    if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                        val = &val[1..val.len() - 1];
                    }
                    attribs.push((percent_decode_owned(key), percent_decode(val)));
                }
            }
            let mut i = 1;
            while i < attribs.len() {
                if attribs[..i].iter().any(|(k, _)| *k == attribs[i].0) {
                    attribs.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        let decoded = tag.contains('%').then(|| percent_decode_slow(tag));
        let tag_norm = normalize_name(decoded.as_deref().unwrap_or(tag).trim(), preserve_case);
        segments.push(PathSegment { tag: tag_norm, attribs });
    }
    segments
}

/// Spaces become `-`; lowercase unless `preserve_case`.
fn normalize_name(name: &str, preserve_case: bool) -> String {
    let plain = !name
        .bytes()
        .any(|b| b == b' ' || (!preserve_case && (b.is_ascii_uppercase() || !b.is_ascii())));
    if plain {
        return name.to_owned();
    }
    let name = name.replace(' ', "-");
    if preserve_case {
        name
    } else {
        name.to_lowercase()
    }
}

/// Split a path string on `/` but respect double-quoted attribute values.
fn split_path_segments(path: &str) -> SmallVec<[&str; 4]> {
    let mut parts = SmallVec::new();
    let mut start = 0;
    let mut in_quotes = false;
    for (i, b) in path.bytes().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b'/' if !in_quotes => {
                parts.push(&path[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&path[start..]);
    parts
}

// ---------------------------------------------------------------------------
// OutputTree — arena-backed hierarchical tree with cursor stack
// ---------------------------------------------------------------------------

/// Hierarchical output tree with a cursor stack.
///
/// All nodes live in a single `arena: Vec<Node>`.  Attributes are pooled
/// in `attr_pool: Vec<(Rc<str>, String)>` — each node references its
/// attributes via `(attr_start, attr_len)`.  The cursor stack holds
/// [`NodeId`]s for O(1) navigation (no root-relative path traversal).
#[derive(Debug, Clone)]
pub struct OutputTree {
    arena: Vec<Node>,
    /// All attribute pairs for all nodes — each node references a contiguous
    /// slice via `(attr_start, attr_len)`.  The pool transfers directly to
    /// [`FlatTree`] during `compact_and_flatten()` (zero-copy ownership move).
    attr_pool: Vec<(Rc<str>, String)>,
    root_id: NodeId,
    /// Cursor stack: each entry is a NodeId.
    stack: Vec<NodeId>,
    name_intern: HashSet<Rc<str>>,
    /// Random keys for [`child_hash_key`].
    key_state: RandomState,
    /// Number of attributes a node was created with, for nodes that `add_attribute`
    /// extended later: name and creation attributes identify a node.
    key_attr_lens: HashMap<NodeId, u32>,
    /// Keep the case of tag names and attribute keys in paths.
    preserve_case: bool,
}

impl Default for OutputTree {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputTree {
    pub fn new() -> Self {
        Self::with_capacity_hint(0)
    }

    /// Create an OutputTree with an arena pre-sized based on expected input bytes.
    ///
    /// Heuristic: ~45 nodes per KB of input text.  Pre-sizing avoids
    /// geometric-growth doublings that temporarily double peak RSS
    /// (e.g., 512K→1024K transition wastes ~40 MB on large workloads).
    pub fn with_capacity_hint(input_bytes: usize) -> Self {
        let mut name_intern = HashSet::with_capacity(64);
        let dot = intern(&mut name_intern, ".");
        // Heuristic: ~45 nodes per KB, minimum 1024.
        let estimated_nodes = if input_bytes > 0 {
            (input_bytes / 1024 * 45).max(1024)
        } else {
            1024
        };
        // Heuristic: ~1.66 attrs per node (IOS XR: 1.49M attrs / 900K nodes).
        let estimated_attrs = estimated_nodes * 5 / 3;
        let mut arena = Vec::with_capacity(estimated_nodes);
        let root = Node::new(dot);
        arena.push(root);
        let root_id: NodeId = 0;
        Self {
            arena,
            attr_pool: Vec::with_capacity(estimated_attrs),
            root_id,
            stack: vec![root_id],
            name_intern,
            key_state: RandomState::new(),
            key_attr_lens: HashMap::new(),
            preserve_case: false,
        }
    }

    /// Keep the case of tag names and attribute keys in `out.*` paths instead
    /// of lowercasing them.
    pub fn set_preserve_case(&mut self, preserve_case: bool) {
        self.preserve_case = preserve_case;
    }

    /// Parse `path` with this tree's case setting.
    pub fn path_segments(&self, path: &str) -> Vec<PathSegment> {
        parse_path_with_case(path, self.preserve_case)
    }

    /// Number of nodes allocated in the arena.
    pub fn node_count(&self) -> usize {
        self.arena.len()
    }

    /// Number of children of the root node (for diagnostics).
    pub fn root_child_count(&self) -> usize {
        self.arena[self.root_id as usize].children.len()
    }

    // === Attribute pool helpers ===

    /// Intern and append raw `(String, String)` attributes directly to the pool.
    /// Avoids creating a temporary `Vec<(Rc<str>, String)>`.
    #[inline]
    fn set_node_attrs_raw(&mut self, node_id: NodeId, raw: &[(String, String)]) {
        let start = self.attr_pool.len() as u32;
        for (k, v) in raw {
            let key = intern(&mut self.name_intern, k);
            self.attr_pool.push((key, v.clone()));
        }
        let len = (self.attr_pool.len() as u32) - start;
        let node = &mut self.arena[node_id as usize];
        node.attr_start = start;
        node.attr_len = len;
    }

    /// Push a single attribute to a node.  If the node's attrs are at the
    /// end of the pool, appends in-place.  Otherwise, relocates the range
    /// to the end first (the old range becomes dead space — rare case).
    fn push_node_attr(&mut self, node_id: NodeId, key: Rc<str>, value: String) {
        let node = &self.arena[node_id as usize];
        self.key_attr_lens.entry(node_id).or_insert(node.attr_len);
        let end = (node.attr_start + node.attr_len) as usize;
        if end != self.attr_pool.len() {
            // Relocate: move existing attrs to end of pool (zero-copy via mem::take).
            let old_start = node.attr_start as usize;
            let old_len = node.attr_len as usize;
            let new_start = self.attr_pool.len();
            for j in old_start..old_start + old_len {
                let entry = std::mem::take(&mut self.attr_pool[j]);
                self.attr_pool.push(entry);
            }
            self.arena[node_id as usize].attr_start = new_start as u32;
        }
        self.attr_pool.push((key, value));
        self.arena[node_id as usize].attr_len += 1;
    }

    // === Arena helpers ===

    /// Allocate a new node in the arena, returning its NodeId.
    #[inline]
    fn alloc(&mut self, node: Node) -> NodeId {
        let id = self.arena.len() as NodeId;
        self.arena.push(node);
        id
    }

    /// Current cursor NodeId.
    #[inline]
    fn cursor(&self) -> NodeId {
        *self.stack.last().unwrap()
    }

    // === Child management ===

    /// Push a child to `parent_id` and update the index.
    fn push_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        self.push_child_keyed(parent_id, child_id, None);
    }

    /// [`push_child`](Self::push_child) with the child's [`child_hash_key`] if the caller has it.
    fn push_child_keyed(&mut self, parent_id: NodeId, child_id: NodeId, key: Option<u64>) {
        let parent = &self.arena[parent_id as usize];
        let idx = parent.children.len();
        // Small parents are scanned linearly and need no keys.
        if parent.index.is_some() || idx == INDEX_THRESHOLD - 1 {
            if parent.index.is_none() {
                let mut ni = Box::new(NodeIndex::new());
                for (i, &cid) in parent.children.iter().enumerate() {
                    ni.child_exact.insert(self.node_key(cid), i);
                }
                self.arena[parent_id as usize].index = Some(ni);
            }
            let key = key.unwrap_or_else(|| self.node_key(child_id));
            if let Some(index) = &mut self.arena[parent_id as usize].index {
                index.child_exact.insert(key, idx);
            }
        }
        self.arena[parent_id as usize].children.push(child_id);
    }

    /// The attributes `id` was created with.
    fn key_attrs(&self, id: NodeId) -> &[(Rc<str>, String)] {
        let c = &self.arena[id as usize];
        let len = if self.key_attr_lens.is_empty() {
            c.attr_len
        } else {
            self.key_attr_lens.get(&id).copied().unwrap_or(c.attr_len)
        };
        &self.attr_pool[c.attr_start as usize..(c.attr_start + len) as usize]
    }

    /// [`child_hash_key`] of an existing node.
    fn node_key(&self, id: NodeId) -> u64 {
        child_hash_key(&self.key_state, &self.arena[id as usize].name, self.key_attrs(id))
    }

    /// The child of `parent_id` with this name and creation attributes (in any
    /// order), the most recently added one if several match. Also returns the
    /// [`child_hash_key`] if it was computed, for [`push_child_keyed`](Self::push_child_keyed).
    fn find_child(&self, parent_id: NodeId, tag: &str, attribs: &[(String, String)]) -> (Option<NodeId>, Option<u64>) {
        let parent = &self.arena[parent_id as usize];
        if let Some(index) = &parent.index {
            let key = child_hash_key(&self.key_state, tag, attribs);
            (index.child_exact.get(&key).map(|&i| parent.children[i]), Some(key))
        } else {
            let found = parent
                .children
                .iter()
                .rev()
                .copied()
                .find(|&cid| *self.arena[cid as usize].name == *tag && same_attrs(self.key_attrs(cid), attribs));
            (found, None)
        }
    }

    /// [`find_child`](Self::find_child) for a path segment.
    fn find_seg(&self, parent_id: NodeId, seg: &PathSegment) -> Option<NodeId> {
        self.find_child(parent_id, &seg.tag, &seg.attribs).0
    }

    // === Compact + flatten ===

    /// Compact the tree and convert to a [`FlatTree`], consuming the arena.
    ///
    /// 1. Reorders children so same-named siblings are contiguous.
    /// 2. Drops build-time indices.
    /// 3. Converts to BFS-ordered `FlatTree`.
    /// 4. The arena is freed in one shot — minimal allocator fragmentation.
    pub fn compact_and_flatten(mut self) -> FlatTree {
        self.compact_node(self.root_id);
        self.attr_pool.shrink_to_fit();
        FlatTree::from_arena(self.arena, self.attr_pool, self.root_id)
    }

    /// Recursively compact a node and its descendants.
    fn compact_node(&mut self, node_id: NodeId) {
        // Collect child IDs to recurse (clone the small Vec<u32>).
        let child_ids: Vec<NodeId> = self.arena[node_id as usize].children.clone();
        for &cid in &child_ids {
            self.compact_node(cid);
        }

        // Drop the index.
        self.arena[node_id as usize].index = None;

        let node = &self.arena[node_id as usize];
        if node.children.len() > 1 {
            // Compute child groups on-the-fly.
            let mut group_map: HashMap<Rc<str>, usize> = HashMap::new();
            let mut groups: Vec<SmallVec<[usize; 1]>> = Vec::new();
            for (i, &cid) in node.children.iter().enumerate() {
                let child_name = &self.arena[cid as usize].name;
                if let Some(&gi) = group_map.get(child_name) {
                    groups[gi].push(i);
                } else {
                    group_map.insert(Rc::clone(child_name), groups.len());
                    groups.push(SmallVec::from_buf([i]));
                }
            }

            // Check if already in group order.
            let needs_reorder = {
                let mut pos = 0usize;
                let mut ok = true;
                'outer: for group in &groups {
                    for &idx in group.iter() {
                        if idx != pos {
                            ok = false;
                            break 'outer;
                        }
                        pos += 1;
                    }
                }
                !ok
            };

            if needs_reorder {
                let old_children = self.arena[node_id as usize].children.clone();
                let node = &mut self.arena[node_id as usize];
                node.children.clear();
                for group in &groups {
                    for &idx in group.iter() {
                        node.children.push(old_children[idx]);
                    }
                }
            }
        }

        let node = &mut self.arena[node_id as usize];
        node.children.shrink_to_fit();
    }

    // === Tree navigation and mutation ===

    /// Walk `segs` from the cursor, creating missing nodes; with `new_leaf` the
    /// last segment always gets a new node. `.` stays at the current node.
    fn walk(&mut self, segs: &[PathSegment], new_leaf: bool) -> NodeId {
        let mut current = self.cursor();
        for (i, seg) in segs.iter().enumerate() {
            if seg.tag == "." {
                continue;
            }
            let found = if new_leaf && i == segs.len() - 1 {
                None
            } else {
                self.find_seg(current, seg)
            };
            current = match found {
                Some(id) => id,
                None => {
                    let name = intern(&mut self.name_intern, &seg.tag);
                    let id = self.alloc(Node::new(name));
                    self.set_node_attrs_raw(id, &seg.attribs);
                    self.push_child(current, id);
                    id
                }
            };
        }
        current
    }

    pub fn create_path(&mut self, segments: &[PathSegment], text: Option<String>) {
        if segments.is_empty() {
            return;
        }
        let id = self.walk(segments, true);
        self.arena[id as usize].text = text.map(Box::new);
    }

    /// Find or create the node and append `text` to its text.
    pub fn add_path(&mut self, segments: &[PathSegment], text: Option<String>) {
        if segments.is_empty() {
            return;
        }
        let id = self.walk(segments, false);
        if let Some(t) = text {
            let node = &mut self.arena[id as usize];
            match &mut node.text {
                Some(existing) => existing.push_str(&t),
                None => node.text = Some(Box::new(t)),
            }
        }
    }

    /// Find or create the node like [`add_path`](Self::add_path) and replace its text.
    pub fn replace_path(&mut self, segments: &[PathSegment], text: Option<String>) {
        if segments.is_empty() {
            return;
        }
        let id = self.walk(segments, false);
        self.arena[id as usize].text = text.map(Box::new);
    }

    /// Find or create the node and append an attribute; the node keeps its identity.
    pub fn add_attribute(&mut self, segments: &[PathSegment], name: &str, value: &str) {
        if segments.is_empty() {
            return;
        }
        let id = self.walk(segments, false);
        let attr_name = intern(&mut self.name_intern, name);
        self.push_node_attr(id, attr_name, value.to_string());
    }

    pub fn set_root_name(&mut self, name: &str) {
        self.arena[self.root_id as usize].name = intern(&mut self.name_intern, name);
    }

    /// Push a new node at the path onto the cursor stack.
    pub fn open_path(&mut self, segments: &[PathSegment]) {
        // A path without segments pushes the current node, so the matching leave stays balanced.
        let id = self.walk(segments, true);
        self.stack.push(id);
    }

    /// Push the node at the path (found or created) onto the cursor stack.
    pub fn enter_path(&mut self, segments: &[PathSegment]) {
        let id = self.walk(segments, false);
        self.stack.push(id);
    }

    pub fn leave(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }
}

// ---------------------------------------------------------------------------
// RuntimeAction + ActionExecutor
// ---------------------------------------------------------------------------

/// High-level action a grammar step can emit to manipulate the output tree.
pub enum RuntimeAction<'a> {
    OutCreate {
        path: &'a str,
        value: Option<&'a str>,
    },
    OutAdd {
        path: &'a str,
        value: Option<&'a str>,
    },
    OutReplace {
        path: &'a str,
        value: Option<&'a str>,
    },
    OutAddAttribute {
        path: &'a str,
        name: &'a str,
        value: &'a str,
    },
    OutSetRootName {
        name: &'a str,
    },
    OutOpen {
        path: &'a str,
    },
    OutEnter {
        path: &'a str,
    },
    OutLeave,
}

/// Trait for types that can execute [`RuntimeAction`]s.
pub trait ActionExecutor {
    /// Apply a single runtime action (e.g. create node, add attribute, leave scope).
    fn exec(&mut self, act: RuntimeAction);
}

impl ActionExecutor for OutputTree {
    fn exec(&mut self, act: RuntimeAction) {
        match act {
            RuntimeAction::OutCreate { path, value } => {
                let decoded = value.map(percent_decode);
                if path == "." {
                    let cur = self.cursor();
                    self.arena[cur as usize].text = decoded.map(Box::new);
                } else if !path.is_empty() && !path.contains('/') && !path.starts_with('.') {
                    let segs = self.path_segments(path);
                    let name = intern(&mut self.name_intern, &segs[0].tag);
                    let node = Node::new(name);
                    let nid = self.alloc(node);
                    self.set_node_attrs_raw(nid, &segs[0].attribs);
                    self.arena[nid as usize].text = decoded.map(Box::new);
                    let cur = self.cursor();
                    self.push_child(cur, nid);
                } else {
                    let segs = self.path_segments(path);
                    self.create_path(&segs, decoded);
                }
            }
            RuntimeAction::OutAdd { path, value } => {
                let decoded = value.map(percent_decode);
                if path == "." {
                    let cur = self.cursor();
                    if let Some(val) = decoded {
                        let target = &mut self.arena[cur as usize];
                        if let Some(existing) = &mut target.text {
                            existing.push_str(&val);
                        } else {
                            target.text = Some(Box::new(val));
                        }
                    }
                } else if !path.is_empty() && !path.contains('/') && !path.starts_with('.') {
                    // relative add — use indexed or linear lookup
                    let segs = self.path_segments(path);
                    let cur = self.cursor();
                    let (existing_child_id, key) = self.find_child(cur, &segs[0].tag, &segs[0].attribs);
                    if let Some(cid) = existing_child_id {
                        if let Some(val) = &decoded {
                            let child = &mut self.arena[cid as usize];
                            if let Some(existing) = &mut child.text {
                                existing.push_str(val);
                            } else {
                                child.text = Some(Box::new(val.clone()));
                            }
                        }
                    } else {
                        let name = intern(&mut self.name_intern, &segs[0].tag);
                        let node = Node::new(name);
                        let nid = self.alloc(node);
                        self.set_node_attrs_raw(nid, &segs[0].attribs);
                        self.arena[nid as usize].text = decoded.map(Box::new);
                        self.push_child_keyed(cur, nid, key);
                    }
                } else {
                    let segs = self.path_segments(path);
                    self.add_path(&segs, decoded);
                }
            }
            RuntimeAction::OutReplace { path, value } => {
                let decoded = value.map(percent_decode);
                if path == "." {
                    let cur = self.cursor();
                    self.arena[cur as usize].text = decoded.map(Box::new);
                } else {
                    let segs = self.path_segments(path);
                    self.replace_path(&segs, decoded);
                }
            }
            RuntimeAction::OutAddAttribute { path, name, value } => {
                if path == "." {
                    let attr_key = intern(&mut self.name_intern, name);
                    let cur = self.cursor();
                    self.push_node_attr(cur, attr_key, value.to_string());
                } else {
                    let segs = self.path_segments(path);
                    self.add_attribute(&segs, name, value);
                }
            }
            RuntimeAction::OutSetRootName { name } => {
                self.set_root_name(name);
            }
            RuntimeAction::OutOpen { path } => {
                let segs = self.path_segments(path);
                self.open_path(&segs);
            }
            RuntimeAction::OutEnter { path } => {
                let segs = self.path_segments(path);
                self.enter_path(&segs);
            }
            RuntimeAction::OutLeave => {
                self.leave();
            }
        }
    }
}
