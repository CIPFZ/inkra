//! Mermaid `flowchart` / `graph` (SPEC 2.4 P1 subset): directions TD/TB/BT/LR/RL,
//! node shapes, edge styles and labels, chains, `&` groups, subgraphs. Layout
//! is layered (Sugiyama-style): ranks by longest path, crossing reduction by
//! barycenters, coordinates by neighbour averaging.

use std::collections::HashMap;

use crate::draw::{self, Rgba, Scene};
use crate::{Error, Palette};

/// Node shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `[text]`
    Rect,
    /// `(text)`
    Round,
    /// `([text])`
    Stadium,
    /// `[[text]]`
    Subroutine,
    /// `[(text)]`
    Cylinder,
    /// `((text))`
    Circle,
    /// `(((text)))`
    DoubleCircle,
    /// `>text]`
    Asymmetric,
    /// `{text}`
    Diamond,
    /// `{{text}}`
    Hexagon,
    /// `[/text/]`
    Parallelogram,
    /// `[\text\]`
    ParallelogramAlt,
    /// `[/text\]`
    Trapezoid,
    /// `[\text/]`
    TrapezoidAlt,
}

/// Edge line styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    /// `-->`, `---`
    Solid,
    /// `-.->`
    Dotted,
    /// `==>`
    Thick,
    /// `~~~` (invisible, layout only)
    Invisible,
}

/// Arrow ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    /// No head.
    None,
    /// `>`
    Arrow,
    /// `o`
    Circle,
    /// `x`
    Cross,
}

/// A node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Id.
    pub id: String,
    /// Label.
    pub label: String,
    /// Shape.
    pub shape: Shape,
    /// Fill override (`style id fill:#…`).
    pub fill: Option<Rgba>,
    /// Stroke override.
    pub stroke: Option<Rgba>,
}

/// An edge.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Source node index.
    pub from: usize,
    /// Target node index.
    pub to: usize,
    /// Label.
    pub label: String,
    /// Line style.
    pub line: Line,
    /// Head at the target.
    pub head: Head,
    /// Head at the source (`<-->`).
    pub tail: Head,
    /// Minimum rank distance (longer arrows `--->`).
    pub min_len: usize,
}

/// A subgraph.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    /// Id (the first word after `subgraph`).
    pub id: String,
    /// Title.
    pub title: String,
    /// Member nodes.
    pub nodes: Vec<usize>,
}

/// Layout direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Top to bottom.
    TB,
    /// Bottom to top.
    BT,
    /// Left to right.
    LR,
    /// Right to left.
    RL,
}

/// A parsed flowchart.
#[derive(Debug, Clone, PartialEq)]
pub struct Graph {
    /// Direction.
    pub dir: Dir,
    /// Nodes.
    pub nodes: Vec<Node>,
    /// Edges.
    pub edges: Vec<Edge>,
    /// Subgraphs.
    pub clusters: Vec<Cluster>,
}

struct P<'a> {
    s: &'a [char],
    i: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn at(&self, pat: &str) -> bool {
        let p: Vec<char> = pat.chars().collect();
        self.s.len() >= self.i + p.len() && self.s[self.i..self.i + p.len()] == p[..]
    }
    fn eat(&mut self, pat: &str) -> bool {
        if self.at(pat) {
            self.i += pat.chars().count();
            true
        } else {
            false
        }
    }
    fn ws(&mut self) {
        while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
            self.i += 1;
        }
    }
    fn ident(&mut self) -> Option<String> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric()
                || c == '_'
                || c == '.'
                || (c == '-' && !self.at("--") && !self.at("-."))
            {
                self.i += 1;
            } else {
                break;
            }
        }
        (self.i > start).then(|| self.s[start..self.i].iter().collect())
    }
    /// Text up to (not including) `close`, handling `"quoted"` text.
    fn until(&mut self, close: &str) -> Option<String> {
        self.ws();
        if self.peek() == Some('"') {
            self.i += 1;
            let start = self.i;
            while self.peek().is_some_and(|c| c != '"') {
                self.i += 1;
            }
            let t: String = self.s[start..self.i].iter().collect();
            self.i += 1;
            self.ws();
            return self.eat(close).then_some(t);
        }
        let start = self.i;
        while self.i < self.s.len() && !self.at(close) {
            self.i += 1;
        }
        if self.i >= self.s.len() {
            return None;
        }
        let t: String = self.s[start..self.i].iter().collect();
        self.eat(close);
        Some(t.trim().to_owned())
    }
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    t.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(t)
        .to_owned()
}

