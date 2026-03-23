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
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// NodeId — typed index into the node arena
// ---------------------------------------------------------------------------

/// Index into the node arena (`OutputTree.arena`).
pub type NodeId = u32;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Compute a 64-bit hash of the compound (name + attributes) key.
///
/// This replaces the old `child_lookup_key()` which allocated a fresh
/// `String` per call.  Using a hash saves ~70 MB of peak RSS on large
/// workloads (900K nodes × ~80 bytes per key String).
fn child_hash_key<K: AsRef<str>, V: AsRef<str>>(name: &str, attribs: &[(K, V)]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut h);
    for (k, v) in attribs {
        0u8.hash(&mut h); // separator
        k.as_ref().hash(&mut h);
        1u8.hash(&mut h); // separator
        v.as_ref().hash(&mut h);
    }
    h.finish()
}

/// Intern attribute keys from a `[(String,String)]` slice into `[(Rc<str>,String)]`.
#[allow(dead_code)]
fn intern_attrs(table: &mut HashSet<Rc<str>>, attribs: &[(String, String)]) -> Vec<(Rc<str>, String)> {
    attribs.iter()
        .map(|(k, v)| (intern(table, k), v.clone()))
        .collect()
}

/// Compare interned attributes with plain-string attributes.
fn attrs_eq(a: &[(Rc<str>, String)], b: &[(String, String)]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|((k1, v1), (k2, v2))| &**k1 == k2.as_str() && v1 == v2)
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
    /// Maps hash of compound key (name + attributes) → index in that
    /// node's `children: Vec<NodeId>` (NOT an arena NodeId).
    child_exact: HashMap<u64, usize>,
}

/// Minimum number of children before a [`NodeIndex`] is created.
const INDEX_THRESHOLD: usize = 8;

impl NodeIndex {
    fn new() -> Self {
        Self {
            child_exact: HashMap::new(),
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
        while self.pos < self.children.len()
            && Rc::ptr_eq(&self.children[self.pos].name, name_ptr)
        {
            self.pos += 1;
        }
        Some(&self.children[start..self.pos])
    }
}

// ---------------------------------------------------------------------------
// URL percent-encode / -decode
// ---------------------------------------------------------------------------

/// URL percent-decode (equivalent to Python's `urllib.parse.unquote`).
/// Decodes %XX sequences.  Does NOT convert '+' to space (that is
/// `unquote_plus`, which Gelatin does not use).
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

/// URL percent-encode (equivalent to Python's `urllib.parse.quote(s, safe=' ')`).
pub fn percent_encode(input: &str) -> String {
    let needs_encoding = input.bytes().any(|b| !matches!(b,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
        | b'-' | b'_' | b'.' | b'~' | b' '
    ));
    if !needs_encoding {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + input.len() / 4);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' | b' ' => out.push(b as char),
            _ => {
                out.push('%');
                out.push(char::from(HEX_UPPER[(b >> 4) as usize]));
                out.push(char::from(HEX_UPPER[(b & 0xF) as usize]));
            }
        }
    }
    out
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
pub fn parse_path(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    for part in split_path_segments(path) {
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
                    let key = pair[..epos].trim().to_lowercase().replace(' ', "-");
                    let mut val = pair[epos + 1..].trim();
                    if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                        val = &val[1..val.len() - 1];
                    }
                    attribs.push((percent_decode(&key), percent_decode(val)));
                }
            }
        }
        let tag_norm = percent_decode(tag).trim().replace(' ', "-").to_lowercase();
        segments.push(PathSegment { tag: tag_norm, attribs });
    }
    segments
}

