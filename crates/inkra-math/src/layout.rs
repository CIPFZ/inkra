//! Box layout of a math list, following the OpenType MATH specification and
//! TeX's Appendix G in simplified form.

use ttf_parser::GlyphId;

use crate::font;
use crate::parse::{ArrayKind, ColAlign, MathStyle, Node};
use crate::symbols::{self, Class, Variant};

/// A positioned element inside a box. `y` is the baseline shift (positive up).
#[derive(Debug, Clone, PartialEq)]
pub enum Elem {
    /// A glyph at font size `size` (px).
    Glyph {
        /// Face index.
        face: u8,
        /// Glyph.
        gid: u16,
        /// Font size in px.
        size: f32,
    },
    /// A filled rectangle from the baseline up.
    Rule {
        /// Width.
        w: f32,
        /// Height.
        h: f32,
    },
    /// A nested box.
    Box(Box<MBox>),
    /// A nested box in a color.
    Colored([u8; 3], Box<MBox>),
    /// A frame around `w × (h + d)` with stroke `t` (`\boxed`).
    Frame {
        /// Width.
        w: f32,
        /// Height above the baseline.
        h: f32,
        /// Depth.
        d: f32,
        /// Stroke.
        t: f32,
    },
}

/// A laid-out box: width, height above and depth below the baseline.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MBox {
    /// Width.
    pub w: f32,
    /// Height (ascent).
    pub h: f32,
    /// Depth (descent).
    pub d: f32,
    /// Italic correction (for scripts after a slanted glyph).
    pub italic: f32,
    /// Top accent attachment, if this is a single glyph.
    pub accent_x: Option<f32>,
    /// Elements `(x, y, elem)`.
    pub items: Vec<(f32, f32, Elem)>,
}

impl MBox {
    fn push(&mut self, x: f32, y: f32, b: MBox) {
        self.items.push((x, y, Elem::Box(Box::new(b))));
    }

    /// Horizontal concatenation.
    fn hpack(boxes: Vec<MBox>) -> MBox {
        let mut out = MBox::default();
        let mut x = 0.0;
        for b in boxes {
            out.h = out.h.max(b.h);
            out.d = out.d.max(b.d);
            let w = b.w;
            out.italic = b.italic;
            out.push(x, 0.0, b);
            x += w;
        }
        out.w = x;
        out
    }

    fn kern(w: f32) -> MBox {
        MBox {
            w,
            ..Default::default()
        }
    }

    /// The same box shifted up by `dy`.
    fn raised(self, dy: f32) -> MBox {
        let (w, h, d, italic) = (self.w, self.h + dy, self.d - dy, self.italic);
        let mut out = MBox {
            w,
            h,
            d,
            italic,
            ..Default::default()
        };
        out.push(0.0, dy, self);
        out
    }
}

/// MATH constants in font units.
#[derive(Debug, Clone)]
struct Consts {
    upem: f32,
    script_pct: f32,
    script_script_pct: f32,
    axis: f32,
    accent_base_height: f32,
    sub_shift_down: f32,
    sub_top_max: f32,
    sub_baseline_drop_min: f32,
    sup_shift_up: f32,
    sup_shift_up_cramped: f32,
    sup_bottom_min: f32,
    sup_baseline_drop_max: f32,
    sub_sup_gap_min: f32,
    sup_bottom_max_with_sub: f32,
    space_after_script: f32,
    upper_limit_gap_min: f32,
    upper_limit_rise_min: f32,
    lower_limit_gap_min: f32,
    lower_limit_drop_min: f32,
    stack_top_shift_up: f32,
    stack_top_display_shift_up: f32,
    stack_bottom_shift_down: f32,
    stack_bottom_display_shift_down: f32,
    stack_gap_min: f32,
    stack_display_gap_min: f32,
    frac_num_shift_up: f32,
    frac_num_display_shift_up: f32,
    frac_den_shift_down: f32,
    frac_den_display_shift_down: f32,
    frac_num_gap_min: f32,
    frac_num_display_gap_min: f32,
    frac_rule: f32,
    frac_den_gap_min: f32,
    frac_den_display_gap_min: f32,
    overbar_gap: f32,
    overbar_rule: f32,
    overbar_extra: f32,
    underbar_gap: f32,
    underbar_rule: f32,
    underbar_extra: f32,
    radical_gap: f32,
    radical_display_gap: f32,
    radical_rule: f32,
    radical_extra: f32,
    radical_kern_before: f32,
    radical_kern_after: f32,
    radical_degree_raise: f32,
    delimited_min: f32,
    display_op_min: f32,
    min_overlap: f32,
}