/// Parses a node reference with an optional shape: `A`, `A[text]`, `A((x))`, …
fn node_ref(p: &mut P<'_>) -> Option<(String, Option<(String, Shape)>)> {
    p.ws();
    let id = p.ident()?;
    let shapes: &[(&str, &str, Shape)] = &[
        ("(((", ")))", Shape::DoubleCircle),
        ("((", "))", Shape::Circle),
        ("([", "])", Shape::Stadium),
        ("[[", "]]", Shape::Subroutine),
        ("[(", ")]", Shape::Cylinder),
        ("{{", "}}", Shape::Hexagon),
        ("[/", "/]", Shape::Parallelogram),
        ("[\\", "\\]", Shape::ParallelogramAlt),
        ("(", ")", Shape::Round),
        ("[", "]", Shape::Rect),
        ("{", "}", Shape::Diamond),
        (">", "]", Shape::Asymmetric),
    ];
    for (open, close, shape) in shapes {
        if p.at(open) {
            // `[/text\]` and `[\text/]` are trapezoids.
            let save = p.i;
            p.eat(open);
            if *shape == Shape::Parallelogram || *shape == Shape::ParallelogramAlt {
                let alt_close = if *shape == Shape::Parallelogram {
                    "\\]"
                } else {
                    "/]"
                };
                let start = p.i;
                let mut j = start;
                let mut found = None;
                while j < p.s.len() {
                    let rest: String = p.s[j..(j + 2).min(p.s.len())].iter().collect();
                    if rest == *close {
                        found = Some((j, *shape));
                        break;
                    }
                    if rest == alt_close {
                        let t = if *shape == Shape::Parallelogram {
                            Shape::Trapezoid
                        } else {
                            Shape::TrapezoidAlt
                        };
                        found = Some((j, t));
                        break;
                    }
                    j += 1;
                }
                if let Some((end, sh)) = found {
                    let t: String = p.s[start..end].iter().collect();
                    p.i = end + 2;
                    return Some((id, Some((unquote(&t), sh))));
                }
                p.i = save;
                continue;
            }
            match p.until(close) {
                Some(t) => return Some((id, Some((unquote(&t), *shape)))),
                None => {
                    p.i = save;
                    return None;
                }
            }
        }
    }
    Some((id, None))
}