/// Split a path string on `/` but respect double-quoted attribute values.
fn split_path_segments(path: &str) -> Vec<&str> {
    let mut parts = Vec::new();
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
    /// Cache for parsed path segments.
    path_cache: HashMap<Box<str>, Vec<PathSegment>>,
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
            path_cache: HashMap::with_capacity(128),
        }
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

    /// Get a node's attributes as a slice of the pool.
    #[inline]
    #[allow(dead_code)]
    fn node_attrs(&self, node_id: NodeId) -> &[(Rc<str>, String)] {
        let node = &self.arena[node_id as usize];
        if node.attr_len == 0 { return &[]; }
        &self.attr_pool[node.attr_start as usize..(node.attr_start + node.attr_len) as usize]
    }

    /// Append attributes to the pool and set a node's attr range.
    /// The node MUST have attr_len == 0 (freshly created).
    #[inline]
    #[allow(dead_code)]
    fn set_node_attrs(&mut self, node_id: NodeId, attrs: Vec<(Rc<str>, String)>) {
        let start = self.attr_pool.len() as u32;
        let len = attrs.len() as u32;
        self.attr_pool.extend(attrs);
        let node = &mut self.arena[node_id as usize];
        node.attr_start = start;
        node.attr_len = len;
    }

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

    /// Extend a node's attributes with additional pairs.  Relocates if
    /// the node's range is not at the pool's tail.
    #[allow(dead_code)]
    fn extend_node_attrs(&mut self, node_id: NodeId, extra: &[(Rc<str>, String)]) {
        if extra.is_empty() { return; }
        let node = &self.arena[node_id as usize];
        let end = (node.attr_start + node.attr_len) as usize;
        if end != self.attr_pool.len() {
            let old_start = node.attr_start as usize;
            let old_len = node.attr_len as usize;
            let new_start = self.attr_pool.len();
            for j in old_start..old_start + old_len {
                let entry = std::mem::take(&mut self.attr_pool[j]);
                self.attr_pool.push(entry);
            }
            self.arena[node_id as usize].attr_start = new_start as u32;
        }
        self.attr_pool.extend_from_slice(extra);
        self.arena[node_id as usize].attr_len += extra.len() as u32;
    }

    /// Replace all of a node's attributes with new ones.
    /// Old range becomes dead space (acceptable — happens rarely).
    #[allow(dead_code)]
    fn replace_node_attrs(&mut self, node_id: NodeId, attrs: Vec<(Rc<str>, String)>) {
        let start = self.attr_pool.len() as u32;
        let len = attrs.len() as u32;
        self.attr_pool.extend(attrs);
        let node = &mut self.arena[node_id as usize];
        node.attr_start = start;
        node.attr_len = len;
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
        // Compute hash key from child data before borrowing parent.
        let child = &self.arena[child_id as usize];
        let child_attrs = if child.attr_len == 0 { &[][..] } else {
            &self.attr_pool[child.attr_start as usize..(child.attr_start + child.attr_len) as usize]
        };
        let key = child_hash_key(&child.name, child_attrs);

        let parent = &self.arena[parent_id as usize];
        let idx = parent.children.len();
        let needs_index_init = idx == INDEX_THRESHOLD - 1 && parent.index.is_none();

        if needs_index_init {
            // Collect existing children's hash keys.
            let existing: Vec<(u64, usize)> = self.arena[parent_id as usize]
                .children
                .iter()
                .enumerate()
                .map(|(i, &cid)| {
                    let c = &self.arena[cid as usize];
                    let c_attrs = if c.attr_len == 0 { &[][..] } else {
                        &self.attr_pool[c.attr_start as usize..(c.attr_start + c.attr_len) as usize]
                    };
                    (child_hash_key(&c.name, c_attrs), i)
                })
                .collect();
            let parent = &mut self.arena[parent_id as usize];
            let mut ni = Box::new(NodeIndex::new());
            for (k, i) in existing {
                ni.child_exact.entry(k).or_insert(i);
            }
            parent.index = Some(ni);
        }

        let parent = &mut self.arena[parent_id as usize];
        if let Some(index) = &mut parent.index {
            index.child_exact.entry(key).or_insert(idx);
        }
        parent.children.push(child_id);
    }

    /// Find the **index in parent.children** for a matching (name, attribs) segment.
    fn find_child_idx(&self, parent_id: NodeId, seg: &PathSegment) -> Option<usize> {
        let parent = &self.arena[parent_id as usize];
        if let Some(index) = &parent.index {
            let key = child_hash_key(&seg.tag, &seg.attribs);
            index.child_exact.get(&key).copied()
        } else {
            parent.children.iter().position(|&cid| {
                let c = &self.arena[cid as usize];
                let c_attrs = if c.attr_len == 0 { &[][..] } else {
                    &self.attr_pool[c.attr_start as usize..(c.attr_start + c.attr_len) as usize]
                };
                *c.name == *seg.tag && attrs_eq(c_attrs, &seg.attribs)
            })
        }
    }

    /// Get the arena NodeId of the `child_idx`-th child of `parent_id`.
    #[inline]
    fn child_node_id(&self, parent_id: NodeId, child_idx: usize) -> NodeId {
        self.arena[parent_id as usize].children[child_idx]
    }

    /// Rebuild the child_exact index for `parent_id` after mutations.
    fn rebuild_index(&mut self, parent_id: NodeId) {
        let parent = &self.arena[parent_id as usize];
        if parent.children.len() < INDEX_THRESHOLD {
            self.arena[parent_id as usize].index = None;
            return;
        }
        let entries: Vec<(u64, usize)> = self.arena[parent_id as usize]
            .children
            .iter()
            .enumerate()
            .map(|(i, &cid)| {
                let c = &self.arena[cid as usize];
                let c_attrs = if c.attr_len == 0 { &[][..] } else {
                    &self.attr_pool[c.attr_start as usize..(c.attr_start + c.attr_len) as usize]
                };
                (child_hash_key(&c.name, c_attrs), i)
            })
            .collect();
        let index = self.arena[parent_id as usize]
            .index
            .get_or_insert_with(|| Box::new(NodeIndex::new()));
        index.child_exact.clear();
        for (k, i) in entries {
            index.child_exact.entry(k).or_insert(i);
        }
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

    // === Path cache ===

    fn cached_parse_path(&mut self, path: &str) -> Vec<PathSegment> {
        if let Some(cached) = self.path_cache.get(path) {
            return cached.clone();
        }
        let segs = parse_path(path);
        self.path_cache.insert(path.into(), segs.clone());
        segs
    }

    // === Tree navigation and mutation ===

    pub fn create_path(&mut self, segments: &[PathSegment], text: Option<String>) {
        if segments.is_empty() {
            return;
        }
        let names: Vec<Rc<str>> = segments.iter()
            .map(|s| intern(&mut self.name_intern, &s.tag))
            .collect();
        let mut current = self.cursor();
        for (i, seg) in segments.iter().enumerate() {
            let is_leaf = i == segments.len() - 1;
            if seg.tag == "." {
                if is_leaf {
                    self.arena[current as usize].text = text.clone().map(Box::new);
                }
                continue;
            }
            if is_leaf {
                let leaf = Node::new(Rc::clone(&names[i]));
                let leaf_id = self.alloc(leaf);
                self.set_node_attrs_raw(leaf_id, &seg.attribs);
                self.arena[leaf_id as usize].text = text.clone().map(Box::new);
                self.push_child(current, leaf_id);
            } else if let Some(idx) = self.find_child_idx(current, seg) {
                current = self.child_node_id(current, idx);
            } else {
                let mid = Node::new(Rc::clone(&names[i]));
                let mid_id = self.alloc(mid);
                self.set_node_attrs_raw(mid_id, &seg.attribs);
                self.push_child(current, mid_id);
                current = mid_id;
            }
        }
    }

    pub fn add_path(&mut self, segments: &[PathSegment], text: Option<String>) {
        if segments.is_empty() {
            return;
        }
        let names: Vec<Rc<str>> = segments.iter()
            .map(|s| intern(&mut self.name_intern, &s.tag))
            .collect();
        let mut current = self.cursor();
        for (i, seg) in segments.iter().enumerate() {
            let is_leaf = i == segments.len() - 1;
            if seg.tag == "." {
                if is_leaf {
                    if let Some(t) = text.clone() {
                        let node = &mut self.arena[current as usize];
                        if let Some(existing) = &mut node.text {
                            existing.push_str(&t);
                        } else {
                            node.text = Some(Box::new(t));
                        }
                    }
                }
                continue;
            }
            if let Some(idx) = self.find_child_idx(current, seg) {
                let child_id = self.child_node_id(current, idx);
                if is_leaf {
                    if let Some(t) = text.clone() {
                        let child = &mut self.arena[child_id as usize];
                        if let Some(existing) = &mut child.text {
                            existing.push_str(&t);
                        } else {
                            child.text = Some(Box::new(t));
                        }
                    }
                }
                current = child_id;
            } else {
                let node = Node::new(Rc::clone(&names[i]));
                let nid = self.alloc(node);
                self.set_node_attrs_raw(nid, &seg.attribs);
                if is_leaf {
                    self.arena[nid as usize].text = text.clone().map(Box::new);
                }
                self.push_child(current, nid);
                current = nid;
            }
        }
    }

    pub fn replace_path(&mut self, segments: &[PathSegment], text: Option<String>) {
        if segments.is_empty() {
            return;
        }
        let names: Vec<Rc<str>> = segments.iter()
            .map(|s| intern(&mut self.name_intern, &s.tag))
            .collect();
        let mut current = self.cursor();
        for (i, seg) in segments.iter().enumerate() {
            let is_leaf = i == segments.len() - 1;
            if seg.tag == "." {
                if is_leaf {
                    self.arena[current as usize].text = text.clone().map(Box::new);
                }
                continue;
            }
            if is_leaf {
                // Remove matching children, then add new.
                let arena = &self.arena;
                let pool = &self.attr_pool;
                let keep: Vec<NodeId> = arena[current as usize]
                    .children
                    .iter()
                    .copied()
                    .filter(|&cid| {
                        let c = &arena[cid as usize];
                        let c_attrs = if c.attr_len == 0 { &[][..] } else {
                            &pool[c.attr_start as usize..(c.attr_start + c.attr_len) as usize]
                        };
                        !(*c.name == *seg.tag && attrs_eq(c_attrs, &seg.attribs))
                    })
                    .collect();
                self.arena[current as usize].children = keep;
                self.rebuild_index(current);
                let leaf = Node::new(Rc::clone(&names[i]));
                let leaf_id = self.alloc(leaf);
                self.set_node_attrs_raw(leaf_id, &seg.attribs);
                self.arena[leaf_id as usize].text = text.clone().map(Box::new);
                self.push_child(current, leaf_id);
            } else if let Some(idx) = self.find_child_idx(current, seg) {
                current = self.child_node_id(current, idx);
            } else {
                let mid = Node::new(Rc::clone(&names[i]));
                let mid_id = self.alloc(mid);
                self.set_node_attrs_raw(mid_id, &seg.attribs);
                self.push_child(current, mid_id);
                current = mid_id;
            }
        }
    }

    pub fn add_attribute(&mut self, segments: &[PathSegment], name: &str, value: &str) {
        if segments.is_empty() {
            return;
        }
        let inames: Vec<Rc<str>> = segments.iter()
            .map(|s| intern(&mut self.name_intern, &s.tag))
            .collect();
        let attr_name = intern(&mut self.name_intern, name);
        let mut current = self.cursor();
        for (i, seg) in segments.iter().enumerate() {
            let is_leaf = i == segments.len() - 1;
            if seg.tag == "." {
                if is_leaf {
                    self.push_node_attr(current, Rc::clone(&attr_name), value.to_string());
                }
                continue;
            }
            if let Some(idx) = self.find_child_idx(current, seg) {
                current = self.child_node_id(current, idx);
            } else {
                let node = Node::new(Rc::clone(&inames[i]));
                let nid = self.alloc(node);
                self.set_node_attrs_raw(nid, &seg.attribs);
                self.push_child(current, nid);
                current = nid;
            }
            if is_leaf {
                self.push_node_attr(current, Rc::clone(&attr_name), value.to_string());
            }
        }
    }

    pub fn set_root_name(&mut self, name: &str) {
        self.arena[self.root_id as usize].name = intern(&mut self.name_intern, name);
    }

    pub fn open_path(&mut self, segments: &[PathSegment]) {
        self._traverse_and_push(segments, true);
    }

    pub fn enter_path(&mut self, segments: &[PathSegment]) {
        self._traverse_and_push(segments, false);
    }

    fn _traverse_and_push(&mut self, segments: &[PathSegment], create_leaf_always_new: bool) {
        if segments.is_empty() {
            return;
        }
        let new_cursor = self._traverse_from(segments, create_leaf_always_new);
        self.stack.push(new_cursor);
    }

    fn _traverse_from(
        &mut self,
        segs: &[PathSegment],
        create_leaf_always_new: bool,
    ) -> NodeId {
        let names: Vec<Rc<str>> = segs.iter()
            .map(|s| intern(&mut self.name_intern, &s.tag))
            .collect();
        let mut current = self.cursor();
        for (i, seg) in segs.iter().enumerate() {
            if seg.tag == "." {
                continue;
            }
            let is_leaf = i == segs.len() - 1;
            if is_leaf && create_leaf_always_new {
                let leaf = Node::new(Rc::clone(&names[i]));
                let leaf_id = self.alloc(leaf);
                self.set_node_attrs_raw(leaf_id, &seg.attribs);
                self.push_child(current, leaf_id);
                current = leaf_id;
            } else if let Some(idx) = self.find_child_idx(current, seg) {
                current = self.child_node_id(current, idx);
            } else {
                let mid = Node::new(Rc::clone(&names[i]));
                let mid_id = self.alloc(mid);
                self.set_node_attrs_raw(mid_id, &seg.attribs);
                self.push_child(current, mid_id);
                current = mid_id;
            }
        }
        current
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
                } else if !path.contains('/') && !path.starts_with('.') {
                    let segs = self.cached_parse_path(path);
                    let name = intern(&mut self.name_intern, &segs[0].tag);
                    let node = Node::new(name);
                    let nid = self.alloc(node);
                    self.set_node_attrs_raw(nid, &segs[0].attribs);
                    self.arena[nid as usize].text = decoded.map(Box::new);
                    let cur = self.cursor();
                    self.push_child(cur, nid);
                } else {
                    let segs = self.cached_parse_path(path);
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
                } else if !path.contains('/') && !path.starts_with('.') {
                    // relative add — use indexed or linear lookup
                    let segs = self.cached_parse_path(path);
                    let cur = self.cursor();
                    let existing_child_id = {
                        let arena = &self.arena;
                        let pool = &self.attr_pool;
                        let parent = &arena[cur as usize];
                        if let Some(index) = &parent.index {
                            let key = child_hash_key(&segs[0].tag, &segs[0].attribs);
                            index.child_exact.get(&key).map(|&idx| parent.children[idx])
                        } else {
                            parent.children.iter().copied().find(|&cid| {
                                let c = &arena[cid as usize];
                                let c_attrs = if c.attr_len == 0 { &[][..] } else {
                                    &pool[c.attr_start as usize..(c.attr_start + c.attr_len) as usize]
                                };
                                *c.name == *segs[0].tag && attrs_eq(c_attrs, &segs[0].attribs)
                            })
                        }
                    };
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
                        self.push_child(cur, nid);
                    }
                } else {
                    let segs = self.cached_parse_path(path);
                    self.add_path(&segs, decoded);
                }
            }
            RuntimeAction::OutReplace { path, value } => {
                let decoded = value.map(percent_decode);
                if path == "." {
                    let cur = self.cursor();
                    self.arena[cur as usize].text = decoded.map(Box::new);
                } else {
                    let segs = self.cached_parse_path(path);
                    self.replace_path(&segs, decoded);
                }
            }
            RuntimeAction::OutAddAttribute { path, name, value } => {
                if path == "." {
                    let attr_key = intern(&mut self.name_intern, name);
                    let cur = self.cursor();
                    self.push_node_attr(cur, attr_key, value.to_string());
                } else {
                    let segs = self.cached_parse_path(path);
                    self.add_attribute(&segs, name, value);
                }
            }
            RuntimeAction::OutSetRootName { name } => {
                self.set_root_name(name);
            }
            RuntimeAction::OutOpen { path } => {
                let segs = self.cached_parse_path(path);
                self.open_path(&segs);
            }
            RuntimeAction::OutEnter { path } => {
                let segs = self.cached_parse_path(path);
                self.enter_path(&segs);
            }
            RuntimeAction::OutLeave => {
                self.leave();
            }
        }
    }
}