fn consts() -> Consts {
    let face = font::math_face();
    let upem = face.map_or(1000.0, |f| f32::from(f.units_per_em()));
    let k = face.and_then(|f| f.tables().math).and_then(|m| m.constants);
    let min_overlap = face
        .and_then(|f| f.tables().math)
        .and_then(|m| m.variants)
        .map_or(0.0, |v| f32::from(v.min_connector_overlap));
    macro_rules! v {
        ($m:ident, $d:expr) => {
            k.map_or($d * upem, |c| f32::from(c.$m().value))
        };
    }
    Consts {
        upem,
        script_pct: k.map_or(70.0, |c| f32::from(c.script_percent_scale_down())),
        script_script_pct: k.map_or(50.0, |c| f32::from(c.script_script_percent_scale_down())),
        axis: v!(axis_height, 0.25),
        accent_base_height: v!(accent_base_height, 0.45),
        sub_shift_down: v!(subscript_shift_down, 0.2),
        sub_top_max: v!(subscript_top_max, 0.35),
        sub_baseline_drop_min: v!(subscript_baseline_drop_min, 0.2),
        sup_shift_up: v!(superscript_shift_up, 0.36),
        sup_shift_up_cramped: v!(superscript_shift_up_cramped, 0.29),
        sup_bottom_min: v!(superscript_bottom_min, 0.11),
        sup_baseline_drop_max: v!(superscript_baseline_drop_max, 0.38),
        sub_sup_gap_min: v!(sub_superscript_gap_min, 0.16),
        sup_bottom_max_with_sub: v!(superscript_bottom_max_with_subscript, 0.36),
        space_after_script: v!(space_after_script, 0.05),
        upper_limit_gap_min: v!(upper_limit_gap_min, 0.13),
        upper_limit_rise_min: v!(upper_limit_baseline_rise_min, 0.3),
        lower_limit_gap_min: v!(lower_limit_gap_min, 0.13),
        lower_limit_drop_min: v!(lower_limit_baseline_drop_min, 0.67),
        stack_top_shift_up: v!(stack_top_shift_up, 0.44),
        stack_top_display_shift_up: v!(stack_top_display_style_shift_up, 0.68),
        stack_bottom_shift_down: v!(stack_bottom_shift_down, 0.35),
        stack_bottom_display_shift_down: v!(stack_bottom_display_style_shift_down, 0.69),
        stack_gap_min: v!(stack_gap_min, 0.2),
        stack_display_gap_min: v!(stack_display_style_gap_min, 0.47),
        frac_num_shift_up: v!(fraction_numerator_shift_up, 0.39),
        frac_num_display_shift_up: v!(fraction_numerator_display_style_shift_up, 0.68),
        frac_den_shift_down: v!(fraction_denominator_shift_down, 0.35),
        frac_den_display_shift_down: v!(fraction_denominator_display_style_shift_down, 0.69),
        frac_num_gap_min: v!(fraction_numerator_gap_min, 0.04),
        frac_num_display_gap_min: v!(fraction_num_display_style_gap_min, 0.12),
        frac_rule: v!(fraction_rule_thickness, 0.04),
        frac_den_gap_min: v!(fraction_denominator_gap_min, 0.04),
        frac_den_display_gap_min: v!(fraction_denom_display_style_gap_min, 0.12),
        overbar_gap: v!(overbar_vertical_gap, 0.12),
        overbar_rule: v!(overbar_rule_thickness, 0.04),
        overbar_extra: v!(overbar_extra_ascender, 0.04),
        underbar_gap: v!(underbar_vertical_gap, 0.12),
        underbar_rule: v!(underbar_rule_thickness, 0.04),
        underbar_extra: v!(underbar_extra_descender, 0.04),
        radical_gap: v!(radical_vertical_gap, 0.05),
        radical_display_gap: v!(radical_display_style_vertical_gap, 0.15),
        radical_rule: v!(radical_rule_thickness, 0.04),
        radical_extra: v!(radical_extra_ascender, 0.04),
        radical_kern_before: v!(radical_kern_before_degree, 0.28),
        radical_kern_after: v!(radical_kern_after_degree, -0.55),
        radical_degree_raise: k
            .map_or(60.0, |c| f32::from(c.radical_degree_bottom_raise_percent())),
        delimited_min: k.map_or(1.3 * upem, |c| {
            f32::from(c.delimited_sub_formula_min_height())
        }),
        display_op_min: k.map_or(1.3 * upem, |c| f32::from(c.display_operator_min_height())),
        min_overlap,
    }
}

/// Layout state.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    style: MathStyle,
    cramped: bool,
    /// Text-style font size in px.
    base: f32,
    /// `\middle` delimiter size, while laying out a `\left…\right` body.
    middle: Option<f32>,
}

impl Ctx {
    /// A context at `size` px.
    pub fn new(size: f32, display: bool) -> Self {
        Self {
            style: if display {
                MathStyle::Display
            } else {
                MathStyle::Text
            },
            cramped: false,
            base: size,
            middle: None,
        }
    }

    fn size(&self, k: &Consts) -> f32 {
        match self.style {
            MathStyle::Display | MathStyle::Text => self.base,
            MathStyle::Script => self.base * k.script_pct / 100.0,
            MathStyle::ScriptScript => self.base * k.script_script_pct / 100.0,
        }
    }

    fn with(self, style: MathStyle, cramped: bool) -> Self {
        Self {
            style,
            cramped,
            ..self
        }
    }

    fn sup(self) -> Self {
        let s = match self.style {
            MathStyle::Display | MathStyle::Text => MathStyle::Script,
            _ => MathStyle::ScriptScript,
        };
        self.with(s, self.cramped)
    }

    fn sub(self) -> Self {
        self.sup().with(self.sup().style, true)
    }

    fn num(self) -> Self {
        let s = match self.style {
            MathStyle::Display => MathStyle::Text,
            MathStyle::Text => MathStyle::Script,
            _ => MathStyle::ScriptScript,
        };
        self.with(s, self.cramped)
    }

    fn den(self) -> Self {
        let n = self.num();
        n.with(n.style, true)
    }

    fn cramp(self) -> Self {
        self.with(self.style, true)
    }

    fn display(&self) -> bool {
        self.style == MathStyle::Display
    }

    fn tight(&self) -> bool {
        matches!(self.style, MathStyle::Script | MathStyle::ScriptScript)
    }
}

/// The layout engine for one formula.
pub struct Layout {
    k: Consts,
    /// Problems found while laying out.
    pub errors: Vec<String>,
    /// `\left…\right` groups already laid out, by node address and context:
    /// a body with `\middle` is laid out twice, which would make nesting cost
    /// 2^depth (an unclosed `\left` nests everything after it).
    left_right: std::collections::HashMap<(usize, MathStyle, bool, u32), Atom>,
}

#[derive(Debug, Clone)]
struct Atom {
    class: Class,
    b: MBox,
    /// A single glyph (scripts attach without baseline drops).
    is_char: bool,
    /// Limits are set above/below (display operators).
    limits: bool,
}