/// Parses an edge operator with optional inline text: `-->`, `-- text -->`,
/// `-->|text|`, `-.->`, `==>`, `---`, `--o`, `--x`, `<-->`, `~~~`.
fn edge_op(p: &mut P<'_>) -> Option<(Line, Head, Head, usize, String)> {
    p.ws();
    let save = p.i;
    let tail = if p.eat("<") {
        Head::Arrow
    } else if p.at("o--") || p.at("o==") || p.at("o-.") {
        p.i += 1;
        Head::Circle
    } else if p.at("x--") || p.at("x==") || p.at("x-.") {
        p.i += 1;
        Head::Cross
    } else {
        Head::None
    };
    let mut line;
    let mut len = 0usize;
    let mut text = String::new();
    if p.eat("~~~") {
        while p.eat("~") {}
        return Some((Line::Invisible, Head::None, Head::None, 1, String::new()));
    }
    if p.at("--") || p.at("==") || p.at("-.") {
        line = if p.at("==") {
            Line::Thick
        } else if p.at("-.") {
            Line::Dotted
        } else {
            Line::Solid
        };
        // Consume the stroke.
        let ch = if line == Line::Thick { '=' } else { '-' };
        while p.peek() == Some(ch) || p.peek() == Some('.') {
            if p.peek() == Some('.') {
                line = Line::Dotted;
            }
            p.i += 1;
            len += 1;
        }
        // Inline text: `-- text -->`
        if p.peek() == Some(' ') {
            let start = p.i;
            let mut j = p.i;
            let stops: [&str; 3] = ["-->", "==>", ".->"];
            let mut found = None;
            while j < p.s.len() {
                let rest: String = p.s[j..(j + 3).min(p.s.len())].iter().collect();
                if stops.iter().any(|s| rest == *s)
                    || rest.starts_with("---")
                    || rest.starts_with("===")
                {
                    found = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(end) = found {
                text = p.s[start..end].iter().collect::<String>().trim().to_owned();
                p.i = end;
                // The closing run alone decides the length (`-- text -->`).
                len = 0;
                while p.peek().is_some_and(|c| c == '-' || c == '=' || c == '.') {
                    p.i += 1;
                    len += 1;
                }
            }
        }
        let head = if p.eat(">") {
            Head::Arrow
        } else if p.peek() == Some('o') && !p.s.get(p.i + 1).is_some_and(|c| c.is_alphanumeric()) {
            p.i += 1;
            Head::Circle
        } else if p.peek() == Some('x') && !p.s.get(p.i + 1).is_some_and(|c| c.is_alphanumeric()) {
            p.i += 1;
            Head::Cross
        } else {
            Head::None
        };
        p.ws();
        if p.eat("|") {
            let start = p.i;
            while p.peek().is_some_and(|c| c != '|') {
                p.i += 1;
            }
            text = unquote(&p.s[start..p.i].iter().collect::<String>());
            p.eat("|");
        }
        // `-->`/`---` have length 1, each extra stroke adds one rank.
        let min_len = len
            .saturating_sub(if head == Head::None { 2 } else { 1 })
            .clamp(1, 4);
        let tail = match tail {
            // `<-->`: the same head on both ends.
            Head::Arrow => head,
            t => t,
        };
        return Some((line, head, tail, min_len, text));
    }
    p.i = save;
    None
}

fn parse_color(spec: &str) -> Option<Rgba> {
    let h = spec.trim().strip_prefix('#')?;
    // `get`: a byte range inside a multi-byte character (`#fʿ`) is no color.
    let v = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
    match h.len() {
        3 => Some([v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17, 255]),
        6 => Some([v(0, 2)?, v(2, 2)?, v(4, 2)?, 255]),
        _ => None,
    }
}

/// Parses flowchart source (the first line is the header).
pub fn parse(src: &str) -> Result<Graph, Error> {
    let mut lines = src.lines().enumerate();
    let (_, header) = lines
        .find(|(_, l)| !l.trim().is_empty() && !l.trim().starts_with("%%"))
        .ok_or(Error::new(0, "empty diagram"))?;
    let mut words = header.split_whitespace();
    let _kw = words.next();
    let dir = match words.next().unwrap_or("TB").trim_end_matches(';') {
        "TD" | "TB" => Dir::TB,
        "BT" => Dir::BT,
        "LR" => Dir::LR,
        "RL" => Dir::RL,
        d => return Err(Error::new(1, &format!("unknown direction {d}"))),
    };
    let mut g = Graph {
        dir,
        nodes: Vec::new(),
        edges: Vec::new(),
        clusters: Vec::new(),
    };
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut open_clusters: Vec<usize> = Vec::new();
    fn get(
        g: &mut Graph,
        index: &mut HashMap<String, usize>,
        id: &str,
        shape: Option<(String, Shape)>,
        open: &[usize],
    ) -> usize {
        let i = match index.get(id) {
            Some(&i) => {
                // A node first used outside joins the subgraph that mentions it.
                if let Some(&c) = open.last()
                    && !g.clusters.iter().any(|cl| cl.nodes.contains(&i))
                {
                    g.clusters[c].nodes.push(i);
                }
                i
            }
            None => {
                let i = g.nodes.len();
                g.nodes.push(Node {
                    id: id.to_owned(),
                    label: id.to_owned(),
                    shape: Shape::Rect,
                    fill: None,
                    stroke: None,
                });
                index.insert(id.to_owned(), i);
                if let Some(&c) = open.last() {
                    g.clusters[c].nodes.push(i);
                }
                i
            }
        };
        if let Some((label, sh)) = shape {
            g.nodes[i].label = label;
            g.nodes[i].shape = sh;
        }
        i
    }
    for (ln, raw) in lines {
        let line_no = ln + 1;
        let line = raw
            .split("%%")
            .next()
            .unwrap_or("")
            .trim()
            .trim_end_matches(';');
        if line.is_empty() {
            continue;
        }
        let low = line.to_ascii_lowercase();
        if low.starts_with("subgraph") {
            let rest = line["subgraph".len()..].trim();
            let title = match (rest.find('['), rest.rfind(']')) {
                (Some(a), Some(b)) if b > a => unquote(&rest[a + 1..b]),
                _ => unquote(rest),
            };
            let id = rest
                .split(['[', ' '])
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            g.clusters.push(Cluster {
                id,
                title,
                nodes: Vec::new(),
            });
            open_clusters.push(g.clusters.len() - 1);
            continue;
        }
        if low == "end" {
            if let Some(c) = open_clusters.pop()
                && let Some(&parent) = open_clusters.last()
            {
                let members = g.clusters[c].nodes.clone();
                g.clusters[parent].nodes.extend(members);
            }
            continue;
        }
        if low.starts_with("direction ") {
            continue;
        }
        if low.starts_with("style ") {
            let mut it = line[6..].trim().splitn(2, ' ');
            let id = it.next().unwrap_or("");
            let props = it.next().unwrap_or("");
            if let Some(&i) = index.get(id) {
                for prop in props.split(',') {
                    let (k, v) = prop.split_once(':').unwrap_or(("", ""));
                    match k.trim() {
                        "fill" => g.nodes[i].fill = parse_color(v),
                        "stroke" => g.nodes[i].stroke = parse_color(v),
                        _ => {}
                    }
                }
            }
            continue;
        }
        if low.starts_with("classdef ")
            || low.starts_with("class ")
            || low.starts_with("click ")
            || low.starts_with("linkstyle ")
        {
            continue;
        }
        // Statements: node groups joined by edges.
        let chars: Vec<char> = line.chars().collect();
        let mut p = P { s: &chars, i: 0 };
        let mut prev: Vec<usize> = Vec::new();
        let mut pending: Option<(Line, Head, Head, usize, String)> = None;
        loop {
            // A group: `A & B & C`
            let mut group = Vec::new();
            loop {
                let Some((id, shape)) = node_ref(&mut p) else {
                    return Err(Error::new(line_no, &format!("cannot parse `{}`", line)));
                };
                group.push(get(&mut g, &mut index, &id, shape, &open_clusters));
                p.ws();
                // `A:::class`
                if p.eat(":::") {
                    let _ = p.ident();
                }
                if !p.eat("&") {
                    break;
                }
            }
            if let Some((ln_style, head, tail, min_len, label)) = pending.take() {
                for &a in &prev {
                    for &b in &group {
                        g.edges.push(Edge {
                            from: a,
                            to: b,
                            label: label.clone(),
                            line: ln_style,
                            head,
                            tail,
                            min_len,
                        });
                    }
                }
            }
            prev = group;
            p.ws();
            if p.peek().is_none() {
                break;
            }
            match edge_op(&mut p) {
                Some(e) => pending = Some(e),
                None => {
                    return Err(Error::new(
                        line_no,
                        &format!("unexpected `{}`", chars[p.i..].iter().collect::<String>()),
                    ));
                }
            }
        }
    }
    resolve_cluster_edges(&mut g);
    Ok(g)
}

/// Edges to or from a subgraph id (`one --> two`) connect to its members: the
/// placeholder node created for the id is removed.
fn resolve_cluster_edges(g: &mut Graph) {
    let mut removed = Vec::new();
    for c in &g.clusters {
        let Some(v) = g.nodes.iter().position(|n| n.id == c.id && n.label == n.id) else {
            continue;
        };
        let members: Vec<usize> = c.nodes.iter().copied().filter(|&m| m != v).collect();
        let (Some(&first), Some(&last)) = (members.first(), members.last()) else {
            continue;
        };
        for e in &mut g.edges {
            if e.from == v {
                e.from = last;
            }
            if e.to == v {
                e.to = first;
            }
        }
        removed.push(v);
    }
    if removed.is_empty() {
        return;
    }
    // Two subgraphs with the same id share one placeholder.
    removed.sort_unstable();
    removed.dedup();
    let map: Vec<Option<usize>> = (0..g.nodes.len())
        .map(|i| {
            if removed.binary_search(&i).is_ok() {
                None
            } else {
                Some(i - removed.iter().filter(|&&r| r < i).count())
            }
        })
        .collect();
    let mut k = 0;
    g.nodes.retain(|_| {
        k += 1;
        map[k - 1].is_some()
    });
    g.edges
        .retain(|e| map[e.from].is_some() && map[e.to].is_some());
    for e in &mut g.edges {
        e.from = map[e.from].unwrap_or(0);
        e.to = map[e.to].unwrap_or(0);
    }
    for c in &mut g.clusters {
        c.nodes = c.nodes.iter().filter_map(|&n| map[n]).collect();
    }
}

// ---- layout ----

#[derive(Debug, Clone, Copy)]
struct Placed {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

const FONT: f32 = 14.0;

fn node_size(n: &Node) -> (Vec<String>, f32, f32) {
    let max = if matches!(
        n.shape,
        Shape::Circle | Shape::DoubleCircle | Shape::Diamond
    ) {
        120.0
    } else {
        200.0
    };
    let lines = draw::wrap(&n.label, FONT, max);
    let (tw, th) = draw::lines_size(&lines, FONT);
    let (w, h) = match n.shape {
        Shape::Circle | Shape::DoubleCircle => {
            let d = tw.max(th) + 24.0;
            (d, d)
        }
        Shape::Diamond => {
            let s = (tw + th) * 1.0 + 24.0;
            (s.max(60.0), s.max(60.0) * 0.75 + 8.0)
        }
        Shape::Hexagon => (tw + 48.0, th + 20.0),
        Shape::Parallelogram | Shape::ParallelogramAlt | Shape::Trapezoid | Shape::TrapezoidAlt => {
            (tw + 50.0, th + 20.0)
        }
        Shape::Cylinder => (tw + 30.0, th + 34.0),
        _ => (tw + 30.0, th + 20.0),
    };
    (lines, w.max(40.0), h.max(34.0))
}

/// Lays out and draws a flowchart.
pub fn render(g: &Graph, pal: &Palette) -> Scene {
    let n = g.nodes.len();
    let horizontal = matches!(g.dir, Dir::LR | Dir::RL);
    let sizes: Vec<(Vec<String>, f32, f32)> = g.nodes.iter().map(node_size).collect();
    // 1. Ranks: longest path over a DAG (back edges found by DFS are ignored).
    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, e) in g.edges.iter().enumerate() {
        out_edges[e.from].push(k);
    }
    let mut state = vec![0u8; n];
    let mut back = vec![false; g.edges.len()];
    fn dfs(v: usize, out: &[Vec<usize>], edges: &[Edge], state: &mut [u8], back: &mut [bool]) {
        state[v] = 1;
        for &k in &out[v] {
            let t = edges[k].to;
            if state[t] == 1 {
                back[k] = true;
            } else if state[t] == 0 {
                dfs(t, out, edges, state, back);
            }
        }
        state[v] = 2;
    }
    for v in 0..n {
        if state[v] == 0 {
            dfs(v, &out_edges, &g.edges, &mut state, &mut back);
        }
    }
    let mut rank = vec![0usize; n];
    // Relax repeatedly (n is small).
    for _ in 0..n.max(1) {
        let mut changed = false;
        for (k, e) in g.edges.iter().enumerate() {
            if back[k] || e.from == e.to {
                continue;
            }
            let want = rank[e.from] + e.min_len;
            if rank[e.to] < want {
                rank[e.to] = want;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let max_rank = rank.iter().copied().max().unwrap_or(0);
    // 2. Layers with dummy nodes for long edges.
    #[derive(Clone)]
    struct Slot {
        w: f32,
        h: f32,
    }
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    let mut slots: Vec<Slot> = Vec::new();
    for v in 0..n {
        let (_, w, h) = &sizes[v];
        let (w, h) = if horizontal { (*h, *w) } else { (*w, *h) };
        slots.push(Slot { w, h });
        layers[rank[v]].push(v);
    }
    // Edge chains through dummies: (edge, slot path).
    let mut chains: Vec<(usize, Vec<usize>)> = Vec::new();
    for (k, e) in g.edges.iter().enumerate() {
        let (a, b) = if back[k] {
            (e.to, e.from)
        } else {
            (e.from, e.to)
        };
        let mut path = vec![a];
        if rank[b] > rank[a] + 1 {
            for layer in &mut layers[rank[a] + 1..rank[b]] {
                let s = slots.len();
                let lw = if e.label.is_empty() { 10.0 } else { 40.0 };
                slots.push(Slot { w: lw, h: 10.0 });
                layer.push(s);
                path.push(s);
            }
        }
        path.push(b);
        chains.push((k, path));
    }
    // Adjacency between consecutive slots.
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); slots.len()];
    let mut down: Vec<Vec<usize>> = vec![Vec::new(); slots.len()];
    for (_, path) in &chains {
        for w in path.windows(2) {
            if w[0] != w[1] {
                down[w[0]].push(w[1]);
                up[w[1]].push(w[0]);
            }
        }
    }
    // Cluster grouping key keeps subgraph members together.
    let mut cluster_of = vec![usize::MAX; slots.len()];
    for (ci, c) in g.clusters.iter().enumerate() {
        for &v in &c.nodes {
            if cluster_of[v] == usize::MAX {
                cluster_of[v] = ci;
            }
        }
    }
    // 3. Ordering: barycenter sweeps.
    let mut pos_in: Vec<f32> = vec![0.0; slots.len()];
    for l in &layers {
        for (i, &s) in l.iter().enumerate() {
            pos_in[s] = i as f32;
        }
    }
    for sweep in 0..8 {
        let downward = sweep % 2 == 0;
        let order: Vec<usize> = if downward {
            (1..layers.len()).collect()
        } else {
            (0..layers.len().saturating_sub(1)).rev().collect()
        };
        for r in order {
            let nb = |s: usize| if downward { &up[s] } else { &down[s] };
            let mut keyed: Vec<(f32, usize, usize)> = layers[r]
                .iter()
                .map(|&s| {
                    let ns = nb(s);
                    let bc = if ns.is_empty() {
                        pos_in[s]
                    } else {
                        ns.iter().map(|&t| pos_in[t]).sum::<f32>() / ns.len() as f32
                    };
                    (bc, cluster_of[s], s)
                })
                .collect();
            // Members of a cluster share the cluster's mean barycenter.
            let mut cmean: HashMap<usize, (f32, usize)> = HashMap::new();
            for &(bc, c, _) in &keyed {
                if c != usize::MAX {
                    let e = cmean.entry(c).or_insert((0.0, 0));
                    e.0 += bc;
                    e.1 += 1;
                }
            }
            keyed.sort_by(|a, b| {
                let ka = if a.1 == usize::MAX {
                    a.0
                } else {
                    cmean[&a.1].0 / cmean[&a.1].1 as f32
                };
                let kb = if b.1 == usize::MAX {
                    b.0
                } else {
                    cmean[&b.1].0 / cmean[&b.1].1 as f32
                };
                ka.total_cmp(&kb)
                    .then(a.1.cmp(&b.1))
                    .then(a.0.total_cmp(&b.0))
            });
            layers[r] = keyed.into_iter().map(|k| k.2).collect();
            for (i, &s) in layers[r].iter().enumerate() {
                pos_in[s] = i as f32;
            }
        }
    }
    // 4. Coordinates (in "layout space": along = rank axis, across = order axis).
    let gap_across = 36.0;
    let gap_along = 56.0;
    let mut across = vec![0.0f32; slots.len()];
    for l in &layers {
        let mut x = 0.0;
        for &s in l {
            across[s] = x + slots[s].w / 2.0;
            x += slots[s].w + gap_across;
        }
    }
    // Neighbour averaging with overlap resolution.
    for it in 0..12 {
        let downward = it % 2 == 0;
        let rs: Vec<usize> = if downward {
            (0..layers.len()).collect()
        } else {
            (0..layers.len()).rev().collect()
        };
        for r in rs {
            let l = layers[r].clone();
            let mut want: Vec<f32> = l
                .iter()
                .map(|&s| {
                    let ns: Vec<usize> = up[s].iter().chain(down[s].iter()).copied().collect();
                    if ns.is_empty() {
                        across[s]
                    } else {
                        ns.iter().map(|&t| across[t]).sum::<f32>() / ns.len() as f32
                    }
                })
                .collect();
            // Keep order and spacing: left-to-right then right-to-left passes.
            for i in 1..l.len() {
                let min = want[i - 1] + (slots[l[i - 1]].w + slots[l[i]].w) / 2.0 + gap_across;
                if want[i] < min {
                    want[i] = min;
                }
            }
            for (i, &s) in l.iter().enumerate() {
                across[s] = want[i];
            }
        }
    }
    // Subgraphs occupy disjoint bands: overlapping clusters (members on
    // different ranks) push later ones, and everything right of them, aside.
    if g.clusters.len() > 1 {
        let band = |c: &Cluster, across: &[f32]| -> Option<(f32, f32)> {
            c.nodes
                .iter()
                .map(|&v| (across[v] - slots[v].w / 2.0, across[v] + slots[v].w / 2.0))
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        };
        let mut order: Vec<usize> = (0..g.clusters.len()).collect();
        order.sort_by(|&a, &b| {
            let ba = band(&g.clusters[a], &across).map_or(0.0, |x| x.0);
            let bb = band(&g.clusters[b], &across).map_or(0.0, |x| x.0);
            ba.total_cmp(&bb)
        });
        // Nested subgraphs (and repeated ids) share members: only disjoint ones
        // are moved apart, else every push widens the parent and the next push
        // grows with it (a fuzzed input reached a million pixels).
        let disjoint = |a: &Cluster, b: &Cluster| !a.nodes.iter().any(|v| b.nodes.contains(v));
        // All slots side by side: no separation needs more room than that.
        let limit = slots.iter().map(|s| s.w + gap_across).sum::<f32>()
            + 2.0 * 26.0 * g.clusters.len() as f32;
        let mut done: Vec<usize> = Vec::new();
        for &ci in &order {
            let Some((lo, _)) = band(&g.clusters[ci], &across) else {
                continue;
            };
            let others: Vec<usize> = done
                .iter()
                .copied()
                .filter(|&d| disjoint(&g.clusters[d], &g.clusters[ci]))
                .collect();
            let prev_hi = others
                .iter()
                .filter_map(|&d| band(&g.clusters[d], &across))
                .map(|b| b.1)
                .fold(f32::NEG_INFINITY, f32::max);
            let need = prev_hi + 2.0 * 26.0 - lo;
            let widest = across.iter().copied().fold(0.0, f32::max);
            if prev_hi.is_finite() && need > 0.0 && widest + need < limit {
                let protected: std::collections::HashSet<usize> = others
                    .iter()
                    .flat_map(|&d| g.clusters[d].nodes.iter().copied())
                    .collect();
                for (s, a) in across.iter_mut().enumerate() {
                    if !protected.contains(&s) && *a >= lo - 0.5 {
                        *a += need;
                    }
                }
                for &m in &g.clusters[ci].nodes {
                    if across[m] < lo + need - 0.5 {
                        across[m] += need;
                    }
                }
            }
            done.push(ci);
        }
    }
    let min_across = slots
        .iter()
        .enumerate()
        .map(|(i, s)| across[i] - s.w / 2.0)
        .fold(f32::INFINITY, f32::min);
    for a in &mut across {
        *a -= min_across;
    }
    let mut along_of_rank = vec![0.0f32; layers.len()];
    let mut acc = 0.0;
    for (r, l) in layers.iter().enumerate() {
        let thick = l.iter().map(|&s| slots[s].h).fold(0.0, f32::max);
        along_of_rank[r] = acc + thick / 2.0;
        acc += thick + gap_along;
    }
    let total_along = (acc - gap_along).max(0.0);
    let total_across = slots
        .iter()
        .enumerate()
        .map(|(i, s)| across[i] + s.w / 2.0)
        .fold(0.0, f32::max);
    let slot_rank: Vec<usize> = {
        let mut v = vec![0; slots.len()];
        for (r, l) in layers.iter().enumerate() {
            for &s in l {
                v[s] = r;
            }
        }
        v
    };
    let margin = 16.0;
    let cluster_pad = if g.clusters.is_empty() { 0.0 } else { 26.0 };
    let to_xy = |s: usize| -> (f32, f32) {
        let a = across[s];
        let mut l = along_of_rank[slot_rank[s]];
        if matches!(g.dir, Dir::BT | Dir::RL) {
            l = total_along - l;
        }
        if horizontal {
            (l + margin + cluster_pad, a + margin + cluster_pad)
        } else {
            (a + margin + cluster_pad, l + margin + cluster_pad)
        }
    };
    let placed: Vec<Placed> = (0..n)
        .map(|v| {
            let (x, y) = to_xy(v);
            let (_, w, h) = sizes[v];
            Placed { x, y, w, h }
        })
        .collect();
    let (width, height) = if horizontal {
        (
            total_along + 2.0 * (margin + cluster_pad),
            total_across + 2.0 * (margin + cluster_pad),
        )
    } else {
        (
            total_across + 2.0 * (margin + cluster_pad),
            total_along + 2.0 * (margin + cluster_pad),
        )
    };
    let mut sc = Scene {
        width,
        height,
        fills: Vec::new(),
    };
    // Clusters behind everything.
    for c in &g.clusters {
        let mut r: Option<(f32, f32, f32, f32)> = None;
        for &v in &c.nodes {
            let p = placed[v];
            let b = (
                p.x - p.w / 2.0,
                p.y - p.h / 2.0,
                p.x + p.w / 2.0,
                p.y + p.h / 2.0,
            );
            r = Some(match r {
                None => b,
                Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
            });
        }
        if let Some((x0, y0, x1, y1)) = r {
            let (x0, y0, x1, y1) = (x0 - 14.0, y0 - 24.0, x1 + 14.0, y1 + 12.0);
            sc.fill(
                draw::rounded_rect(x0, y0, x1 - x0, y1 - y0, 4.0),
                pal.cluster_fill,
            );
            sc.stroke(
                draw::rounded_rect(x0, y0, x1 - x0, y1 - y0, 4.0),
                pal.cluster_stroke,
                1.0,
                None,
            );
            let tw = draw::text_width(&c.title, FONT * 0.9);
            draw::draw_text(
                &mut sc,
                &c.title,
                (x0 + x1) / 2.0 - tw / 2.0,
                y0 + 15.0,
                FONT * 0.9,
                pal.fg,
            );
        }
    }
    // Edges.
    let mut labels = Vec::new();
    for (k, path) in &chains {
        let e = &g.edges[*k];
        if e.line == Line::Invisible {
            continue;
        }
        let mut pts: Vec<(f32, f32)> = path.iter().map(|&s| to_xy(s)).collect();
        if back[*k] {
            pts.reverse();
        }
        let (from, to) = (e.from, e.to);
        if from == to {
            // Self loop.
            let p = placed[from];
            let (x, y) = (p.x + p.w / 2.0, p.y);
            pts = vec![
                (x, y - 8.0),
                (x + 26.0, y - 18.0),
                (x + 26.0, y + 18.0),
                (x, y + 8.0),
            ];
        } else {
            let first = pts[1];
            let last = pts[pts.len() - 2];
            pts[0] = border(&g.nodes[from], placed[from], first);
            let end = pts.len() - 1;
            pts[end] = border(&g.nodes[to], placed[to], last);
        }
        let (width, dash) = match e.line {
            Line::Thick => (2.6, None),
            Line::Dotted => (1.4, Some((3.0, 3.0))),
            _ => (1.4, None),
        };
        // Leave room for the arrowhead.
        let mut line_pts = pts.clone();
        if e.head != Head::None {
            let l = line_pts.len();
            line_pts[l - 1] = shorten(line_pts[l - 2], line_pts[l - 1], 6.0);
        }
        if e.tail != Head::None {
            line_pts[0] = shorten(line_pts[1], line_pts[0], 6.0);
        }
        sc.stroke(draw::smooth(&line_pts), pal.edge, width, dash);
        let l = pts.len();
        head(&mut sc, e.head, pts[l - 2], pts[l - 1], pal);
        head(&mut sc, e.tail, pts[1], pts[0], pal);
        if !e.label.is_empty() {
            let mid = if pts.len() > 2 {
                pts[pts.len() / 2]
            } else {
                ((pts[0].0 + pts[1].0) / 2.0, (pts[0].1 + pts[1].1) / 2.0)
            };
            labels.push((e.label.clone(), mid));
        }
    }
    // Nodes.
    for (v, nd) in g.nodes.iter().enumerate() {
        let p = placed[v];
        let fill = nd.fill.unwrap_or(pal.node_fill);
        let stroke = nd.stroke.unwrap_or(pal.node_stroke);
        shape(&mut sc, nd.shape, p, fill, stroke);
        draw::draw_lines_centered(&mut sc, &sizes[v].0, p.x, p.y, FONT, pal.fg);
    }
    // Edge labels on top.
    for (t, (x, y)) in labels {
        let lines = draw::wrap(&t, FONT * 0.9, 160.0);
        let (w, h) = draw::lines_size(&lines, FONT * 0.9);
        sc.fill(
            draw::rounded_rect(x - w / 2.0 - 4.0, y - h / 2.0 - 2.0, w + 8.0, h + 4.0, 3.0),
            pal.label_bg,
        );
        draw::draw_lines_centered(&mut sc, &lines, x, y, FONT * 0.9, pal.fg);
    }
    sc
}

fn shorten(a: (f32, f32), b: (f32, f32), by: f32) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < by * 1.5 {
        return b;
    }
    (b.0 - dx / len * by, b.1 - dy / len * by)
}

fn head(sc: &mut Scene, h: Head, from: (f32, f32), tip: (f32, f32), pal: &Palette) {
    match h {
        Head::None => {}
        Head::Arrow => sc.fill(draw::arrowhead(from, tip, 9.0), pal.edge),
        Head::Circle => {
            let c = shorten(from, tip, 4.0);
            sc.fill(draw::ellipse(c.0, c.1, 4.0, 4.0), pal.edge);
        }
        Head::Cross => {
            let c = shorten(from, tip, 4.0);
            let d = 4.0;
            sc.stroke(
                draw::polyline(&[(c.0 - d, c.1 - d), (c.0 + d, c.1 + d)]),
                pal.edge,
                1.6,
                None,
            );
            sc.stroke(
                draw::polyline(&[(c.0 - d, c.1 + d), (c.0 + d, c.1 - d)]),
                pal.edge,
                1.6,
                None,
            );
        }
    }
}

/// Where the segment from the node center towards `toward` leaves the node.
fn border(n: &Node, p: Placed, toward: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (toward.0 - p.x, toward.1 - p.y);
    if dx.abs() < 1e-3 && dy.abs() < 1e-3 {
        return (p.x, p.y);
    }
    let (hw, hh) = (p.w / 2.0, p.h / 2.0);
    let t = match n.shape {
        Shape::Circle | Shape::DoubleCircle => {
            let r = hw;
            r / (dx * dx + dy * dy).sqrt()
        }
        Shape::Diamond => 1.0 / (dx.abs() / hw + dy.abs() / hh),
        _ => {
            let tx = if dx.abs() > 1e-3 {
                hw / dx.abs()
            } else {
                f32::INFINITY
            };
            let ty = if dy.abs() > 1e-3 {
                hh / dy.abs()
            } else {
                f32::INFINITY
            };
            tx.min(ty)
        }
    };
    (p.x + dx * t, p.y + dy * t)
}

fn shape(sc: &mut Scene, s: Shape, p: Placed, fill: Rgba, stroke: Rgba) {
    let (x, y, w, h) = (p.x - p.w / 2.0, p.y - p.h / 2.0, p.w, p.h);
    let path = match s {
        Shape::Rect | Shape::Subroutine => draw::rounded_rect(x, y, w, h, 2.0),
        Shape::Round => draw::rounded_rect(x, y, w, h, 8.0),
        Shape::Stadium => draw::rounded_rect(x, y, w, h, h / 2.0),
        Shape::Circle | Shape::DoubleCircle => draw::ellipse(p.x, p.y, w / 2.0, h / 2.0),
        Shape::Diamond => draw::polygon(&[(p.x, y), (x + w, p.y), (p.x, y + h), (x, p.y)]),
        Shape::Hexagon => {
            let k = h / 2.0;
            draw::polygon(&[
                (x + k, y),
                (x + w - k, y),
                (x + w, p.y),
                (x + w - k, y + h),
                (x + k, y + h),
                (x, p.y),
            ])
        }
        Shape::Parallelogram => {
            let k = 14.0;
            draw::polygon(&[(x + k, y), (x + w, y), (x + w - k, y + h), (x, y + h)])
        }
        Shape::ParallelogramAlt => {
            let k = 14.0;
            draw::polygon(&[(x, y), (x + w - k, y), (x + w, y + h), (x + k, y + h)])
        }
        Shape::Trapezoid => {
            let k = 14.0;
            draw::polygon(&[(x + k, y), (x + w - k, y), (x + w, y + h), (x, y + h)])
        }
        Shape::TrapezoidAlt => {
            let k = 14.0;
            draw::polygon(&[(x, y), (x + w, y), (x + w - k, y + h), (x + k, y + h)])
        }
        Shape::Asymmetric => {
            let k = 12.0;
            draw::polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x + k, p.y)])
        }
        Shape::Cylinder => {
            let e = 7.0;
            let mut b = tiny_skia_path::PathBuilder::new();
            b.move_to(x, y + e);
            b.cubic_to(x, y - e / 3.0, x + w, y - e / 3.0, x + w, y + e);
            b.line_to(x + w, y + h - e);
            b.cubic_to(x + w, y + h + e / 3.0, x, y + h + e / 3.0, x, y + h - e);
            b.close();
            b.finish()
        }
    };
    sc.fill(path.clone(), fill);
    sc.stroke(path, stroke, 1.3, None);
    match s {
        Shape::Subroutine => {
            sc.stroke(
                draw::polyline(&[(x + 7.0, y), (x + 7.0, y + h)]),
                stroke,
                1.3,
                None,
            );
            sc.stroke(
                draw::polyline(&[(x + w - 7.0, y), (x + w - 7.0, y + h)]),
                stroke,
                1.3,
                None,
            );
        }
        Shape::DoubleCircle => sc.stroke(
            draw::ellipse(p.x, p.y, w / 2.0 - 4.0, h / 2.0 - 4.0),
            stroke,
            1.3,
            None,
        ),
        Shape::Cylinder => {
            let e = 7.0;
            let mut b = tiny_skia_path::PathBuilder::new();
            b.move_to(x, y + e);
            b.cubic_to(x, y + e + e * 1.3, x + w, y + e + e * 1.3, x + w, y + e);
            sc.stroke(b.finish(), stroke, 1.3, None);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nodes_edges_and_subgraphs() {
        let g = parse(
            "flowchart LR\n  A[Start] --> B{Is it?}\n  B -->|Yes| C[OK]\n  C --> D[Rethink]\n  D --> B\n  B -- No ----> E[End]\n  subgraph one [Group]\n    F((x)) -.-> G[(db)]\n  end\n  A & F --> H>flag]",
        )
        .unwrap();
        assert_eq!(g.dir, Dir::LR);
        assert_eq!(
            g.nodes.iter().find(|n| n.id == "B").unwrap().shape,
            Shape::Diamond
        );
        assert!(g.edges.iter().any(|e| e.label == "Yes"));
        assert!(g.edges.iter().any(|e| e.label == "No" && e.min_len >= 2));
        assert!(g.edges.iter().any(|e| e.line == Line::Dotted));
        assert_eq!(g.clusters.len(), 1);
        assert_eq!(g.clusters[0].nodes.len(), 2);
        assert_eq!(
            g.edges.iter().filter(|e| g.nodes[e.to].id == "H").count(),
            2
        );
        assert!(parse("flowchart XY\nA-->B").is_err());
        assert!(parse("graph TD\nA -->").is_err());
    }

    #[test]
    fn subgraphs_sharing_an_id_share_the_placeholder() {
        // Found by the `diagram` fuzz target: the placeholder was removed twice.
        let g = parse(
            "flowchart TB\n  one --> c\n  subgraph one\n    a\n  end\n  subgraph one\n    b\n  end\n",
        )
        .unwrap();
        assert!(g.nodes.iter().all(|n| n.id != "one"));
        for e in &g.edges {
            assert!(e.from < g.nodes.len() && e.to < g.nodes.len());
        }
    }

    #[test]
    fn colors_with_non_ascii_bytes_are_ignored() {
        // Found by the `diagram` fuzz target: a byte slice inside `ʿ`.
        assert_eq!(parse_color("#fʿ"), None);
        assert_eq!(parse_color("#ʿʿʿ"), None);
        assert_eq!(parse_color("#f9f"), Some([255, 153, 255, 255]));
        assert!(parse("flowchart LR\n  a-->b\n  style a fill:#fʿ,stroke:#333\n").is_ok());
    }
}