const RED: [u8; 3] = [220, 38, 38];

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    /// A layout engine reading the bundled font's MATH constants.
    pub fn new() -> Self {
        Self {
            k: consts(),
            errors: Vec::new(),
            left_right: std::collections::HashMap::new(),
        }
    }

    fn px(&self, units: f32, ctx: Ctx) -> f32 {
        units * ctx.size(&self.k) / self.k.upem
    }

    fn em(&self, ctx: Ctx) -> f32 {
        ctx.size(&self.k)
    }

    // ---- glyphs ----

    fn glyph_box(&self, face_id: u8, gid: GlyphId, size: f32) -> MBox {
        let Some(f) = font::face(face_id) else {
            return MBox::default();
        };
        let s = size / f32::from(f.units_per_em());
        let adv = f.glyph_hor_advance(gid).map_or(0.0, f32::from) * s;
        let (h, d) = f.glyph_bounding_box(gid).map_or((0.0, 0.0), |r| {
            (f32::from(r.y_max) * s, -f32::from(r.y_min) * s)
        });
        let (italic, accent_x) = if face_id == 0 {
            let gi = f.tables().math.and_then(|m| m.glyph_info);
            let it = gi
                .and_then(|g| g.italic_corrections)
                .and_then(|v| v.get(gid))
                .map_or(0.0, |v| f32::from(v.value) * s);
            let ax = gi
                .and_then(|g| g.top_accent_attachments)
                .and_then(|v| v.get(gid))
                .map(|v| f32::from(v.value) * s);
            (it, ax)
        } else {
            (0.0, None)
        };
        // Height and depth are the true ink extents (an over-brace glyph has a
        // negative depth); packing clamps them at the baseline.
        MBox {
            w: adv,
            h,
            d,
            italic,
            accent_x,
            items: vec![(
                0.0,
                0.0,
                Elem::Glyph {
                    face: face_id,
                    gid: gid.0,
                    size,
                },
            )],
        }
    }

    /// A character box; missing glyphs become a hollow box.
    fn char_box(&mut self, c: char, ctx: Ctx) -> MBox {
        let size = self.em(ctx);
        match font::lookup(c) {
            Some((f, g)) => self.glyph_box(f, g, size),
            None => {
                // U+25A1 □ as a visible placeholder (TH-006: never crash on missing glyphs).
                match font::lookup('□') {
                    Some((f, g)) => self.glyph_box(f, g, size),
                    None => MBox {
                        w: size * 0.5,
                        h: size * 0.7,
                        ..Default::default()
                    },
                }
            }
        }
    }

    fn text_box(&mut self, text: &str, ctx: Ctx, variant: Variant) -> MBox {
        let mut boxes = Vec::new();
        for c in text.chars() {
            let c = if c == '\u{a0}' { ' ' } else { c };
            let c = if variant == Variant::Upright {
                c
            } else {
                symbols::styled(c, variant)
            };
            let mut b = self.char_box(c, ctx);
            b.italic = 0.0;
            boxes.push(b);
        }
        let mut b = MBox::hpack(boxes);
        b.italic = 0.0;
        b
    }

    /// The vertical variant or assembly of `c` reaching `target` (h + d).
    fn stretch_v(&mut self, c: char, target: f32, ctx: Ctx) -> MBox {
        self.stretch_v_from(c, target, 0, ctx)
    }

    /// Like [`stretch_v`](Self::stretch_v), but never smaller than variant `min`
    /// (`\big` & co. pick successive size variants).
    fn stretch_v_from(&mut self, c: char, target: f32, min: usize, ctx: Ctx) -> MBox {
        let size = self.em(ctx);
        let Some((0, gid)) = font::lookup(c) else {
            return self.char_box(c, ctx);
        };
        let base = self.glyph_box(0, gid, size);
        if base.h + base.d >= target && min == 0 {
            return base;
        }
        let Some(f) = font::math_face() else {
            return base;
        };
        let s = size / self.k.upem;
        let Some(cons) = f
            .tables()
            .math
            .and_then(|m| m.variants)
            .and_then(|v| v.vertical_constructions.get(gid))
        else {
            return base;
        };
        let mut best = base;
        for (i, v) in cons.variants.into_iter().enumerate() {
            let b = self.glyph_box(0, v.variant_glyph, size);
            let total = b.h + b.d;
            best = b;
            if total >= target && i >= min {
                return best;
            }
        }
        let Some(asm) = cons.assembly else {
            return best;
        };
        let parts: Vec<_> = asm.parts.into_iter().collect();
        if parts.is_empty() {
            return best;
        }
        let overlap = self.k.min_overlap * s;
        let total_for = |n: usize| -> f32 {
            let mut t = 0.0;
            let mut count = 0;
            for p in &parts {
                let reps = if p.part_flags.extender() { n } else { 1 };
                t += f32::from(p.full_advance) * s * reps as f32;
                count += reps;
            }
            t - overlap * (count.saturating_sub(1)) as f32
        };
        let mut n = 0;
        while total_for(n) < target && n < 200 {
            n += 1;
        }
        // Stack the parts bottom to top.
        let mut out = MBox::default();
        let mut y = 0.0f32;
        let mut first = true;
        let mut w = 0.0f32;
        for p in &parts {
            let reps = if p.part_flags.extender() { n } else { 1 };
            for _ in 0..reps {
                if !first {
                    y -= overlap;
                }
                first = false;
                let g = self.glyph_box(0, p.glyph_id, size);
                w = w.max(g.w);
                // Align the part's bottom (bbox) with y.
                let shift = y + g.d;
                out.push(0.0, shift, g);
                y += f32::from(p.full_advance) * s;
            }
        }
        out.w = w;
        out.h = y;
        out.d = 0.0;
        out.italic = f32::from(asm.italics_correction.value) * s;
        out
    }

    /// The horizontal variant or assembly of `c` reaching `target` width.
    fn stretch_h(&mut self, c: char, target: f32, ctx: Ctx) -> MBox {
        let size = self.em(ctx);
        let Some((0, gid)) = font::lookup(c) else {
            return self.char_box(c, ctx);
        };
        let base = self.glyph_box(0, gid, size);
        if base.w >= target {
            return base;
        }
        let Some(f) = font::math_face() else {
            return base;
        };
        let s = size / self.k.upem;
        let Some(cons) = f
            .tables()
            .math
            .and_then(|m| m.variants)
            .and_then(|v| v.horizontal_constructions.get(gid))
        else {
            return base;
        };
        let mut best = base;
        for v in cons.variants {
            let b = self.glyph_box(0, v.variant_glyph, size);
            let w = b.w;
            best = b;
            if w >= target {
                return best;
            }
        }
        let Some(asm) = cons.assembly else {
            return best;
        };
        let parts: Vec<_> = asm.parts.into_iter().collect();
        if parts.is_empty() {
            return best;
        }
        let overlap = self.k.min_overlap * s;
        let total_for = |n: usize| -> f32 {
            let mut t = 0.0;
            let mut count = 0;
            for p in &parts {
                let reps = if p.part_flags.extender() { n } else { 1 };
                t += f32::from(p.full_advance) * s * reps as f32;
                count += reps;
            }
            t - overlap * (count.saturating_sub(1)) as f32
        };
        let mut n = 0;
        while total_for(n) < target && n < 400 {
            n += 1;
        }
        let mut out = MBox {
            h: f32::NEG_INFINITY,
            d: f32::NEG_INFINITY,
            ..Default::default()
        };
        let mut x = 0.0f32;
        let mut first = true;
        for p in &parts {
            let reps = if p.part_flags.extender() { n } else { 1 };
            for _ in 0..reps {
                if !first {
                    x -= overlap;
                }
                first = false;
                let g = self.glyph_box(0, p.glyph_id, size);
                out.h = out.h.max(g.h);
                out.d = out.d.max(g.d);
                out.push(x, 0.0, g);
                x += f32::from(p.full_advance) * s;
            }
        }
        if !out.h.is_finite() {
            out.h = 0.0;
            out.d = 0.0;
        }
        out.w = x;
        out
    }

    /// A delimiter of total size `target`, centered on the math axis.
    fn delim(&mut self, c: Option<char>, target: f32, ctx: Ctx) -> MBox {
        let Some(c) = c else {
            // Null delimiter: \nulldelimiterspace = 1.2pt.
            return MBox::kern(0.12 * self.em(ctx));
        };
        let b = self.stretch_v(c, target, ctx);
        let axis = self.px(self.k.axis, ctx);
        let center = (b.h - b.d) / 2.0;
        b.raised(axis - center)
    }

    // ---- lists ----

    /// Lays out a math list.
    pub fn list(&mut self, nodes: &[Node], ctx: Ctx) -> MBox {
        let atoms = self.atoms(nodes, ctx);
        self.pack(atoms, ctx)
    }

    fn atoms(&mut self, nodes: &[Node], ctx: Ctx) -> Vec<Atom> {
        let mut ctx = ctx;
        let mut out: Vec<Atom> = Vec::new();
        for n in nodes {
            if let Node::Style(s) = n {
                ctx = ctx.with(*s, ctx.cramped);
                continue;
            }
            if let Some(a) = self.atom(n, ctx) {
                out.push(a);
            }
        }
        out
    }

    /// Joins atoms with TeX inter-atom spacing (TeXbook p. 170).
    fn pack(&mut self, mut atoms: Vec<Atom>, ctx: Ctx) -> MBox {
        // Bin → Ord where a binary operator makes no sense (rules 5 and 6).
        for i in 0..atoms.len() {
            if atoms[i].class != Class::Bin {
                continue;
            }
            let prev = i.checked_sub(1).map(|p| atoms[p].class);
            let next = atoms.get(i + 1).map(|a| a.class);
            let bad_prev = matches!(
                prev,
                None | Some(Class::Bin | Class::Op | Class::Rel | Class::Open | Class::Punct)
            );
            let bad_next = matches!(next, None | Some(Class::Rel | Class::Close | Class::Punct));
            if bad_prev || bad_next {
                atoms[i].class = Class::Ord;
            }
        }
        let mu = self.em(ctx) / 18.0;
        let mut boxes = Vec::with_capacity(atoms.len() * 2);
        for i in 0..atoms.len() {
            if i > 0 {
                let s = space(atoms[i - 1].class, atoms[i].class, ctx.tight());
                if s > 0.0 {
                    boxes.push(MBox::kern(s * mu));
                }
            }
            boxes.push(std::mem::take(&mut atoms[i].b));
        }
        let mut b = MBox::hpack(boxes);
        b.italic = 0.0;
        b
    }

    fn ord(b: MBox) -> Atom {
        Atom {
            class: Class::Ord,
            b,
            is_char: false,
            limits: false,
        }
    }

    fn atom(&mut self, n: &Node, ctx: Ctx) -> Option<Atom> {
        Some(match n {
            Node::Sym { ch, class, variant } => {
                let c = if matches!(class, Class::Ord) || ch.is_alphanumeric() {
                    symbols::styled(*ch, *variant)
                } else {
                    *ch
                };
                let b = self.char_box(c, ctx);
                Atom {
                    class: *class,
                    b,
                    is_char: true,
                    limits: false,
                }
            }
            Node::BigOp {
                ch,
                limits,
                default_limits,
            } => {
                let b = if ctx.display() {
                    let min = self.px(self.k.display_op_min, ctx);
                    self.stretch_v(*ch, min, ctx)
                } else {
                    self.char_box(*ch, ctx)
                };
                let axis = self.px(self.k.axis, ctx);
                let italic = b.italic;
                let center = (b.h - b.d) / 2.0;
                let mut b = b.raised(axis - center);
                b.italic = italic;
                let lim = limits.unwrap_or(*default_limits && ctx.display());
                Atom {
                    class: Class::Op,
                    b,
                    is_char: false,
                    limits: lim,
                }
            }
            Node::OpName {
                name,
                limits,
                default_limits,
            } => {
                let b = self.text_box(name, ctx, Variant::Upright);
                let lim = limits.unwrap_or(*default_limits && ctx.display());
                Atom {
                    class: Class::Op,
                    b,
                    is_char: false,
                    limits: lim,
                }
            }
            Node::Text { text, bold, italic } => {
                let v = match (bold, italic) {
                    (true, true) => Variant::BoldItalic,
                    (true, false) => Variant::Bold,
                    (false, true) => Variant::Italic,
                    _ => Variant::Upright,
                };
                Self::ord(self.text_box(text, ctx, v))
            }
            Node::Group(l) => {
                let b = self.list(l, ctx);
                Atom {
                    class: Class::Ord,
                    b,
                    is_char: false,
                    limits: false,
                }
            }
            Node::Scripts { base, sup, sub } => {
                let base = self
                    .atom(base, ctx)
                    .unwrap_or_else(|| Self::ord(MBox::default()));
                self.scripts(base, sup.as_deref(), sub.as_deref(), ctx)
            }
            Node::Frac {
                num,
                den,
                rule,
                style,
                delims,
            } => {
                let ctx2 = match style {
                    Some(s) => ctx.with(*s, ctx.cramped),
                    None => ctx,
                };
                let b = self.frac(num, den, *rule, ctx2);
                let b = match delims {
                    Some((l, r)) => {
                        let target = self.delim_target(&b, ctx2);
                        let lb = self.delim(Some(*l).filter(|c| *c != ' '), target, ctx2);
                        let rb = self.delim(Some(*r).filter(|c| *c != ' '), target, ctx2);
                        MBox::hpack(vec![lb, b, rb])
                    }
                    None => b,
                };
                Atom {
                    class: Class::Inner,
                    b,
                    is_char: false,
                    limits: false,
                }
            }
            Node::Sqrt { index, body } => Self::ord(self.sqrt(index.as_deref(), body, ctx)),
            Node::LeftRight { left, body, right } => {
                // The result does not depend on an enclosing `\middle` size: the
                // probe measures with neutral `\middle`s.
                let key = (
                    std::ptr::from_ref(n) as usize,
                    ctx.style,
                    ctx.cramped,
                    ctx.base.to_bits(),
                );
                if let Some(a) = self.left_right.get(&key) {
                    return Some(a.clone());
                }
                // First pass to measure without `\middle`, then lay out with them.
                let probe = self.list(
                    body,
                    Ctx {
                        middle: None,
                        ..ctx
                    },
                );
                let target = self.delim_target(&probe, ctx);
                let inner = if body.iter().any(|n| matches!(n, Node::Middle(_))) {
                    let c2 = Ctx {
                        middle: Some(target),
                        ..ctx
                    };
                    self.list(body, c2)
                } else {
                    probe
                };
                let lb = self.delim(*left, target, ctx);
                let rb = self.delim(*right, target, ctx);
                let a = Atom {
                    class: Class::Inner,
                    b: MBox::hpack(vec![lb, inner, rb]),
                    is_char: false,
                    limits: false,
                };
                self.left_right.insert(key, a.clone());
                a
            }
            Node::Middle(c) => {
                let target = ctx.middle.unwrap_or_else(|| self.em(ctx));
                let b = self.delim(*c, target, ctx);
                Atom {
                    class: Class::Rel,
                    b,
                    is_char: false,
                    limits: false,
                }
            }
            Node::Big { ch, size, class } => {
                let em = self.em(Ctx {
                    style: MathStyle::Text,
                    ..ctx
                });
                let step = (*size as usize).clamp(1, 4);
                let target = em * [1.2, 1.8, 2.4, 3.0][step - 1];
                let b = match ch {
                    Some(c) => {
                        let g = self.stretch_v_from(*c, target, step, ctx);
                        let axis = self.px(self.k.axis, ctx);
                        let center = (g.h - g.d) / 2.0;
                        g.raised(axis - center)
                    }
                    None => self.delim(None, target, ctx),
                };
                Atom {
                    class: *class,
                    b,
                    is_char: false,
                    limits: false,
                }
            }
            Node::Accent {
                ch,
                body,
                stretchy,
                under,
            } => Self::ord(self.accent(*ch, body, *stretchy, *under, ctx)),
            Node::Overline(body) => {
                let b = self.list(body, ctx.cramp());
                let gap = self.px(self.k.overbar_gap, ctx);
                let t = self.px(self.k.overbar_rule, ctx);
                let extra = self.px(self.k.overbar_extra, ctx);
                let mut out = MBox {
                    w: b.w,
                    h: b.h + gap + t + extra,
                    d: b.d,
                    ..Default::default()
                };
                out.items
                    .push((0.0, b.h + gap, Elem::Rule { w: b.w, h: t }));
                out.push(0.0, 0.0, b);
                Self::ord(out)
            }
            Node::Underline(body) => {
                let b = self.list(body, ctx);
                let gap = self.px(self.k.underbar_gap, ctx);
                let t = self.px(self.k.underbar_rule, ctx);
                let extra = self.px(self.k.underbar_extra, ctx);
                let mut out = MBox {
                    w: b.w,
                    h: b.h,
                    d: b.d + gap + t + extra,
                    ..Default::default()
                };
                out.items
                    .push((0.0, -(b.d + gap + t), Elem::Rule { w: b.w, h: t }));
                out.push(0.0, 0.0, b);
                Self::ord(out)
            }
            Node::Brace { body, over } => {
                let b = self.list(body, ctx);
                let ch = if *over { '⏞' } else { '⏟' };
                let br = self.stretch_h(ch, b.w, ctx);
                let gap = 0.1 * self.em(ctx);
                let x = (b.w - br.w) / 2.0;
                let mut out = MBox {
                    w: b.w.max(br.w),
                    ..Default::default()
                };
                let bx = (out.w - b.w) / 2.0;
                if *over {
                    let y = b.h + gap + br.d;
                    out.h = y + br.h;
                    out.d = b.d;
                    out.push(x.max(0.0), y, br);
                } else {
                    let y = -(b.d + gap + br.h);
                    out.h = b.h;
                    out.d = -y + br.d;
                    out.push(x.max(0.0), y, br);
                }
                out.push(bx, 0.0, b);
                // `\overbrace` is a limits operator: its scripts are labels.
                Atom {
                    class: Class::Op,
                    b: out,
                    is_char: false,
                    limits: true,
                }
            }
            Node::Stack {
                base,
                over,
                under,
                class,
            } => Atom {
                class: *class,
                b: self.stack(base, over.as_deref(), under.as_deref(), ctx),
                is_char: false,
                limits: false,
            },
            Node::Array {
                kind,
                rows,
                cols,
                delims,
                small,
                vlines,
            } => {
                let b = self.array(*kind, rows, cols, *small, vlines, ctx);
                let b = if delims.0.is_some() || delims.1.is_some() {
                    let target = self.delim_target(&b, ctx);
                    let lb = self.delim(delims.0, target, ctx);
                    let rb = self.delim(delims.1, target, ctx);
                    MBox::hpack(vec![lb, b, rb])
                } else {
                    b
                };
                Atom {
                    class: Class::Inner,
                    b,
                    is_char: false,
                    limits: false,
                }
            }
            Node::Space(mu) => {
                let b = MBox::kern(mu * self.em(ctx) / 18.0);
                Atom {
                    class: Class::Ord,
                    b,
                    is_char: false,
                    limits: false,
                }
                .spacer()
            }
            Node::SpaceEm(em) => Atom {
                class: Class::Ord,
                b: MBox::kern(em * self.em(ctx)),
                is_char: false,
                limits: false,
            }
            .spacer(),
            Node::Color { rgb, body } => {
                let atoms = self.atoms(body, ctx);
                let class = if atoms.len() == 1 {
                    atoms[0].class
                } else {
                    Class::Ord
                };
                let b = self.pack(atoms, ctx);
                let (w, h, d) = (b.w, b.h, b.d);
                Atom {
                    class,
                    b: MBox {
                        w,
                        h,
                        d,
                        items: vec![(0.0, 0.0, Elem::Colored(*rgb, Box::new(b)))],
                        ..Default::default()
                    },
                    is_char: false,
                    limits: false,
                }
            }
            Node::Boxed(body) => {
                let b = self.list(body, ctx);
                let pad = 0.25 * self.em(ctx);
                let t = self.px(self.k.frac_rule, ctx);
                let w = b.w + 2.0 * pad;
                let h = b.h + pad;
                let d = b.d + pad;
                let mut out = MBox {
                    w,
                    h,
                    d,
                    ..Default::default()
                };
                out.items.push((0.0, 0.0, Elem::Frame { w, h, d, t }));
                out.push(pad, 0.0, b);
                Self::ord(out)
            }
            Node::Style(_) => return None,
            Node::Class { class, body } => {
                let b = self.list(body, ctx);
                Atom {
                    class: *class,
                    b,
                    is_char: false,
                    limits: *class == Class::Op && ctx.display(),
                }
            }
            Node::Phantom {
                body,
                width,
                height,
            } => {
                let b = self.list(body, ctx);
                Self::ord(MBox {
                    w: if *width { b.w } else { 0.0 },
                    h: if *height { b.h } else { 0.0 },
                    d: if *height { b.d } else { 0.0 },
                    ..Default::default()
                })
            }
            Node::Not(inner) => {
                let a = self
                    .atom(inner, ctx)
                    .unwrap_or_else(|| Self::ord(MBox::default()));
                let slash = self.char_box('⧸', ctx);
                let mut b = a.b.clone();
                let x = (b.w - slash.w) / 2.0;
                b.h = b.h.max(slash.h);
                b.d = b.d.max(slash.d);
                b.push(x, 0.0, slash);
                Atom {
                    class: a.class,
                    b,
                    is_char: false,
                    limits: false,
                }
            }
            Node::Error(src) => {
                let b = self.text_box(src, ctx, Variant::Upright);
                let (w, h, d) = (b.w, b.h, b.d);
                Self::ord(MBox {
                    w,
                    h,
                    d,
                    items: vec![(0.0, 0.0, Elem::Colored(RED, Box::new(b)))],
                    ..Default::default()
                })
            }
        })
    }

    /// The delimiter size for a body (TeX rule 19: \delimiterfactor 901,
    /// \delimitershortfall 5pt).
    fn delim_target(&self, b: &MBox, ctx: Ctx) -> f32 {
        let axis = self.px(self.k.axis, ctx);
        let delta = (b.h - axis).max(b.d + axis);
        let size = (2.0 * delta * 0.901).max(2.0 * delta - 0.5 * self.em(ctx));
        size.max(
            self.px(self.k.delimited_min, ctx)
                .min(2.0 * delta + 0.1 * self.em(ctx)),
        )
    }

    fn scripts(
        &mut self,
        base: Atom,
        sup: Option<&[Node]>,
        sub: Option<&[Node]>,
        ctx: Ctx,
    ) -> Atom {
        let sup_b = sup.map(|l| self.list(l, ctx.sup()));
        let sub_b = sub.map(|l| self.list(l, ctx.sub()));
        let k = self.k.clone();
        if base.limits {
            return Atom {
                class: base.class,
                b: self.limits(base.b, sup_b, sub_b, ctx),
                is_char: false,
                limits: true,
            };
        }
        let b = base.b;
        let is_op = base.class == Class::Op;
        let sup_ctx = ctx.sup();
        let mut out = MBox {
            w: b.w,
            h: b.h,
            d: b.d,
            ..Default::default()
        };
        let italic = b.italic;
        let (bw, bh, bd) = (b.w, b.h, b.d);
        out.push(0.0, 0.0, b);
        let mut sup_shift = 0.0;
        let mut sub_shift = 0.0;
        if let Some(s) = &sup_b {
            let drop = if base.is_char {
                0.0
            } else {
                bh - self.px(k.sup_baseline_drop_max, sup_ctx)
            };
            let std = self.px(
                if ctx.cramped {
                    k.sup_shift_up_cramped
                } else {
                    k.sup_shift_up
                },
                ctx,
            );
            sup_shift = drop.max(std).max(s.d + self.px(k.sup_bottom_min, ctx));
        }
        if let Some(s) = &sub_b {
            let drop = if base.is_char {
                0.0
            } else {
                bd + self.px(k.sub_baseline_drop_min, sup_ctx)
            };
            let std = self.px(k.sub_shift_down, ctx);
            sub_shift = drop.max(std);
            if sup_b.is_none() {
                sub_shift = sub_shift.max(s.h - self.px(k.sub_top_max, ctx));
            }
        }
        if let (Some(sp), Some(sb)) = (&sup_b, &sub_b) {
            let gap = (sup_shift - sp.d) - (sb.h - sub_shift);
            let min = self.px(k.sub_sup_gap_min, ctx);
            if gap < min {
                sub_shift += min - gap;
                let lift = self.px(k.sup_bottom_max_with_sub, ctx) - (sup_shift - sp.d);
                if lift > 0.0 {
                    sup_shift += lift;
                    sub_shift -= lift;
                }
            }
        }
        let space_after = self.px(k.space_after_script, ctx);
        let mut w = bw;
        if let Some(s) = sup_b {
            let x = bw + if is_op { 0.0 } else { italic };
            w = w.max(x + s.w);
            out.h = out.h.max(sup_shift + s.h);
            out.d = out.d.max(s.d - sup_shift);
            out.push(x, sup_shift, s);
        }
        if let Some(s) = sub_b {
            let x = bw - if is_op { italic } else { 0.0 };
            w = w.max(x + s.w);
            out.d = out.d.max(sub_shift + s.d);
            out.h = out.h.max(s.h - sub_shift);
            out.push(x, -sub_shift, s);
        }
        out.w = w + space_after;
        Atom {
            class: base.class,
            b: out,
            is_char: false,
            limits: false,
        }
    }

    fn limits(&mut self, b: MBox, sup: Option<MBox>, sub: Option<MBox>, ctx: Ctx) -> MBox {
        let k = &self.k;
        let w =
            b.w.max(sup.as_ref().map_or(0.0, |s| s.w))
                .max(sub.as_ref().map_or(0.0, |s| s.w));
        let italic = b.italic;
        let mut out = MBox {
            w,
            h: b.h,
            d: b.d,
            ..Default::default()
        };
        let (bh, bd, bw) = (b.h, b.d, b.w);
        if let Some(s) = sup {
            let gap = self
                .px(k.upper_limit_gap_min, ctx)
                .max(self.px(k.upper_limit_rise_min, ctx) - s.d);
            let y = bh + gap + s.d;
            out.h = y + s.h;
            out.push((w - s.w) / 2.0 + italic / 2.0, y, s);
        }
        if let Some(s) = sub {
            let gap = self
                .px(k.lower_limit_gap_min, ctx)
                .max(self.px(k.lower_limit_drop_min, ctx) - s.h);
            let y = -(bd + gap + s.h);
            out.d = -y + s.d;
            out.push((w - s.w) / 2.0 - italic / 2.0, y, s);
        }
        out.push((w - bw) / 2.0, 0.0, b);
        out
    }

    fn frac(&mut self, num: &[Node], den: &[Node], rule: bool, ctx: Ctx) -> MBox {
        let k = self.k.clone();
        let n = self.list(num, ctx.num());
        let d = self.list(den, ctx.den());
        let axis = self.px(k.axis, ctx);
        let disp = ctx.display();
        let pad = 0.12 * self.em(ctx);
        let w = n.w.max(d.w) + 2.0 * pad;
        let mut out = MBox {
            w,
            ..Default::default()
        };
        let (up, down) = if rule {
            let t = self.px(k.frac_rule, ctx);
            let mut up = self.px(
                if disp {
                    k.frac_num_display_shift_up
                } else {
                    k.frac_num_shift_up
                },
                ctx,
            );
            let mut down = self.px(
                if disp {
                    k.frac_den_display_shift_down
                } else {
                    k.frac_den_shift_down
                },
                ctx,
            );
            let num_gap = self.px(
                if disp {
                    k.frac_num_display_gap_min
                } else {
                    k.frac_num_gap_min
                },
                ctx,
            );
            let den_gap = self.px(
                if disp {
                    k.frac_den_display_gap_min
                } else {
                    k.frac_den_gap_min
                },
                ctx,
            );
            let g = (up - n.d) - (axis + t / 2.0);
            if g < num_gap {
                up += num_gap - g;
            }
            let g = (axis - t / 2.0) - (d.h - down);
            if g < den_gap {
                down += den_gap - g;
            }
            out.items
                .push((0.0, axis - t / 2.0, Elem::Rule { w, h: t }));
            (up, down)
        } else {
            let mut up = self.px(
                if disp {
                    k.stack_top_display_shift_up
                } else {
                    k.stack_top_shift_up
                },
                ctx,
            );
            let mut down = self.px(
                if disp {
                    k.stack_bottom_display_shift_down
                } else {
                    k.stack_bottom_shift_down
                },
                ctx,
            );
            let min = self.px(
                if disp {
                    k.stack_display_gap_min
                } else {
                    k.stack_gap_min
                },
                ctx,
            );
            let g = (up - n.d) - (d.h - down);
            if g < min {
                up += (min - g) / 2.0;
                down += (min - g) / 2.0;
            }
            (up, down)
        };
        out.h = up + n.h;
        out.d = down + d.d;
        let nx = (w - n.w) / 2.0;
        let dx = (w - d.w) / 2.0;
        out.push(nx, up, n);
        out.push(dx, -down, d);
        out
    }

    fn sqrt(&mut self, index: Option<&[Node]>, body: &[Node], ctx: Ctx) -> MBox {
        let k = self.k.clone();
        let b = self.list(body, ctx.cramp());
        let t = self.px(k.radical_rule, ctx);
        let mut gap = self.px(
            if ctx.display() {
                k.radical_display_gap
            } else {
                k.radical_gap
            },
            ctx,
        );
        let extra = self.px(k.radical_extra, ctx);
        let target = b.h + b.d + gap + t;
        let rad = self.stretch_v('√', target, ctx);
        let total = rad.h + rad.d;
        if total > target {
            gap += (total - target) / 2.0;
        }
        // The radical's top aligns with the top of the rule.
        let rule_bottom = b.h + gap;
        let rad_shift = rule_bottom + t - rad.h;
        let rw = rad.w;
        let rad_d = rad.d - rad_shift;
        let mut out = MBox::default();
        let mut x = 0.0;
        if let Some(idx) = index {
            let ib = self.list(idx, ctx.with(MathStyle::ScriptScript, false));
            let before = self.px(k.radical_kern_before, ctx);
            let after = self.px(k.radical_kern_after, ctx);
            let raise = (rad.h + rad.d) * k.radical_degree_raise / 100.0 - rad_d;
            x = before;
            let iw = ib.w;
            out.h = out.h.max(raise + ib.h);
            out.push(x, raise, ib);
            x = (x + iw + after).max(0.0);
        }
        out.push(x, rad_shift, rad);
        x += rw;
        out.items
            .push((x, rule_bottom, Elem::Rule { w: b.w, h: t }));
        let bw = b.w;
        out.push(x, 0.0, b.clone());
        out.w = x + bw;
        out.h = out.h.max(rule_bottom + t + extra);
        out.d = rad_d.max(b.d);
        out
    }

    fn accent(&mut self, ch: char, body: &[Node], stretchy: bool, under: bool, ctx: Ctx) -> MBox {
        let b = self.list(body, ctx.cramp());
        // A single-glyph body keeps its accent attachment point.
        let single = body.len() == 1 && matches!(body[0], Node::Sym { .. });
        let base_attach = if single {
            let probe = self.atom(&body[0], ctx.cramp()).map(|a| a.b.accent_x);
            probe.flatten().unwrap_or(b.w / 2.0)
        } else {
            b.w / 2.0
        };
        let is_arrow = matches!(ch, '→' | '←' | '↔' | '⇒' | '↦' | '↪' | '↩' | '⇐' | '⇔');
        let acc = if stretchy || is_arrow {
            let target = if body.is_empty() {
                1.5 * self.em(ctx)
            } else {
                b.w
            };
            self.stretch_h(ch, target, ctx)
        } else {
            self.char_box(ch, ctx)
        };
        let acc_attach = acc.accent_x.unwrap_or(acc.w / 2.0);
        let mut out = MBox {
            w: b.w.max(acc.w),
            h: b.h,
            d: b.d,
            ..Default::default()
        };
        let bx = (out.w - b.w) / 2.0;
        if under {
            let gap = 0.08 * self.em(ctx);
            let y = -(b.d + gap + acc.h);
            out.d = out.d.max(-y + acc.d);
            out.push(bx + base_attach - acc_attach, y, acc);
        } else if is_arrow || matches!(ch, '⏠' | '⏜') {
            // Arrows and parens sit on top with a small gap.
            let gap = 0.08 * self.em(ctx);
            let y = b.h + gap + acc.d;
            out.h = y + acc.h;
            out.push(bx + base_attach - acc_attach, y, acc);
        } else {
            let abh = self.px(self.k.accent_base_height, ctx);
            let y = (b.h - abh).max(0.0);
            out.h = out.h.max(y + acc.h);
            out.push(bx + base_attach - acc_attach, y, acc);
        }
        out.push(bx, 0.0, b);
        out
    }

    fn stack(
        &mut self,
        base: &[Node],
        over: Option<&[Node]>,
        under: Option<&[Node]>,
        ctx: Ctx,
    ) -> MBox {
        let o = over.map(|l| self.list(l, ctx.sup()));
        let u = under.map(|l| self.list(l, ctx.sub()));
        // Extensible arrows (`\xrightarrow`) grow to fit their labels.
        let b = match base {
            [
                Node::Accent {
                    ch,
                    body,
                    stretchy: true,
                    ..
                },
            ] if body.is_empty() => {
                let need = o
                    .as_ref()
                    .map_or(0.0, |b| b.w)
                    .max(u.as_ref().map_or(0.0, |b| b.w))
                    + 0.6 * self.em(ctx);
                let arrow = self.stretch_h(*ch, need.max(1.5 * self.em(ctx)), ctx);
                let axis = self.px(self.k.axis, ctx);
                let c = (arrow.h - arrow.d) / 2.0;
                arrow.raised(axis - c)
            }
            _ => self.list(base, ctx),
        };
        let gap = 0.12 * self.em(ctx);
        let w =
            b.w.max(o.as_ref().map_or(0.0, |x| x.w))
                .max(u.as_ref().map_or(0.0, |x| x.w));
        let mut out = MBox {
            w,
            h: b.h,
            d: b.d,
            ..Default::default()
        };
        let (bh, bd, bw) = (b.h, b.d, b.w);
        if let Some(s) = o {
            let y = bh + gap + s.d;
            out.h = y + s.h;
            out.push((w - s.w) / 2.0, y, s);
        }
        if let Some(s) = u {
            let y = -(bd + gap + s.h);
            out.d = -y + s.d;
            out.push((w - s.w) / 2.0, y, s);
        }
        out.push((w - bw) / 2.0, 0.0, b);
        out
    }

    fn array(
        &mut self,
        kind: ArrayKind,
        rows: &[Vec<Vec<Node>>],
        cols: &[ColAlign],
        small: bool,
        vlines: &[usize],
        ctx: Ctx,
    ) -> MBox {
        let cell_ctx = match kind {
            ArrayKind::Aligned | ArrayKind::Gathered => ctx.with(MathStyle::Display, ctx.cramped),
            _ if small => ctx.with(MathStyle::Script, ctx.cramped),
            _ => ctx.with(MathStyle::Text, ctx.cramped),
        };
        let ncols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut cells: Vec<Vec<MBox>> = Vec::new();
        for row in rows {
            let mut r = Vec::new();
            for (ci, cell) in row.iter().enumerate() {
                // In aligned environments the left column of each pair starts with
                // an empty group so a leading relation keeps its spacing.
                let b = if kind == ArrayKind::Aligned && ci % 2 == 1 {
                    let mut l = vec![Node::Group(Vec::new())];
                    l.extend(cell.iter().cloned());
                    self.list(&l, cell_ctx)
                } else {
                    self.list(cell, cell_ctx)
                };
                r.push(b);
            }
            cells.push(r);
        }
        let em = self.em(cell_ctx);
        let mut col_w = vec![0.0f32; ncols];
        for r in &cells {
            for (i, c) in r.iter().enumerate() {
                col_w[i] = col_w[i].max(c.w);
            }
        }
        let strut_h = 0.7 * em;
        let strut_d = 0.3 * em;
        let row_gap = match kind {
            ArrayKind::Aligned | ArrayKind::Gathered => 0.35 * em,
            _ if small => 0.1 * em,
            _ => 0.2 * em,
        };
        let col_gap = |i: usize| -> f32 {
            match kind {
                ArrayKind::Aligned => {
                    if i % 2 == 1 {
                        0.0
                    } else {
                        2.0 * em
                    }
                }
                ArrayKind::Cases => 1.0 * em,
                _ if small => 0.5 * em,
                _ => 1.0 * em,
            }
        };
        let align = |i: usize| -> ColAlign {
            match kind {
                ArrayKind::Aligned => {
                    if i.is_multiple_of(2) {
                        ColAlign::Right
                    } else {
                        ColAlign::Left
                    }
                }
                ArrayKind::Cases => ColAlign::Left,
                ArrayKind::Array => cols.get(i).copied().unwrap_or(ColAlign::Center),
                _ => ColAlign::Center,
            }
        };
        let mut col_x = Vec::with_capacity(ncols);
        let mut x = 0.0;
        for (i, w) in col_w.iter().enumerate() {
            if i > 0 {
                x += col_gap(i);
            }
            col_x.push(x);
            x += w;
        }
        let total_w = x;
        // Rows from the top.
        let mut out = MBox {
            w: total_w,
            ..Default::default()
        };
        let mut y = 0.0f32; // top of the current row, going down (negative)
        let mut row_boxes = Vec::new();
        for (ri, r) in cells.into_iter().enumerate() {
            let h = r.iter().map(|c| c.h).fold(strut_h, f32::max);
            let d = r.iter().map(|c| c.d).fold(strut_d, f32::max);
            if ri > 0 {
                y -= row_gap;
            }
            let baseline = y - h;
            for (ci, c) in r.into_iter().enumerate() {
                let slack = col_w[ci] - c.w;
                let cx = col_x[ci]
                    + match align(ci) {
                        ColAlign::Left => 0.0,
                        ColAlign::Center => slack / 2.0,
                        ColAlign::Right => slack,
                    };
                row_boxes.push((cx, baseline, c));
            }
            y = baseline - d;
        }
        let height = -y;
        // Center vertically on the axis.
        let axis = self.px(self.k.axis, ctx);
        let shift = axis + height / 2.0;
        for (cx, by, c) in row_boxes {
            out.push(cx, by + shift, c);
        }
        // Vertical rules between columns (`array{c|c}`).
        let t = self.px(self.k.frac_rule, ctx);
        for &i in vlines {
            let x = if i == 0 {
                0.0
            } else if i >= ncols {
                total_w
            } else {
                (col_x[i - 1] + col_w[i - 1] + col_x[i]) / 2.0
            };
            out.items
                .push((x - t / 2.0, shift - height, Elem::Rule { w: t, h: height }));
        }
        out.h = shift;
        out.d = height - shift;
        out
    }
}

impl Atom {
    /// Explicit spaces take no inter-atom spacing of their own.
    fn spacer(mut self) -> Self {
        self.class = Class::Ord;
        self
    }
}

/// Inter-atom spacing in mu (TeXbook table, chapter 18). Negative entries
/// (TeX's parenthesized ones) apply in display and text styles only.
fn space(left: Class, right: Class, tight: bool) -> f32 {
    const T: [[i8; 8]; 8] = [
        //Ord Op Bin Rel Open Close Punct Inner
        [0, 1, -2, -3, 0, 0, 0, -1],     // Ord
        [1, 1, 0, -3, 0, 0, 0, -1],      // Op
        [-2, -2, 0, 0, -2, 0, 0, -2],    // Bin
        [-3, -3, 0, 0, -3, 0, 0, -3],    // Rel
        [0, 0, 0, 0, 0, 0, 0, 0],        // Open
        [0, 1, -2, -3, 0, 0, 0, -1],     // Close
        [-1, -1, 0, -1, -1, -1, -1, -1], // Punct
        [-1, 1, -2, -3, -1, 0, -1, -1],  // Inner
    ];
    let idx = |c: Class| match c {
        Class::Ord => 0,
        Class::Op => 1,
        Class::Bin => 2,
        Class::Rel => 3,
        Class::Open => 4,
        Class::Close => 5,
        Class::Punct => 6,
        Class::Inner => 7,
    };
    let code = T[idx(left)][idx(right)];
    let mu = match code.abs() {
        1 => 3.0,
        2 => 4.0,
        3 => 5.0,
        _ => 0.0,
    };
    if code < 0 && tight { 0.0 } else { mu }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_table() {
        assert_eq!(space(Class::Ord, Class::Rel, false), 5.0);
        assert_eq!(space(Class::Ord, Class::Rel, true), 0.0);
        assert_eq!(space(Class::Ord, Class::Bin, false), 4.0);
        assert_eq!(space(Class::Ord, Class::Op, true), 3.0);
        assert_eq!(space(Class::Open, Class::Ord, false), 0.0);
        assert_eq!(space(Class::Punct, Class::Ord, false), 3.0);
    }
}
